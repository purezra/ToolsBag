use crate::error::{AppError, AppResult};
use crate::models::{VideoInfoImportResponse, VideoInfoItem};
use crate::tools::{detect_media_type_from_path, mediainfo, MediaType};
use crate::utils::emit_progress;
use exif;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;
use walkdir::WalkDir;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const MAX_SCAN_FILES: usize = 50_000;
const MAX_SCAN_DEPTH: usize = 32;

type VideoProbeResult = (
    Option<f64>,
    Option<u32>,
    Option<u32>,
    Option<f64>,
    Option<String>,
    Option<String>,
    Option<String>,
);
type ImageProbeResult = (
    Option<u32>,
    Option<u32>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub media_type: String,
    pub duration_sec: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bitrate_mbps: Option<f64>,
    pub codec: Option<String>,
    pub frame_rate: Option<String>,
    pub device: Option<String>,
    pub taken_at: Option<String>,
    pub focal_length: Option<String>,
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfoStatus {
    pub available: bool,
    pub path: Option<String>,
}

/// 外部工具状态
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalToolStatus {
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatCount {
    pub ext: String,
    pub count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStats {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub format_counts: Vec<FormatCount>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResponse {
    pub items: Vec<MediaItem>,
    pub stats: ImportStats,
}

#[tauri::command]
pub fn get_mediainfo_status() -> MediaInfoStatus {
    let available = mediainfo::init_mediainfo();
    let path = if available {
        Some(mediainfo::get_mediainfo_path().unwrap_or_default())
    } else {
        None
    };
    MediaInfoStatus { available, path }
}

/// 检测 ffprobe 是否可用及版本
#[tauri::command]
pub fn check_ffprobe_status() -> ExternalToolStatus {
    let mut cmd = Command::new("ffprobe");
    cmd.args(["-version"]);

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    match cmd.output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // 解析版本号，格式如 "ffprobe version 6.0-full_build-www.gyan.dev ..."
            let version = stdout.lines().next().and_then(|line| {
                if line.contains("version") {
                    line.split_whitespace()
                        .skip_while(|s| *s != "version")
                        .nth(1)
                        .map(|v| v.to_string())
                } else {
                    None
                }
            });

            // 尝试获取路径
            let path = get_command_path("ffprobe");

            ExternalToolStatus {
                name: "ffprobe".into(),
                available: true,
                version,
                path,
            }
        }
        _ => ExternalToolStatus {
            name: "ffprobe".into(),
            available: false,
            version: None,
            path: None,
        },
    }
}

/// 检测 exiftool 是否可用及版本
#[tauri::command]
pub fn check_exiftool_status() -> ExternalToolStatus {
    let mut cmd = Command::new("exiftool");
    cmd.args(["-ver"]);

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    match cmd.output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let version = stdout.trim().to_string();
            let path = get_command_path("exiftool");

            ExternalToolStatus {
                name: "exiftool".into(),
                available: true,
                version: if version.is_empty() {
                    None
                } else {
                    Some(version)
                },
                path,
            }
        }
        _ => ExternalToolStatus {
            name: "exiftool".into(),
            available: false,
            version: None,
            path: None,
        },
    }
}

/// 获取命令的完整路径
fn get_command_path(cmd_name: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("where");
        cmd.arg(cmd_name);
        cmd.creation_flags(CREATE_NO_WINDOW);

        if let Ok(output) = cmd.output() {
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .map(|s| s.to_string());
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let cmd = Command::new("which").arg(cmd_name).output();
        if let Ok(output) = cmd {
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .to_string()
                    .into();
            }
        }
    }

    None
}

#[tauri::command]
pub fn select_media_paths(app: AppHandle, kind: String) -> Result<Vec<String>, String> {
    let picked = match kind.as_str() {
        "folder" => app.dialog().file().blocking_pick_folders(),
        _ => app.dialog().file().blocking_pick_files(),
    };
    let list = picked
        .unwrap_or_default()
        .into_iter()
        .map(|p| match p {
            tauri_plugin_dialog::FilePath::Path(pb) => pb.to_string_lossy().to_string(),
            tauri_plugin_dialog::FilePath::Url(url) => url.to_string(),
        })
        .collect();
    Ok(list)
}

#[tauri::command]
pub fn read_clipboard_paths(app: AppHandle) -> Result<Vec<String>, String> {
    let mgr = app.clipboard();
    match mgr.read_text() {
        Ok(text) => {
            let paths: Vec<String> = text
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if paths.is_empty() {
                Err("剪贴板未包含路径".to_string())
            } else {
                Ok(paths)
            }
        }
        Err(e) => Err(format!("读取剪贴板失败: {}", e)),
    }
}

