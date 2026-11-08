//! PDF 合并工具 - 后端核心
//!
//! 支持三类输入按任意顺序合并：
//! - PDF：按页原样拼接（保持原页面尺寸与内容）
//! - 图片：JPG/PNG/WebP/TIFF/BMP/GIF 等，解码后以 JPEG 单页嵌入
//! - Word：DOC/DOCX，先调用本机 LibreOffice 或 Microsoft Word 转成 PDF，
//!   再按用户输入的页码（如 `1,5,7` / `1-3,5`）挑选页面
//!
//! 文档转换结果按「路径 + 大小 + 修改时间」缓存到应用缓存目录，重复导入/合并
//! 不会反复启动 Office。

use image::{DynamicImage, RgbImage};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// PDF 页面任一边的硬上限（200 英寸），超过时等比缩放图片页。
const MAX_PAGE_POINTS: f64 = 14400.0;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "tif", "tiff", "bmp", "gif"];
const DOC_EXTS: &[&str] = &["doc", "docx"];

// ==================== 对外数据类型 ====================

/// 单个输入文件的分析结果（前端据此展示与排序）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeInputInfo {
    pub path: String,
    pub name: String,
    /// `pdf` / `image` / `doc`
    pub kind: String,
    pub extension: String,
    pub page_count: usize,
    pub file_size: u64,
    /// 修改时间（Unix 毫秒），排序用
    pub modified_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// 该文件单独分析失败时的原因；前端会提示并跳过
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 合并请求中的单项（顺序即最终页面顺序）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeItemRequest {
    pub path: String,
    /// 仅 Word 文档使用；留空/None 表示全部页面
    #[serde(default)]
    pub page_selection: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MergeProgress {
    /// 已处理数量（0..=total）
    pub current: usize,
    pub total: usize,
    pub current_file: String,
    /// `pdf` / `image` / `convert` / `done`
    pub phase: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MergeResult {
    pub success: bool,
    pub output_path: String,
    pub file_size: u64,
    pub page_count: usize,
    pub source_count: usize,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfficeConverterStatus {
    /// LibreOffice 可执行文件路径
    pub libreoffice: Option<String>,
    /// 是否检测到 Microsoft Word（Windows）
    pub word: bool,
    pub available: bool,
}

// ==================== 缓存目录 ====================

static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 由 main.rs 在应用启动时调用，把 Word→PDF 缓存放到应用缓存目录。
pub fn init_merge_cache(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let _ = CACHE_DIR.set(dir);
}

fn cache_dir() -> PathBuf {
    CACHE_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::temp_dir().join("toolsbag_pdf_merge"))
}

// ==================== 工具函数 ====================

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn modified_ms(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// 按扩展名判定输入类型。
fn detect_kind(path: &Path) -> Result<&'static str, String> {
    let ext = extension_of(path);
    if ext == "pdf" {
        Ok("pdf")
    } else if DOC_EXTS.contains(&ext.as_str()) {
        Ok("doc")
    } else if IMAGE_EXTS.contains(&ext.as_str()) {
        Ok("image")
    } else {
        Err(format!(
            "不支持的文件类型：{}（仅支持 PDF、图片、DOC/DOCX）",
            file_name(path)
        ))
    }
}

fn create_no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let finder = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    let output = create_no_window(Command::new(finder).arg(name))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let candidates: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    // Windows 上 `where soffice` 可能同时返回 soffice.com 和 soffice.exe，
    // 优先用 GUI 版 exe（配合 CREATE_NO_WINDOW 不会闪窗口）。
    let preferred = candidates
        .iter()
        .find(|line| line.to_ascii_lowercase().ends_with(".exe"))
        .or_else(|| candidates.first())?;
    let pb = PathBuf::from(preferred);
    pb.is_file().then_some(pb)
}

fn find_soffice() -> Option<PathBuf> {
    static CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            if let Ok(env_path) = std::env::var("SOFFICE_PATH") {
                let pb = PathBuf::from(env_path);
                if pb.is_file() {
                    return Some(pb);
                }
            }
            if let Some(found) = find_on_path("soffice") {
                return Some(found);
            }
            let candidates = if cfg!(target_os = "windows") {
                vec![
                    PathBuf::from(r"C:\Program Files\LibreOffice\program\soffice.exe"),
                    PathBuf::from(r"C:\Program Files (x86)\LibreOffice\program\soffice.exe"),
                ]
            } else if cfg!(target_os = "macos") {
                vec![PathBuf::from(
                    "/Applications/LibreOffice.app/Contents/MacOS/soffice",
                )]
            } else {
                vec![
                    PathBuf::from("/usr/bin/soffice"),
                    PathBuf::from("/usr/bin/libreoffice"),
                    PathBuf::from("/opt/libreoffice/program/soffice"),
                ]
            };
            candidates.into_iter().find(|p| p.is_file())
        })
        .clone()
}

