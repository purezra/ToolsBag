//! Portable self-update.
//!
//! The app is shipped as a single `ToolsBag-Portable.exe`. Updates follow the
//! same idea as magpie: check a release manifest, download the new exe inside
//! the app, then let the user press "restart to update" — no installer wizard.
//!
//! Source of truth: the latest GitHub Release of the project repository.
//! Expected asset name: `ToolsBag-Portable.exe` (uploaded next to the tag).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

const REPO: &str = "purezra/ToolsBag";
const ASSET_NAME: &str = "ToolsBag-Portable.exe";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub asset_url: Option<String>,
    pub asset_size: u64,
    pub sha256: Option<String>,
    pub published_at: Option<String>,
    /// Human-readable status for the UI (e.g. "no release yet").
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(format!("ToolsBag/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("初始化网络客户端失败: {e}"))
}

/// Parse `v1.2.3` / `1.2.3-beta` into numeric components.
fn version_parts(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0)
        })
        .collect()
}

fn is_newer(latest: &str, current: &str) -> bool {
    let a = version_parts(latest);
    let b = version_parts(current);
    let len = a.len().max(b.len());
    for i in 0..len {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        if av != bv {
            return av > bv;
        }
    }
    false
}

#[tauri::command]
pub async fn check_update() -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = client()?
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("无法连接更新服务: {e}"))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(UpdateInfo {
            current,
            latest: String::new(),
            available: false,
            notes: String::new(),
            asset_url: None,
            asset_size: 0,
            sha256: None,
            published_at: None,
            message: Some("尚未发布任何版本".into()),
        });
    }

    if !resp.status().is_success() {
        return Err(format!("更新服务返回 {}", resp.status()));
    }

    let release: GhRelease = resp
        .json()
        .await
        .map_err(|e| format!("解析更新信息失败: {e}"))?;

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(ASSET_NAME))
        .ok_or_else(|| format!("最新版本缺少 {ASSET_NAME} 资产"))?;

    let latest = release.tag_name.trim_start_matches('v').to_string();
    let available = is_newer(&latest, &current);
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(|s| s.to_ascii_lowercase());

    Ok(UpdateInfo {
        current,
        latest,
        available,
        notes: release.body.unwrap_or_default(),
        asset_url: Some(asset.browser_download_url.clone()),
        asset_size: asset.size,
        sha256,
        published_at: release.published_at,
        message: None,
    })
}

fn is_pe_file(path: &Path) -> bool {
    let mut head = [0u8; 2];
    std::fs::File::open(path)
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head))
        .map(|_| &head == b"MZ")
        .unwrap_or(false)
}

#[tauri::command]
pub async fn download_update(
    app: AppHandle,
    url: String,
    version: String,
    sha256: Option<String>,
) -> Result<String, String> {
    let dir = std::env::temp_dir();
    let safe_version = version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let target = dir.join(format!("ToolsBag-{safe_version}.exe"));
    let staging = dir.join(format!(
        "ToolsBag-{safe_version}-{}.part",
        std::process::id()
    ));

    let mut resp = client()?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("下载更新失败: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("下载更新失败: HTTP {}", resp.status()));
    }

    let total = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&staging)
        .await
        .map_err(|e| format!("创建临时文件失败: {e}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("读取下载数据失败: {e}"))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("写入更新文件失败: {e}"))?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        let _ = app.emit(
            "update-progress",
            serde_json::json!({ "downloaded": downloaded, "total": total }),
        );
    }

    file.flush()
        .await
        .map_err(|e| format!("写入更新文件失败: {e}"))?;
    drop(file);

    if !is_pe_file(&staging) {
        let _ = std::fs::remove_file(&staging);
        return Err("下载的文件不是有效的 Windows 程序".into());
    }

    if let Some(expected) = sha256 {
        let actual = format!("{:x}", hasher.finalize());
        if !expected.eq_ignore_ascii_case(&actual) {
            let _ = std::fs::remove_file(&staging);
            return Err("更新文件校验失败（SHA-256 不匹配）".into());
        }
    }

    // Windows 不允许 rename 覆盖已存在的目标；先删掉旧缓存。
    let _ = std::fs::remove_file(&target);
    std::fs::rename(&staging, &target).map_err(|e| format!("保存更新文件失败: {e}"))?;

    Ok(target.to_string_lossy().into_owned())
}

/// Replace the running exe with the downloaded one, then restart.
#[tauri::command]
pub fn apply_update(app: AppHandle, path: String) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("开发模式不支持自更新，请使用打包后的便携版".into());
    }
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err("更新文件不存在，请重新下载".into());
    }

    // Stage next to the running exe so the final replace is a same-volume
    // rename (the download itself lives in %TEMP%, possibly another drive).
    let current = std::env::current_exe().map_err(|e| format!("定位当前程序失败: {e}"))?;
    let exe_dir = current
        .parent()
        .ok_or_else(|| "无法定位程序所在目录".to_string())?;
    let staged = exe_dir.join(format!(".toolsbag-update-{}.exe", std::process::id()));
    std::fs::copy(&path, &staged).map_err(|e| format!("准备更新文件失败: {e}"))?;

    self_replace::self_replace(&staged).map_err(|e| format!("替换程序失败: {e}"))?;
    app.restart();
}