#[tauri::command]
pub async fn import_media(
    app: AppHandle,
    paths: Vec<String>,
    recursive: bool,
) -> Result<ImportResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        import_media_inner(&app, paths, recursive).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn import_media_inner(
    app: &AppHandle,
    paths: Vec<String>,
    recursive: bool,
) -> AppResult<ImportResponse> {
    if paths.is_empty() {
        return Ok(ImportResponse {
            items: vec![],
            stats: ImportStats {
                total: 0,
                success: 0,
                failed: 0,
                format_counts: vec![],
            },
        });
    }

    let collected = collect_media_paths(&paths, recursive)?;
    let id = Uuid::new_v4();
    emit_progress(app, "media_import", 0, collected.len().max(1), "start", id);

    let format_counts = build_format_counts(&collected);
    let progress = AtomicUsize::new(0);

    let items: Vec<MediaItem> = collected
        .par_iter()
        .enumerate()
        .map(|(idx, path)| {
            let item = match build_media_item(idx as u64, path) {
                Ok(item) => item,
                Err(err) => MediaItem {
                    id: idx as u64,
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    path: path.to_string_lossy().to_string(),
                    size: 0,
                    media_type: detect_media_type_from_path(path)
                        .map(|t| t.as_str().to_string())
                        .unwrap_or_else(|| "unknown".into()),
                    duration_sec: None,
                    width: None,
                    height: None,
                    bitrate_mbps: None,
                    codec: None,
                    frame_rate: None,
                    device: None,
                    taken_at: None,
                    focal_length: None,
                    status: "error".into(),
                    reason: Some(err.to_string()),
                },
            };

            let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
            if done == collected.len() || done % 20 == 0 {
                emit_progress(
                    app,
                    "media_import",
                    done,
                    collected.len().max(1),
                    "processing",
                    id,
                );
            }

            item
        })
        .collect();

    emit_progress(
        app,
        "media_import",
        collected.len(),
        collected.len().max(1),
        "done",
        id,
    );

    let success = items.iter().filter(|item| item.status == "success").count();
    let failed = items.len().saturating_sub(success);

    Ok(ImportResponse {
        items,
        stats: ImportStats {
            total: collected.len(),
            success,
            failed,
            format_counts,
        },
    })
}

fn collect_media_paths(input: &[String], recursive: bool) -> AppResult<Vec<PathBuf>> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();

    let mut push_unique = |path: PathBuf| -> AppResult<()> {
        if seen.insert(path.clone()) {
            result.push(path);
            if result.len() > MAX_SCAN_FILES {
                return Err(AppError::InvalidArgument(format!(
                    "扫描文件数超过上限（{}），请缩小范围后重试",
                    MAX_SCAN_FILES
                )));
            }
        }
        Ok(())
    };

    for path_str in input {
        let p = PathBuf::from(path_str);
        if !p.exists() {
            continue;
        }
        if p.is_file() {
            if detect_media_type_from_path(&p).is_some() {
                push_unique(p)?;
            }
        } else if p.is_dir() {
            if recursive {
                for entry in WalkDir::new(&p)
                    .max_depth(MAX_SCAN_DEPTH + 1)
                    .follow_links(false)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                {
                    if detect_media_type_from_path(entry.path()).is_some() {
                        push_unique(entry.path().to_path_buf())?;
                    }
                }
            } else if let Ok(read_dir) = std::fs::read_dir(&p) {
                for entry in read_dir.flatten() {
                    let path = entry.path();
                    if path.is_file() && detect_media_type_from_path(&path).is_some() {
                        push_unique(path)?;
                    }
                }
            }
        }
    }
    Ok(result)
}

fn build_format_counts(paths: &[PathBuf]) -> Vec<FormatCount> {
    let mut map: HashMap<String, usize> = HashMap::new();
    for path in paths {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let key = ext.to_ascii_lowercase();
            *map.entry(key).or_insert(0) += 1;
        }
    }
    let mut list: Vec<FormatCount> = map
        .into_iter()
        .map(|(ext, count)| FormatCount { ext, count })
        .collect();
    list.sort_by(|a, b| a.ext.cmp(&b.ext));
    list
}

fn build_media_item(idx: u64, path: &Path) -> AppResult<MediaItem> {
    let media_type = detect_media_type_from_path(path)
        .ok_or_else(|| AppError::UnsupportedFormat(path.to_string_lossy().to_string()))?;
    let md = std::fs::metadata(path)?;
    let size = md.len();
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    if media_type == MediaType::Video {
        let (duration, width, height, bitrate, codec, frame_rate, err) = probe_video(path);
        return Ok(MediaItem {
            id: idx,
            name,
            path: path.to_string_lossy().to_string(),
            size,
            media_type: media_type.as_str().to_string(),
            duration_sec: duration,
            width,
            height,
            bitrate_mbps: bitrate,
            codec,
            frame_rate,
            device: None,
            taken_at: None,
            focal_length: None,
            status: "success".to_string(),
            reason: err,
        });
    }

    let (width, height, device, taken_at, focal_length, err) = probe_image(path);
    Ok(MediaItem {
        id: idx,
        name,
        path: path.to_string_lossy().to_string(),
        size,
        media_type: media_type.as_str().to_string(),
        duration_sec: None,
        width,
        height,
        bitrate_mbps: None,
        codec: None,
        frame_rate: None,
        device,
        taken_at,
        focal_length,
        status: "success".to_string(),
        reason: err,
    })
}