fn word_available() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| {
        if !cfg!(target_os = "windows") {
            return false;
        }
        if find_on_path("winword").is_some() {
            return true;
        }
        // Office 安装后通常会把 WINWORD.EXE 写进 App Paths，但目录不一定在 PATH。
        for key in [
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\winword.exe",
            r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\winword.exe",
        ] {
            if let Ok(out) =
                create_no_window(Command::new("reg.exe").args(["query", key, "/ve"])).output()
            {
                if out.status.success()
                    && String::from_utf8_lossy(&out.stdout)
                        .to_ascii_lowercase()
                        .contains("winword.exe")
                {
                    return true;
                }
            }
        }
        for candidate in [
            r"C:\Program Files\Microsoft Office\root\Office16\WINWORD.EXE",
            r"C:\Program Files (x86)\Microsoft Office\root\Office16\WINWORD.EXE",
            r"C:\Program Files\Microsoft Office\Office16\WINWORD.EXE",
        ] {
            if Path::new(candidate).is_file() {
                return true;
            }
        }
        false
    })
}

/// 读取 PDF 页数（会完整解析 PDF；分析阶段一次性使用）。
fn ensure_unencrypted(doc: &Document, name: &str) -> Result<(), String> {
    if doc.trailer.has(b"Encrypt") {
        return Err(format!("「{name}」是加密 PDF，请先解除密码后再合并"));
    }
    Ok(())
}

fn pdf_page_count(path: &Path) -> Result<usize, String> {
    let doc = Document::load(path).map_err(|e| format!("无法解析 PDF：{}", e))?;
    ensure_unencrypted(&doc, &file_name(path))?;
    let count = doc.get_pages().len();
    if count == 0 {
        return Err("PDF 中没有可合并的页面".into());
    }
    Ok(count)
}

// ==================== 页码解析 ====================

/// 解析 `1,5,7` / `1-3,5` / `全部`。返回 `None` 表示全部页面。
fn parse_page_selection(spec: &str, total: usize) -> Result<Option<Vec<usize>>, String> {
    let trimmed = spec.trim();
    if trimmed.is_empty()
        || trimmed == "*"
        || trimmed == "全部"
        || trimmed.eq_ignore_ascii_case("all")
    {
        return Ok(None);
    }
    if total == 0 {
        return Err("文档没有页面".into());
    }

    let normalized = trimmed
        .replace(['，', '、'], ",")
        .replace('；', ";")
        .replace(['–', '—', '～'], "-");

    let mut selected = std::collections::BTreeSet::new();
    for token in normalized.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let (start, end) = match token.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (token, token),
        };
        let start: usize = start
            .parse()
            .map_err(|_| format!("页码格式错误：{token}（示例：1,5,7 或 1-3,5）"))?;
        let end: usize = end
            .parse()
            .map_err(|_| format!("页码格式错误：{token}（示例：1,5,7 或 1-3,5）"))?;
        if start == 0 || end == 0 {
            return Err("页码从 1 开始".into());
        }
        if start > end {
            return Err(format!("页码范围 {token} 的起始页大于结束页"));
        }
        if end > total {
            return Err(format!("页码 {end} 超出文档总页数 {total}"));
        }
        for page in start..=end {
            selected.insert(page);
        }
    }

    if selected.is_empty() {
        return Ok(None);
    }
    Ok(Some(selected.into_iter().collect()))
}

// ==================== Office → PDF 转换 ====================

struct TempWorkDir {
    path: PathBuf,
}

impl TempWorkDir {
    fn new(prefix: &str) -> Result<Self, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = cache_dir()
            .join("work")
            .join(format!("{prefix}_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&path).map_err(|e| format!("创建转换临时目录失败：{e}"))?;
        Ok(Self { path })
    }
}

impl Drop for TempWorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn is_valid_pdf_file(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if meta.len() < 5 {
        return false;
    }
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 4];
    file.read_exact(&mut head).is_ok() && head == *b"%PDF"
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 缓存键：规范化路径（小写）+ 文件大小 + 修改时间。
fn doc_cache_key(path: &Path) -> Result<String, String> {
    let meta = fs::metadata(path).map_err(|e| format!("无法读取文件信息：{e}"))?;
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut material = canonical.to_string_lossy().to_lowercase().into_bytes();
    material.extend_from_slice(&meta.len().to_le_bytes());
    material.extend_from_slice(&modified_ms(&meta).to_le_bytes());
    Ok(format!("{:016x}", fnv1a64(&material)))
}

/// 把本地路径转成 LibreOffice 能接受的 file URL（转义空格/中文等字符）。
///
/// 注意：盘符里的 `:` 必须保留。LibreOffice 遇到 `file:///C%3A/...` 会卡住不返回，
/// 这也是自定义 UserInstallation 时最容易踩的坑。
fn path_to_file_url(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let mut url = String::from("file:///");
    for byte in raw.as_bytes() {
        let ch = *byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~' | '/' | ':') {
            url.push(ch);
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

/// LibreOffice / Word 单次转换的超时时间。超时后强杀进程树，避免整个导入流程卡死。
const OFFICE_CONVERT_TIMEOUT: Duration = Duration::from_secs(180);

struct CommandOutput {
    timed_out: bool,
    log: String,
}

impl CommandOutput {
    fn detail(&self) -> String {
        if self.timed_out {
            return format!("转换超时（{} 秒）", OFFICE_CONVERT_TIMEOUT.as_secs());
        }
        self.log.trim().to_string()
    }
}

/// 等待子进程并限制最长运行时间；超时后连同子进程树一起结束。
///
/// 输出重定向到临时文件而不是管道：LibreOffice 会派生 soffice.bin，
/// 管道可能被孙进程继续持有，父进程结束后 read_to_end 仍可能卡住。
fn run_command_with_timeout(mut cmd: Command, timeout: Duration) -> Result<CommandOutput, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let log_path = std::env::temp_dir().join(format!(
        "toolsbag_pdf_merge_cmd_{}_{nanos}.log",
        std::process::id()
    ));
    let log = fs::File::create(&log_path).map_err(|e| format!("创建命令日志失败：{e}"))?;
    let log_err = log
        .try_clone()
        .map_err(|e| format!("创建命令日志失败：{e}"))?;
    cmd.stdout(Stdio::from(log)).stderr(Stdio::from(log_err));

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            let _ = fs::remove_file(&log_path);
            return Err(err.to_string());
        }
    };
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break Some(status);
        }
        if started.elapsed() >= timeout {
            kill_process_tree(&mut child);
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    let log_text = fs::read_to_string(&log_path).unwrap_or_default();
    let _ = fs::remove_file(&log_path);
    Ok(CommandOutput {
        timed_out: status.is_none(),
        log: log_text,
    })
}

fn kill_process_tree(child: &mut Child) {
    let pid = child.id();
    #[cfg(target_os = "windows")]
    {
        let _ =
            create_no_window(Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]))
                .output();
    }
    let _ = child.kill();
}

fn find_pdf_in_dir(dir: &Path, stem: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut fallback = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || extension_of(&path) != "pdf" {
            continue;
        }
        if stem.is_some() && path.file_stem() == stem {
            return Some(path);
        }
        if fallback.is_none() {
            fallback = Some(path);
        }
    }
    fallback
}

fn move_into_cache(source: &Path, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        let _ = fs::remove_file(dest);
    }
    match fs::rename(source, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source, dest).map_err(|e| format!("写入转换缓存失败：{e}"))?;
            Ok(())
        }
    }
}

fn convert_with_libreoffice(soffice: &Path, src: &Path, dest: &Path) -> Result<(), String> {
    let work = TempWorkDir::new("lo")?;
    let profile = work.path.join("profile");
    fs::create_dir_all(&profile).map_err(|e| format!("创建 LibreOffice 配置目录失败：{e}"))?;

    let mut cmd = Command::new(soffice);
    cmd.arg("--headless")
        .arg("--norestore")
        .arg("--nologo")
        .arg("--nofirststartwizard")
        .arg(format!(
            "-env:UserInstallation={}",
            path_to_file_url(&profile)
        ))
        .arg("--convert-to")
        .arg("pdf")
        .arg("--outdir")
        .arg(&work.path)
        .arg(src);
    create_no_window(&mut cmd);
    let output = run_command_with_timeout(cmd, OFFICE_CONVERT_TIMEOUT)
        .map_err(|e| format!("无法启动 LibreOffice：{e}"))?;

    let pdf = find_pdf_in_dir(&work.path, src.file_stem()).ok_or_else(|| {
        let detail = output.detail();
        if detail.is_empty() {
            "LibreOffice 没有生成 PDF".to_string()
        } else {
            format!("LibreOffice 没有生成 PDF：{detail}")
        }
    })?;

    move_into_cache(&pdf, dest)
}