fn probe_video(path: &Path) -> VideoProbeResult {
    // 优先使用 MediaInfo
    if mediainfo::is_mediainfo_available() {
        if let Some(meta) = mediainfo::get_video_meta(path) {
            let duration = if meta.duration_ms > 0 {
                Some(meta.duration_ms as f64 / 1000.0)
            } else {
                None
            };
            let bitrate = if meta.bitrate_raw > 0 {
                Some(meta.bitrate_raw as f64 / 1_000_000.0)
            } else {
                None
            };
            let frame_rate = if meta.frame_rate.is_empty() || meta.frame_rate == "-" {
                None
            } else {
                Some(meta.frame_rate)
            };
            return (
                duration,
                Some(meta.width),
                Some(meta.height),
                bitrate,
                Some(meta.codec),
                frame_rate,
                None,
            );
        }
    }

    // 回退到 ffprobe
    probe_video_ffprobe(path)
}

fn probe_video_ffprobe(path: &Path) -> VideoProbeResult {
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height,bit_rate,codec_name,r_frame_rate",
        "-show_entries",
        "format=duration",
        "-of",
        "json",
        path.to_string_lossy().as_ref(),
    ]);

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return (
                None,
                None,
                None,
                None,
                None,
                None,
                Some(format!("ffprobe 不可用: {}", e)),
            )
        }
    };

    if !output.status.success() {
        return (
            None,
            None,
            None,
            None,
            None,
            None,
            Some(String::from_utf8_lossy(&output.stderr).to_string()),
        );
    }

    #[derive(serde::Deserialize)]
    struct FfFormat {
        duration: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct FfStream {
        width: Option<u32>,
        height: Option<u32>,
        bit_rate: Option<String>,
        codec_name: Option<String>,
        r_frame_rate: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Ffprobe {
        streams: Option<Vec<FfStream>>,
        format: Option<FfFormat>,
    }

    let parsed: Ffprobe = serde_json::from_slice(&output.stdout)
        .map_err(|e| e.to_string())
        .unwrap_or(Ffprobe {
            streams: None,
            format: None,
        });

    let stream = parsed.streams.as_ref().and_then(|s| s.first());
    let width = stream.and_then(|s| s.width);
    let height = stream.and_then(|s| s.height);
    let bitrate = stream
        .and_then(|s| s.bit_rate.as_ref())
        .and_then(|s| s.parse::<f64>().ok())
        .map(|b| b / 1_000_000.0);
    let codec = stream.and_then(|s| s.codec_name.clone());
    let frame_rate = stream.and_then(|s| s.r_frame_rate.clone()).and_then(|fr| {
        // 解析 "30/1" 或 "30000/1001" 格式
        let parts: Vec<&str> = fr.split('/').collect();
        if parts.len() == 2 {
            let num: f64 = parts[0].parse().ok()?;
            let den: f64 = parts[1].parse().ok()?;
            if den > 0.0 {
                let fps = num / den;
                // 格式化：整数不显示小数，否则保留2位
                if fps == fps.floor() {
                    Some(format!("{}", fps as u32))
                } else {
                    let formatted = format!("{:.2}", fps);
                    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
                    Some(trimmed.to_string())
                }
            } else {
                None
            }
        } else {
            None
        }
    });
    let duration = parsed
        .format
        .and_then(|f| f.duration)
        .and_then(|d| d.parse::<f64>().ok());

    (duration, width, height, bitrate, codec, frame_rate, None)
}

fn probe_image(path: &Path) -> ImageProbeResult {
    let mut cmd = Command::new("exiftool");
    cmd.args([
        "-json",
        "-n",
        "-ImageWidth",
        "-ImageHeight",
        "-Make",
        "-Model",
        "-DateTimeOriginal",
        "-FocalLength",
        "-FileSize#",
        path.to_string_lossy().as_ref(),
    ]);

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    match cmd.output() {
        Ok(output) if output.status.success() => {
            let parsed: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap_or_default();
            if let Some(first) = parsed.first() {
                let width = first
                    .get("ImageWidth")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let height = first
                    .get("ImageHeight")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let make = first
                    .get("Make")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let model = first
                    .get("Model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let device = match (make.is_empty(), model.is_empty()) {
                    (false, false) => Some(format!("{} {}", make, model)),
                    (false, true) => Some(make.to_string()),
                    (true, false) => Some(model.to_string()),
                    _ => None,
                };
                let taken_at = first
                    .get("DateTimeOriginal")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let focal = first
                    .get("FocalLength")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                (width, height, device, taken_at, focal, None)
            } else {
                probe_image_fallback(path)
            }
        }
        Ok(output) => {
            let fallback_err = String::from_utf8_lossy(&output.stderr).to_string();
            let (w, h, device, taken_at, focal, inner_err) = probe_image_fallback(path);
            (
                w,
                h,
                device,
                taken_at,
                focal,
                inner_err.or(Some(fallback_err)),
            )
        }
        Err(e) => {
            let (w, h, device, taken_at, focal, inner_err) = probe_image_fallback(path);
            let reason = if inner_err.is_some() {
                inner_err
            } else {
                Some(format!("exiftool 不可用: {}", e))
            };
            (w, h, device, taken_at, focal, reason)
        }
    }
}

fn probe_image_fallback(path: &Path) -> ImageProbeResult {
    let dims = image::image_dimensions(path);
    let (width, height) = match dims {
        Ok((w, h)) => (Some(w), Some(h)),
        Err(e) => {
            return (
                None,
                None,
                None,
                None,
                None,
                Some(format!("读取尺寸失败: {}", e)),
            )
        }
    };

    let exif_data = read_exif(path);
    (width, height, exif_data.0, exif_data.1, exif_data.2, None)
}

fn read_exif(path: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let file = File::open(path);
    let file = match file {
        Ok(f) => f,
        Err(_) => return (None, None, None),
    };
    let mut bufreader = BufReader::new(file);
    if let Ok(exifreader) = exif::Reader::new().read_from_container(&mut bufreader) {
        let make = exifreader.get_field(exif::Tag::Make, exif::In::PRIMARY);
        let model = exifreader.get_field(exif::Tag::Model, exif::In::PRIMARY);
        let device = match (make, model) {
            (Some(mk), Some(md)) => Some(format!("{} {}", mk.display_value(), md.display_value())),
            (Some(mk), None) => Some(mk.display_value().to_string()),
            (None, Some(md)) => Some(md.display_value().to_string()),
            _ => None,
        };
        let datetime = exifreader
            .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
            .map(|v| v.display_value().to_string());
        let focal = exifreader
            .get_field(exif::Tag::FocalLength, exif::In::PRIMARY)
            .map(|v| v.display_value().to_string());
        return (device, datetime, focal);
    }
    (None, None, None)
}

// ==================== 视频体检 - 导入命令 ====================

#[tauri::command]
pub async fn import_detailed_video_info(
    app: AppHandle,
    paths: Vec<String>,
    recursive: bool,
) -> Result<VideoInfoImportResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        import_detailed_video_info_inner(&app, paths, recursive).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn import_detailed_video_info_inner(
    app: &AppHandle,
    paths: Vec<String>,
    recursive: bool,
) -> AppResult<VideoInfoImportResponse> {
    if paths.is_empty() {
        return Ok(VideoInfoImportResponse {
            items: vec![],
            total: 0,
            success: 0,
            failed: 0,
        });
    }

    // 只收集视频文件
    let collected = collect_media_paths(&paths, recursive)?;
    let video_paths: Vec<PathBuf> = collected
        .into_iter()
        .filter(|p| detect_media_type_from_path(p) == Some(MediaType::Video))
        .collect();

    let total = video_paths.len();
    let id = Uuid::new_v4();
    emit_progress(app, "video_info_import", 0, total.max(1), "start", id);

    let progress = AtomicUsize::new(0);

    let items: Vec<VideoInfoItem> = video_paths
        .par_iter()
        .enumerate()
        .map(|(idx, path)| {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".into());
            let md = std::fs::metadata(path).ok();
            let size = md.map(|m| m.len()).unwrap_or(0);

            let detail = mediainfo::get_detailed_video_meta(path);

            let item = VideoInfoItem {
                id: idx as u64,
                name,
                path: path.to_string_lossy().to_string(),
                size,
                status: if detail.is_some() { "success" } else { "error" }.to_string(),
                reason: if detail.is_none() {
                    Some("无法解析视频元数据".to_string())
                } else {
                    None
                },
                detail,
            };

            let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
            if done == total || done % 10 == 0 {
                emit_progress(
                    app,
                    "video_info_import",
                    done,
                    total.max(1),
                    "processing",
                    id,
                );
            }

            item
        })
        .collect();

    emit_progress(app, "video_info_import", total, total.max(1), "done", id);

    let success = items.iter().filter(|i| i.status == "success").count();
    let failed = items.len().saturating_sub(success);

    Ok(VideoInfoImportResponse {
        items,
        total,
        success,
        failed,
    })
}