#[cfg(target_os = "windows")]
const WORD_CONVERT_SCRIPT: &str = r#"param(
    [Parameter(Mandatory=$true)][string]$In,
    [Parameter(Mandatory=$true)][string]$Out
)
$ErrorActionPreference = 'Stop'
$word = $null
$startedWord = $false
$oldAlerts = 0
$oldSecurity = 1
try {
    $word = New-Object -ComObject Word.Application
    # Word 是单实例 COM 服务：如果用户已经打开了 Word，会附着到现有实例。
    # 只有原本没有文档时才由我们负责退出，避免关掉用户正在编辑的文档。
    $startedWord = ($word.Documents.Count -eq 0)
    $oldAlerts = $word.DisplayAlerts
    $oldSecurity = $word.AutomationSecurity
    $word.DisplayAlerts = 0
    # msoAutomationSecurityForceDisable：打开文档时不执行任何宏。
    $word.AutomationSecurity = 3
    $doc = $word.Documents.Open($In, $false, $true)
    try {
        $doc.SaveAs($Out, 17)
    } finally {
        $doc.Close(0)
    }
} finally {
    if ($word -ne $null) {
        # Word 可能在关掉最后一个文档后自行退出，此时 COM 已断开；
        # 清理失败不应让整个转换返回非零（PDF 已经写出）。
        try { $word.DisplayAlerts = $oldAlerts } catch { }
        try { $word.AutomationSecurity = $oldSecurity } catch { }
        if ($startedWord) { try { $word.Quit() } catch { } }
    }
}
"#;

#[cfg(target_os = "windows")]
fn convert_with_word(src: &Path, dest: &Path) -> Result<(), String> {
    let work = TempWorkDir::new("word")?;
    let script = work.path.join("convert.ps1");
    fs::write(&script, WORD_CONVERT_SCRIPT).map_err(|e| format!("写入 Word 转换脚本失败：{e}"))?;
    let temp_out = work.path.join("word-output.pdf");

    let mut cmd = Command::new("powershell.exe");
    cmd.arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script)
        .arg("-In")
        .arg(src)
        .arg("-Out")
        .arg(&temp_out);
    create_no_window(&mut cmd);
    let output = run_command_with_timeout(cmd, OFFICE_CONVERT_TIMEOUT)
        .map_err(|e| format!("无法启动 PowerShell：{e}"))?;

    if !temp_out.is_file() {
        let detail = output.detail();
        return Err(if detail.is_empty() {
            "Microsoft Word 没有生成 PDF".to_string()
        } else {
            format!("Microsoft Word 转换失败：{detail}")
        });
    }
    move_into_cache(&temp_out, dest)
}

/// 把 DOC/DOCX 转成缓存 PDF，返回缓存文件路径。
fn convert_office_to_pdf(src: &Path) -> Result<PathBuf, String> {
    let dir = cache_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建缓存目录失败：{e}"))?;
    let key = doc_cache_key(src)?;
    let dest = dir.join(format!("{key}.pdf"));
    if is_valid_pdf_file(&dest) {
        return Ok(dest);
    }

    let mut errors: Vec<String> = Vec::new();
    if let Some(soffice) = find_soffice() {
        match convert_with_libreoffice(&soffice, src, &dest) {
            Ok(()) => return Ok(dest),
            Err(err) => errors.push(err),
        }
    }

    #[cfg(target_os = "windows")]
    if word_available() {
        match convert_with_word(src, &dest) {
            Ok(()) => return Ok(dest),
            Err(err) => errors.push(err),
        }
    }

    let detail = if errors.is_empty() {
        "未检测到 LibreOffice 或 Microsoft Word".to_string()
    } else {
        errors.join("；")
    };
    Err(format!(
        "无法将「{}」转换为 PDF：{detail}。请安装 LibreOffice 或 Microsoft Word 后重试。",
        file_name(src)
    ))
}

// ==================== 图片 → 单页 PDF ====================

fn read_exif_orientation(path: &Path) -> Option<u32> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
    let raw = exif
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))?;
    (raw != 1).then_some(raw)
}

fn apply_exif_orientation(image: DynamicImage, path: &Path) -> DynamicImage {
    let Some(orientation) = read_exif_orientation(path) else {
        return image;
    };
    let base = image.to_rgba8();
    let rotated = match orientation {
        2 => image::imageops::flip_horizontal(&base),
        3 => image::imageops::rotate180(&base),
        4 => image::imageops::flip_vertical(&base),
        5 => image::imageops::flip_horizontal(&image::imageops::rotate90(&base)),
        6 => image::imageops::rotate90(&base),
        7 => image::imageops::flip_horizontal(&image::imageops::rotate270(&base)),
        8 => image::imageops::rotate270(&base),
        _ => base,
    };
    DynamicImage::ImageRgba8(rotated)
}

/// 透明像素预合成到白底，输出 RGB。
fn flatten_to_white(image: DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = RgbImage::new(width, height);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        let alpha = f32::from(a) / 255.0;
        let inverse = 1.0 - alpha;
        let blend =
            |channel: u8| -> u8 { (f32::from(channel) * alpha + 255.0 * inverse).round() as u8 };
        rgb.put_pixel(x, y, image::Rgb([blend(r), blend(g), blend(b)]));
    }
    rgb
}

fn image_to_document(path: &Path) -> Result<Document, String> {
    let image =
        image::open(path).map_err(|e| format!("无法解码图片「{}」：{e}", file_name(path)))?;
    let image = apply_exif_orientation(image, path);
    let rgb = flatten_to_white(image);
    let (width, height) = rgb.dimensions();
    if width == 0 || height == 0 {
        return Err(format!("图片「{}」尺寸无效", file_name(path)));
    }

    let mut jpeg = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 92);
    encoder
        .encode(rgb.as_raw(), width, height, image::ExtendedColorType::Rgb8)
        .map_err(|e| format!("图片「{}」编码失败：{e}", file_name(path)))?;

    let scale = (MAX_PAGE_POINTS / f64::from(width.max(height))).min(1.0);
    let page_width = f64::from(width) * scale;
    let page_height = f64::from(height) * scale;

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let image_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => width,
            "Height" => height,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        jpeg,
    ));
    let content = format!("q\n{page_width:.4} 0 0 {page_height:.4} 0 0 cm\n/Im0 Do\nQ\n");
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let resources_id = doc.add_object(dictionary! {
        "XObject" => dictionary! {
            "Im0" => image_id,
        },
    });
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => resources_id,
        "MediaBox" => vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(page_width),
            Object::Real(page_height),
        ],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    Ok(doc)
}

// ==================== 合并核心 ====================

/// 页面属性（MediaBox / Resources / Rotate 等）允许写在祖先 Pages 节点上，
/// 由页面继承。抽出页面并改挂到新 Pages 节点前，必须把继承值直接写到页面字典，
/// 否则合并后的页面会丢失尺寸或资源。
fn materialize_inherited_page_attrs(page: &Dictionary, source: &Document) -> Dictionary {
    const INHERITABLE: [&[u8]; 4] = [b"MediaBox", b"CropBox", b"Resources", b"Rotate"];
    let mut dict = page.clone();
    let mut parent = dict
        .get(b"Parent")
        .ok()
        .and_then(|object| object.as_reference().ok());
    let mut depth = 0;

    while let Some(parent_id) = parent {
        if depth >= 64 {
            break;
        }
        let Ok(parent_dict) = source.get_dictionary(parent_id) else {
            break;
        };
        for key in INHERITABLE {
            if !dict.has(key) {
                if let Ok(value) = parent_dict.get(key) {
                    dict.set(key.to_vec(), value.clone());
                }
            }
        }
        parent = parent_dict
            .get(b"Parent")
            .ok()
            .and_then(|object| object.as_reference().ok());
        depth += 1;
    }
    dict
}

/// 把一个源 PDF 的选中页面及其依赖对象并入 merged。
/// 只复制非 Page/Catalog/Pages 对象，页面对象单独存放，最后统一挂到新 Pages 节点。
fn append_source(
    merged: &mut Document,
    mut source: Document,
    selection: Option<&[usize]>,
    next_id: &mut u32,
    page_ids: &mut Vec<ObjectId>,
    page_dicts: &mut Vec<(ObjectId, Object)>,
) -> Result<usize, String> {
    source.renumber_objects_with(*next_id);
    *next_id = source.max_id + 1;

    let page_map = source.get_pages();
    let selected_ids: Vec<ObjectId> = match selection {
        Some(pages) => pages
            .iter()
            .filter_map(|page| page_map.get(&(*page as u32)).copied())
            .collect(),
        None => page_map.values().copied().collect(),
    };
    if selected_ids.is_empty() {
        return Err("选中的页面为空".into());
    }

    for (id, object) in source.objects.iter() {
        let skip = matches!(
            object.type_name().unwrap_or(""),
            "Catalog" | "Pages" | "Page" | "Outlines" | "Outline"
        );
        if !skip {
            merged.objects.insert(*id, object.clone());
        }
    }

    for page_id in &selected_ids {
        let page = source
            .get_object(*page_id)
            .map_err(|e| format!("读取页面对象失败：{e}"))?;
        let page_dict = page.as_dict().map_err(|_| "页面对象格式无效".to_string())?;
        let mut dict = materialize_inherited_page_attrs(page_dict, &source);
        // Parent 指向本文件，稍后统一改成合并文档的 Pages 节点。
        dict.remove(b"Parent");
        page_dicts.push((*page_id, Object::Dictionary(dict)));
        page_ids.push(*page_id);
    }

    Ok(selected_ids.len())
}

/// 收尾：创建 Pages / Catalog，重编号并返回总页数。
fn finalize_merged(
    merged: &mut Document,
    next_id: u32,
    page_ids: Vec<ObjectId>,
    page_dicts: Vec<(ObjectId, Object)>,
) -> usize {
    merged.max_id = next_id.saturating_sub(1);
    let pages_id = merged.new_object_id();

    for (id, mut object) in page_dicts {
        if let Object::Dictionary(dict) = &mut object {
            dict.set("Parent", pages_id);
        }
        merged.objects.insert(id, object);
    }

    let count = page_ids.len();
    let kids: Vec<Object> = page_ids.into_iter().map(Object::Reference).collect();
    merged.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count as u32,
        }),
    );
    let catalog_id = merged.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged.objects.keys().map(|(id, _)| *id).max().unwrap_or(0);
    merged.renumber_objects();
    count
}

// ==================== 命令：分析输入 ====================

#[tauri::command]
pub async fn analyze_merge_inputs(paths: Vec<String>) -> Result<Vec<MergeInputInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || analyze_merge_inputs_blocking(paths))
        .await
        .map_err(|e| e.to_string())?
}

fn analyze_merge_inputs_blocking(paths: Vec<String>) -> Result<Vec<MergeInputInfo>, String> {
    let mut result = Vec::with_capacity(paths.len());

    for raw in paths {
        let path = PathBuf::from(&raw);
        let name = file_name(&path);
        let extension = extension_of(&path);
        let meta = fs::metadata(&path).ok();
        let file_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let mtime = meta.as_ref().map(modified_ms).unwrap_or(0);

        let mut info = MergeInputInfo {
            path: path.to_string_lossy().to_string(),
            name,
            kind: "unknown".into(),
            extension,
            page_count: 0,
            file_size,
            modified_ms: mtime,
            width: None,
            height: None,
            error: None,
        };

        if !path.is_file() {
            info.error = Some("文件不存在或无法访问".into());
            result.push(info);
            continue;
        }

        match detect_kind(&path) {
            Ok("pdf") => {
                info.kind = "pdf".into();
                match pdf_page_count(&path) {
                    Ok(count) => info.page_count = count,
                    Err(err) => info.error = Some(err),
                }
            }
            Ok("doc") => {
                info.kind = "doc".into();
                match convert_office_to_pdf(&path).and_then(|pdf| pdf_page_count(&pdf)) {
                    Ok(count) => info.page_count = count,
                    Err(err) => info.error = Some(err),
                }
            }
            Ok("image") => {
                info.kind = "image".into();
                info.page_count = 1;
                match image::image_dimensions(&path) {
                    Ok((width, height)) => {
                        info.width = Some(width);
                        info.height = Some(height);
                    }
                    Err(err) => {
                        info.error = Some(format!("无法读取图片：{err}"));
                    }
                }
            }
            Ok(_) => {
                info.error = Some("未知的文件类型".into());
            }
            Err(err) => {
                info.error = Some(err);
            }
        }

        result.push(info);
    }

    Ok(result)
}

// ==================== 命令：合并 ====================

#[tauri::command]
pub async fn merge_pdf_documents(
    app: AppHandle,
    items: Vec<MergeItemRequest>,
    output_path: String,
) -> Result<MergeResult, String> {
    tauri::async_runtime::spawn_blocking(move || merge_blocking(app, items, output_path))
        .await
        .map_err(|e| e.to_string())?
}

fn merge_blocking(
    app: AppHandle,
    items: Vec<MergeItemRequest>,
    output_path: String,
) -> Result<MergeResult, String> {
    if items.is_empty() {
        return Err("请先添加需要合并的文件".into());
    }
    let started = Instant::now();

    let mut output = PathBuf::from(output_path.trim());
    if output.as_os_str().is_empty() {
        return Err("请选择输出路径".into());
    }
    if output.extension().is_none() {
        output.set_extension("pdf");
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("创建输出目录失败：{e}"))?;
        }
    }

    let mut merged = Document::with_version("1.7");
    let mut next_id = 1u32;
    let mut page_ids: Vec<ObjectId> = Vec::new();
    let mut page_dicts: Vec<(ObjectId, Object)> = Vec::new();
    let total = items.len();

    for (index, item) in items.iter().enumerate() {
        let path = PathBuf::from(item.path.trim());
        if !path.is_file() {
            return Err(format!("文件不存在：{}", item.path));
        }
        let kind = detect_kind(&path)?;
        let name = file_name(&path);

        let phase = match kind {
            "doc" => "convert",
            "image" => "image",
            _ => "pdf",
        };
        let _ = app.emit(
            "pdf_merge_progress",
            MergeProgress {
                current: index,
                total,
                current_file: name.clone(),
                phase: phase.into(),
            },
        );

        let (doc, selection) = match kind {
            "doc" => {
                let converted = convert_office_to_pdf(&path)?;
                let doc = Document::load(&converted)
                    .map_err(|e| format!("无法读取「{name}」转换后的 PDF：{e}"))?;
                ensure_unencrypted(&doc, &name)?;
                let selection = parse_page_selection(
                    item.page_selection.as_deref().unwrap_or(""),
                    doc.get_pages().len(),
                )?;
                (doc, selection)
            }
            "pdf" => {
                let doc =
                    Document::load(&path).map_err(|e| format!("无法读取 PDF「{name}」：{e}"))?;
                ensure_unencrypted(&doc, &name)?;
                let selection = parse_page_selection(
                    item.page_selection.as_deref().unwrap_or(""),
                    doc.get_pages().len(),
                )?;
                (doc, selection)
            }
            "image" => (image_to_document(&path)?, None),
            _ => return Err(format!("不支持的文件类型：{name}")),
        };

        append_source(
            &mut merged,
            doc,
            selection.as_deref(),
            &mut next_id,
            &mut page_ids,
            &mut page_dicts,
        )?;

        let _ = app.emit(
            "pdf_merge_progress",
            MergeProgress {
                current: index + 1,
                total,
                current_file: name,
                phase: phase.into(),
            },
        );
    }

    let page_count = finalize_merged(&mut merged, next_id, page_ids, page_dicts);
    if page_count == 0 {
        return Err("没有可合并的页面".into());
    }
    merged
        .save(&output)
        .map_err(|e| format!("写入 PDF 失败：{e}"))?;

    let file_size = fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    let _ = app.emit(
        "pdf_merge_progress",
        MergeProgress {
            current: total,
            total,
            current_file: String::new(),
            phase: "done".into(),
        },
    );

    Ok(MergeResult {
        success: true,
        output_path: output.to_string_lossy().to_string(),
        file_size,
        page_count,
        source_count: total,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

// ==================== 命令：状态 / 剪贴板 ====================

#[tauri::command]
pub fn get_office_converter_status() -> OfficeConverterStatus {
    let libreoffice = find_soffice().map(|path| path.to_string_lossy().to_string());
    let word = word_available();
    OfficeConverterStatus {
        available: libreoffice.is_some() || word,
        libreoffice,
        word,
    }
}

/// 把生成的 PDF 以「文件」形式写入剪贴板（Windows CF_HDROP），可直接粘贴到资源管理器/聊天窗口。
#[tauri::command]
pub fn copy_pdf_file(path: String) -> Result<(), String> {
    let file = PathBuf::from(&path);
    if !file.is_file() {
        return Err("PDF 文件不存在".into());
    }
    #[cfg(target_os = "windows")]
    {
        use clipboard_win::{formats, Clipboard, Setter};
        let _clipboard =
            Clipboard::new_attempts(10).map_err(|e| format!("剪贴板暂时不可用：{e}"))?;
        formats::FileList
            .write_clipboard(&[path])
            .map_err(|e| format!("复制 PDF 文件失败：{e}"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("当前平台不支持把文件复制到剪贴板".into())
    }
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_keeps_drive_colon_and_encodes_unicode() {
        let url = path_to_file_url(Path::new(r"C:\Temp\测试 目录\profile"));
        assert_eq!(
            url,
            "file:///C:/Temp/%E6%B5%8B%E8%AF%95%20%E7%9B%AE%E5%BD%95/profile"
        );
    }

    #[test]
    fn parses_page_lists_and_ranges() {
        assert_eq!(
            parse_page_selection("1,5,7", 10).unwrap(),
            Some(vec![1, 5, 7])
        );
        assert_eq!(
            parse_page_selection("1-3, 5，7", 10).unwrap(),
            Some(vec![1, 2, 3, 5, 7])
        );
        assert_eq!(parse_page_selection("", 10).unwrap(), None);
        assert_eq!(parse_page_selection("全部", 10).unwrap(), None);
        assert_eq!(parse_page_selection("3,3,1", 10).unwrap(), Some(vec![1, 3]));
    }

    #[test]
    fn rejects_invalid_page_selection() {
        assert!(parse_page_selection("0", 10).is_err());
        assert!(parse_page_selection("5-3", 10).is_err());
        assert!(parse_page_selection("1,11", 10).is_err());
        assert!(parse_page_selection("abc", 10).is_err());
    }

    #[test]
    fn materializes_inherited_page_attributes() {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
                "MediaBox" => vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(100),
                    Object::Integer(200),
                ],
                "Resources" => dictionary! {},
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let page = doc.get_object(page_id).unwrap().as_dict().unwrap();
        let materialized = materialize_inherited_page_attrs(page, &doc);
        assert!(materialized.has(b"MediaBox"));
        assert!(materialized.has(b"Resources"));
    }

    fn write_one_page_pdf(path: &Path) {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(100),
                Object::Integer(100),
            ],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc.save(path).unwrap();
    }

    #[test]
    fn merges_two_documents_and_keeps_page_count() {
        let dir = std::env::temp_dir().join(format!(
            "toolsbag_pdf_merge_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let first = dir.join("a.pdf");
        let second = dir.join("b.pdf");
        let output = dir.join("merged.pdf");
        write_one_page_pdf(&first);
        write_one_page_pdf(&second);

        let mut merged = Document::with_version("1.5");
        let mut next_id = 1u32;
        let mut page_ids = Vec::new();
        let mut page_dicts = Vec::new();
        for path in [&first, &second] {
            let doc = Document::load(path).unwrap();
            append_source(
                &mut merged,
                doc,
                None,
                &mut next_id,
                &mut page_ids,
                &mut page_dicts,
            )
            .unwrap();
        }
        let count = finalize_merged(&mut merged, next_id, page_ids, page_dicts);
        assert_eq!(count, 2);
        merged.save(&output).unwrap();

        let reloaded = Document::load(&output).unwrap();
        assert_eq!(reloaded.get_pages().len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn live_mixed_merge_smoke() {
        let Some(dir) = std::env::var_os("TOOLSBAG_PM_LIVE") else {
            return;
        };
        let dir = PathBuf::from(dir);
        let docx = dir.join("sample.docx");
        let png = dir.join("sample.png");
        let pdf = dir.join("sample.pdf");
        if !docx.is_file() || !png.is_file() || !pdf.is_file() {
            return;
        }
        init_merge_cache(dir.join("cache"));

        let converted = convert_office_to_pdf(&docx).unwrap();
        assert_eq!(pdf_page_count(&converted).unwrap(), 3);

        let mut merged = Document::with_version("1.5");
        let mut next_id = 1u32;
        let mut page_ids = Vec::new();
        let mut page_dicts = Vec::new();

        let doc = Document::load(&pdf).unwrap();
        append_source(
            &mut merged,
            doc,
            None,
            &mut next_id,
            &mut page_ids,
            &mut page_dicts,
        )
        .unwrap();

        append_source(
            &mut merged,
            image_to_document(&png).unwrap(),
            None,
            &mut next_id,
            &mut page_ids,
            &mut page_dicts,
        )
        .unwrap();

        let doc = Document::load(&converted).unwrap();
        let selection = parse_page_selection("1,3", doc.get_pages().len()).unwrap();
        append_source(
            &mut merged,
            doc,
            selection.as_deref(),
            &mut next_id,
            &mut page_ids,
            &mut page_dicts,
        )
        .unwrap();

        let count = finalize_merged(&mut merged, next_id, page_ids, page_dicts);
        // 2 页 PDF + 1 页图片 + Word 选中的第 1/3 页
        assert_eq!(count, 5);
        let out = dir.join("live-merged.pdf");
        merged.save(&out).unwrap();
        let reloaded = Document::load(&out).unwrap();
        assert_eq!(reloaded.get_pages().len(), 5);
        println!("LIVE_MERGED={}", out.display());
    }
}
