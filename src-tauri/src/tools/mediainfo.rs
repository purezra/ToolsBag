//! MediaInfo.dll 动态加载封装
//! 用于获取视频、音频、图片的详细元数据

use crate::models::{AudioMeta, ImageMeta, VideoMeta};
use libloading::{Library, Symbol};
use once_cell::sync::OnceCell;
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// MediaInfo 函数类型定义 - 使用 system 调用约定 (Windows stdcall)
type MediaInfoNew = unsafe extern "system" fn() -> *mut std::ffi::c_void;
type MediaInfoDelete = unsafe extern "system" fn(*mut std::ffi::c_void);
type MediaInfoOpenW = unsafe extern "system" fn(*mut std::ffi::c_void, *const u16) -> usize;
type MediaInfoClose = unsafe extern "system" fn(*mut std::ffi::c_void);
type MediaInfoInform = unsafe extern "system" fn(*mut std::ffi::c_void, usize) -> *const u16;
type MediaInfoOption =
    unsafe extern "system" fn(*mut std::ffi::c_void, *const u16, *const u16) -> *const u16;

static MEDIAINFO_LIB: OnceCell<Option<Library>> = OnceCell::new();
static MEDIAINFO_PATH: OnceCell<Mutex<Option<String>>> = OnceCell::new();
static RESOURCE_DIR: OnceCell<PathBuf> = OnceCell::new();

/// 设置 Tauri 资源目录，供 MediaInfo DLL 查找使用
pub fn set_resource_dir(path: PathBuf) {
    RESOURCE_DIR.set(path).ok();
}

/// 获取 MediaInfo.dll 的加载路径
pub fn get_mediainfo_path() -> Option<String> {
    MEDIAINFO_PATH.get()?.lock().ok()?.clone()
}

/// 内嵌的 MediaInfo.dll（编译时嵌入二进制文件）
const EMBEDDED_MEDIAINFO_DLL: &[u8] = include_bytes!("../../MediaInfo.dll");

fn mediainfo_library_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // 从内嵌资源释放到临时目录
    let temp_dll = std::env::temp_dir().join("toolsbag_mediainfo.dll");
    if !temp_dll.exists() {
        if std::fs::write(&temp_dll, EMBEDDED_MEDIAINFO_DLL).is_ok() {
            candidates.push(temp_dll.clone());
        }
    } else {
        candidates.push(temp_dll.clone());
    }

    // Tauri 资源目录（打包时 bundle.resources 的目标位置）
    if let Some(res_dir) = RESOURCE_DIR.get() {
        candidates.push(res_dir.join("MediaInfo.dll"));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("MediaInfo.dll"));
            candidates.push(dir.join("bin").join("windows").join("MediaInfo.dll"));
            candidates.push(dir.join("resources").join("MediaInfo.dll"));
            candidates.push(
                dir.join("resources")
                    .join("bin")
                    .join("windows")
                    .join("MediaInfo.dll"),
            );
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("MediaInfo.dll"));
                candidates.push(parent.join("resources").join("MediaInfo.dll"));
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("MediaInfo.dll"));
        candidates.push(cwd.join("src-tauri").join("MediaInfo.dll"));
        candidates.push(
            cwd.join("src-tauri")
                .join("bin")
                .join("windows")
                .join("MediaInfo.dll"),
        );
    }
    candidates.push(PathBuf::from("MediaInfo.dll"));

    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|path| seen.insert(path.to_string_lossy().to_ascii_lowercase()))
        .collect()
}

/// 初始化 MediaInfo 库
pub fn init_mediainfo() -> bool {
    MEDIAINFO_PATH.get_or_init(|| Mutex::new(None));

    MEDIAINFO_LIB.get_or_init(|| {
        for path in mediainfo_library_candidates() {
            match unsafe { Library::new(&path) } {
                Ok(lib) => {
                    println!("[MediaInfo] Loaded from: {:?}", path);
                    if let Some(mutex) = MEDIAINFO_PATH.get() {
                        if let Ok(mut guard) = mutex.lock() {
                            *guard = Some(path.display().to_string());
                        }
                    }
                    unsafe {
                        if lib.get::<MediaInfoNew>(b"MediaInfo_New\0").is_ok() {
                            println!("[MediaInfo] Found MediaInfo_New");
                            return Some(lib);
                        }
                    }
                }
                Err(e) => {
                    println!("[MediaInfo] Failed to load {:?}: {}", path, e);
                }
            }
        }

        println!("[MediaInfo] DLL not found in any path");
        None
    });

    is_mediainfo_available()
}

/// MediaInfo 句柄包装器（RAII）
struct MediaInfoHandle {
    handle: *mut std::ffi::c_void,
    mi_delete: Symbol<'static, MediaInfoDelete>,
    mi_close: Symbol<'static, MediaInfoClose>,
}

impl MediaInfoHandle {
    /// 打开文件，返回句柄
    fn open(path: &Path) -> Option<Self> {
        let lib = MEDIAINFO_LIB.get()?.as_ref()?;
        unsafe {
            let mi_new: Symbol<MediaInfoNew> = lib.get(b"MediaInfo_New\0").ok()?;
            let mi_delete: Symbol<MediaInfoDelete> = lib.get(b"MediaInfo_Delete\0").ok()?;
            let mi_open_fn: Symbol<MediaInfoOpenW> = lib.get(b"MediaInfo_Open\0").ok()?;
            let mi_close: Symbol<MediaInfoClose> = lib.get(b"MediaInfo_Close\0").ok()?;

            let handle = mi_new();
            if handle.is_null() {
                return None;
            }

            let path_wide = to_wide_string(&path.to_string_lossy());
            if mi_open_fn(handle, path_wide.as_ptr()) == 0 {
                mi_delete(handle);
                return None;
            }

            // 需要 'static 生命周期来存储 Symbol，但 lib 的生命周期由 OnceCell 保证
            let mi_delete: Symbol<'static, MediaInfoDelete> = std::mem::transmute(mi_delete);
            let mi_close: Symbol<'static, MediaInfoClose> = std::mem::transmute(mi_close);

            Some(MediaInfoHandle {
                handle,
                mi_delete,
                mi_close,
            })
        }
    }

    /// 获取 Inform 文本输出
    fn get_inform(&self) -> String {
        let lib = match MEDIAINFO_LIB.get() {
            Some(Some(l)) => l,
            _ => return String::new(),
        };
        unsafe {
            let mi_option: Symbol<MediaInfoOption> = match lib.get(b"MediaInfo_Option\0") {
                Ok(s) => s,
                Err(_) => return String::new(),
            };
            let mi_inform: Symbol<MediaInfoInform> = match lib.get(b"MediaInfo_Inform\0") {
                Ok(s) => s,
                Err(_) => return String::new(),
            };

            let opt_key = to_wide_string("Inform");
            let opt_val = to_wide_string("");
            mi_option(self.handle, opt_key.as_ptr(), opt_val.as_ptr());

            let info_ptr = mi_inform(self.handle, 0);
            from_wide_ptr(info_ptr)
        }
    }

    /// 获取 XML 输出
    fn get_xml(&self) -> String {
        let lib = match MEDIAINFO_LIB.get() {
            Some(Some(l)) => l,
            _ => return String::new(),
        };
        unsafe {
            let mi_option: Symbol<MediaInfoOption> = match lib.get(b"MediaInfo_Option\0") {
                Ok(s) => s,
                Err(_) => return String::new(),
            };
            let mi_inform: Symbol<MediaInfoInform> = match lib.get(b"MediaInfo_Inform\0") {
                Ok(s) => s,
                Err(_) => return String::new(),
            };

            let opt_key = to_wide_string("Inform");
            let opt_val = to_wide_string("XML");
            mi_option(self.handle, opt_key.as_ptr(), opt_val.as_ptr());

            let info_ptr = mi_inform(self.handle, 0);
            from_wide_ptr(info_ptr)
        }
    }
}

impl Drop for MediaInfoHandle {
    fn drop(&mut self) {
        unsafe {
            (self.mi_close)(self.handle);
            (self.mi_delete)(self.handle);
        }
    }
}

/// 使用 MediaInfo_Inform 获取完整信息
pub fn get_full_info(path: &Path) -> Option<String> {
    let mi = MediaInfoHandle::open(path)?;
    Some(mi.get_inform())
}

/// 检查 MediaInfo 是否可用
pub fn is_mediainfo_available() -> bool {
    MEDIAINFO_LIB
        .get()
        .map(|opt| opt.is_some())
        .unwrap_or(false)
}

/// 转换为宽字符串 (Windows UTF-16)
fn to_wide_string(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

/// 从宽字符指针读取字符串（带边界保护）
unsafe fn from_wide_ptr(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    const MAX_LEN: usize = 1_000_000; // 防止无终止符时越界
    let mut len = 0;
    while len < MAX_LEN && *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    String::from_utf16_lossy(slice)
}

/// 格式化码率显示
fn format_bitrate(bps: u64) -> String {
    if bps >= 1_000_000 {
        format!("{:.3} Mbps", bps as f64 / 1_000_000.0)
    } else if bps > 0 {
        format!("{:.3} kbps", bps as f64 / 1_000.0)
    } else {
        "-".to_string()
    }
}

/// 格式化时长 (毫秒 -> HH:MM:SS)
fn format_duration(ms: u64) -> String {
    if ms == 0 {
        return "-".to_string();
    }
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{:02}:{:02}", mins, secs)
    }
}

/// 格式化帧率（保留2位小数，整数时不显示小数，不显示FPS）
fn format_frame_rate(value: &str) -> String {
    // 提取数字部分
    let num_str: String = value
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();

    if let Ok(fps) = num_str.parse::<f64>() {
        if fps == fps.floor() {
            // 整数（安全截断）
            format!("{}", fps.clamp(0.0, u32::MAX as f64) as u32)
        } else {
            // 保留2位小数，去掉末尾的0
            let formatted = format!("{:.2}", fps);
            let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
            trimmed.to_string()
        }
    } else {
        value.to_string()
    }
}

/// 获取视频元数据
pub fn get_video_meta(path: &Path) -> Option<VideoMeta> {
    // 使用 MediaInfo_Inform 获取完整信息，然后解析
    let full_info = get_full_info(path)?;

    if full_info.is_empty() {
        return None;
    }

    // 解析完整信息
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let mut bitrate_raw: u64 = 0;
    let mut aspect_ratio = String::new();
    let mut frame_rate = String::new();
    let mut frame_rate_mode = "CFR".to_string();
    let mut codec = String::new();
    let mut hdr_format: Option<String> = None;
    let mut duration_ms: u64 = 0;

    let mut in_video_section = false;

    for line in full_info.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // 检测段落
        if line == "Video" {
            in_video_section = true;
            continue;
        } else if line == "Audio" || line == "Text" || line == "Menu" {
            in_video_section = false;
            continue;
        }

        // 解析键值对
        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim();
            let value = line[colon_pos + 1..].trim();

            if in_video_section {
                match key {
                    "Width" => {
                        width = parse_number(value);
                    }
                    "Height" => {
                        height = parse_number(value);
                    }
                    "Bit rate" => {
                        bitrate_raw = parse_bitrate(value);
                    }
                    "Display aspect ratio" => {
                        aspect_ratio = value.to_string();
                    }
                    "Frame rate" => {
                        frame_rate = format_frame_rate(value);
                    }
                    // TS 文件可能使用 "Original frame rate"
                    "Original frame rate" => {
                        if frame_rate.is_empty() {
                            frame_rate = format_frame_rate(value);
                        }
                    }
                    "Frame rate mode" => {
                        frame_rate_mode = if value.contains("Variable") {
                            "VFR"
                        } else {
                            "CFR"
                        }
                        .to_string();
                    }
                    "Format" => {
                        if codec.is_empty() {
                            codec = value.to_string();
                        }
                    }
                    "HDR format" => {
                        hdr_format = Some(value.to_string());
                    }
                    "Duration" => {
                        duration_ms = parse_duration_to_ms(value);
                    }
                    _ => {}
                }
            } else {
                // General section
                match key {
                    "Overall bit rate" => {
                        if bitrate_raw == 0 {
                            bitrate_raw = parse_bitrate(value);
                        }
                    }
                    "Duration" => {
                        if duration_ms == 0 {
                            duration_ms = parse_duration_to_ms(value);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // 计算宽高比
    if aspect_ratio.is_empty() && width > 0 && height > 0 {
        let ratio = width as f64 / height as f64;
        if (ratio - 16.0 / 9.0).abs() < 0.1 {
            aspect_ratio = "16:9".to_string();
        } else if (ratio - 4.0 / 3.0).abs() < 0.1 {
            aspect_ratio = "4:3".to_string();
        } else if (ratio - 9.0 / 16.0).abs() < 0.1 {
            aspect_ratio = "9:16".to_string();
        } else {
            aspect_ratio = format!("{:.2}:1", ratio);
        }
    }

    // 如果没有帧率但是 VFR 模式，显示 VFR
    if frame_rate.is_empty() && frame_rate_mode == "VFR" {
        frame_rate = "VFR".to_string();
    }

    Some(VideoMeta {
        width,
        height,
        bitrate: format_bitrate(bitrate_raw),
        bitrate_raw,
        aspect_ratio,
        frame_rate,
        frame_rate_mode,
        codec,
        hdr_format,
        duration_ms,
        duration_str: format_duration(duration_ms),
    })
}

/// 解析数字（去除单位）
fn parse_number(s: &str) -> u32 {
    let s = s.replace(" ", "").replace(",", "");
    // 提取数字部分
    let num_str: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    num_str.parse().unwrap_or(0)
}

/// 解析码率
fn parse_bitrate(s: &str) -> u64 {
    let s_lower = s.to_lowercase();
    let s_clean = s.replace(" ", "").replace(",", "");

    // 提取数字
    let num_str: String = s_clean
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let num: f64 = num_str.parse().unwrap_or(0.0);

    if s_lower.contains("mb/s") || s_lower.contains("mbps") {
        (num * 1_000_000.0) as u64
    } else if s_lower.contains("kb/s") || s_lower.contains("kbps") {
        (num * 1_000.0) as u64
    } else {
        num as u64
    }
}

/// 解析时长到毫秒（防溢出）
fn parse_duration_to_ms(s: &str) -> u64 {
    let mut total_ms: u64 = 0;
    let parts: Vec<&str> = s.split_whitespace().collect();

    let mut i = 0;
    while i < parts.len() {
        if let Ok(num) = parts[i].parse::<u64>() {
            if i + 1 < parts.len() {
                let unit = parts[i + 1].to_lowercase();
                let ms = if unit.starts_with("h") {
                    num.saturating_mul(3600).saturating_mul(1000)
                } else if unit.starts_with("min") || unit == "mn" {
                    num.saturating_mul(60).saturating_mul(1000)
                } else if unit.starts_with("s") && !unit.starts_with("ms") {
                    num.saturating_mul(1000)
                } else if unit.starts_with("ms") {
                    num
                } else {
                    0
                };
                total_ms = total_ms.saturating_add(ms);
                i += 2;
            } else {
                total_ms = num;
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    total_ms
}

/// 获取音频元数据
pub fn get_audio_meta(path: &Path) -> Option<AudioMeta> {
    let full_info = get_full_info(path)?;

    if full_info.is_empty() {
        return None;
    }

    let mut codec = String::new();
    let mut sample_rate = String::new();
    let mut bitrate_raw: u64 = 0;
    let mut duration_ms: u64 = 0;

    let mut in_audio_section = false;

    for line in full_info.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line == "Audio" {
            in_audio_section = true;
            continue;
        } else if line == "Video" || line == "Text" || line == "Menu" {
            in_audio_section = false;
            continue;
        }

        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim();
            let value = line[colon_pos + 1..].trim();

            if in_audio_section {
                match key {
                    "Format" => {
                        if codec.is_empty() {
                            codec = value.to_string();
                        }
                    }
                    "Sampling rate" => {
                        sample_rate = value.to_string();
                    }
                    "Bit rate" => {
                        bitrate_raw = parse_bitrate(value);
                    }
                    "Duration" => {
                        duration_ms = parse_duration_to_ms(value);
                    }
                    _ => {}
                }
            } else {
                // General section
                match key {
                    "Overall bit rate" => {
                        if bitrate_raw == 0 {
                            bitrate_raw = parse_bitrate(value);
                        }
                    }
                    "Duration" => {
                        if duration_ms == 0 {
                            duration_ms = parse_duration_to_ms(value);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Some(AudioMeta {
        codec,
        sample_rate,
        bitrate: format_bitrate(bitrate_raw),
        duration_ms,
        duration_str: format_duration(duration_ms),
    })
}

/// 获取图片元数据
pub fn get_image_meta(path: &Path) -> Option<ImageMeta> {
    let full_info = get_full_info(path)?;

    if full_info.is_empty() {
        return None;
    }

    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let mut format = String::new();

    let mut in_image_section = false;

    for line in full_info.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line == "Image" {
            in_image_section = true;
            continue;
        } else if line == "Video" || line == "Audio" || line == "Text" {
            in_image_section = false;
            continue;
        }

        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim();
            let value = line[colon_pos + 1..].trim();

            // 图片信息可能在 General 或 Image 段
            match key {
                "Width" => {
                    if width == 0 {
                        width = parse_number(value);
                    }
                }
                "Height" => {
                    if height == 0 {
                        height = parse_number(value);
                    }
                }
                "Format" => {
                    if format.is_empty()
                        && (in_image_section
                            || value.contains("JPEG")
                            || value.contains("PNG")
                            || value.contains("GIF")
                            || value.contains("BMP")
                            || value.contains("WebP"))
                    {
                        format = value.to_string();
                    }
                }
                _ => {}
            }
        }
    }

    Some(ImageMeta {
        width,
        height,
        format,
    })
}

// ==================== 视频体检 - 详细元数据解析 ====================

use crate::models::{
    DetailedAudioStream, DetailedTextStream, DetailedVideoMeta, DetailedVideoStream, GeneralInfo,
};

/// 临时存储解析中的流数据
struct ParsedStream {
    section: String, // "General", "Video", "Audio", "Text"
    fields: Vec<(String, String)>,
}

/// 解析 MediaInfo_Inform 的完整文本输出，返回所有流
fn parse_mediainfo_text(text: &str) -> Vec<ParsedStream> {
    let mut streams: Vec<ParsedStream> = Vec::new();
    let mut current_section = String::new();
    let mut current_fields: Vec<(String, String)> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 检测是否是新的段落标题
        // MediaInfo 输出格式可能是 "General"、"Video"、"Text" 或 "Video #1"、"Text #2" 等
        let is_section_header = if !trimmed.contains(':') {
            matches!(
                trimmed,
                "General" | "Video" | "Audio" | "Text" | "Other" | "Menu" | "Image"
            )
        } else {
            // 处理 "Text #1"、"Video #2" 等带编号的格式
            let prefix = trimmed.split(':').next().unwrap_or("").trim();
            let base = prefix.split('#').next().unwrap_or("").trim();
            matches!(
                base,
                "General" | "Video" | "Audio" | "Text" | "Other" | "Menu" | "Image"
            )
        };

        if is_section_header {
            // 保存前一个流
            if !current_section.is_empty() {
                streams.push(ParsedStream {
                    section: current_section.clone(),
                    fields: current_fields.clone(),
                });
            }
            // 提取基础段名（去掉编号）
            let section_base = if trimmed.contains(':') {
                trimmed.split(':').next().unwrap_or("").trim()
            } else {
                trimmed
            };
            current_section = section_base
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            current_fields.clear();
            continue;
        }

        // 解析键值对
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim().to_string();
            let value = trimmed[colon_pos + 1..].trim().to_string();
            if !key.is_empty() {
                current_fields.push((key, value));
            }
        }
    }

    // 保存最后一个流
    if !current_section.is_empty() {
        streams.push(ParsedStream {
            section: current_section,
            fields: current_fields,
        });
    }

    streams
}

/// 从流字段中查找指定 key 的值
fn find_field(fields: &[(String, String)], key: &str) -> String {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

/// 从 General 段解析基础信息
fn parse_general_info(streams: &[ParsedStream]) -> GeneralInfo {
    let general = streams.iter().find(|s| s.section == "General");
    let fields = general.map(|s| s.fields.as_slice()).unwrap_or(&[]);

    let file_size_str = find_field(fields, "File size");
    let file_size = parse_file_size(&file_size_str);
    let duration_str = find_field(fields, "Duration");
    let duration_ms = parse_duration_to_ms(&duration_str);

    // Format 可能在多个位置
    let format = find_field(fields, "Format");
    let format_long = if find_field(fields, "Format/Info").is_empty() {
        find_field(fields, "Format profile")
    } else {
        find_field(fields, "Format/Info")
    };

    GeneralInfo {
        format,
        format_long,
        file_size,
        file_size_str,
        duration: format_duration(duration_ms),
        duration_ms,
        overall_bit_rate: find_field(fields, "Overall bit rate"),
        title: find_field(fields, "Title"),
        encoded_date: find_field(fields, "Encoded date"),
        writing_application: find_field(fields, "Writing application"),
        codec_id: find_field(fields, "Codec ID"),
        stream_count: find_field(fields, "StreamCount").parse().unwrap_or(0),
        encoded_library: find_field(fields, "Encoded library"),
    }
}

/// 解析文件大小字符串为字节数
fn parse_file_size(s: &str) -> u64 {
    let s_clean = s.replace([' ', ','], "");
    let num_str: String = s_clean
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let num: f64 = num_str.parse().unwrap_or(0.0);
    let s_lower = s.to_lowercase();
    if s_lower.contains("gib") || s_lower.contains("gb") {
        (num * 1_073_741_824.0) as u64
    } else if s_lower.contains("mib") || s_lower.contains("mb") {
        (num * 1_048_576.0) as u64
    } else if s_lower.contains("kib") || s_lower.contains("kb") {
        (num * 1024.0) as u64
    } else {
        num as u64
    }
}

/// 从 Video 段解析详细视频流
fn parse_detailed_video_streams(streams: &[ParsedStream]) -> Vec<DetailedVideoStream> {
    streams
        .iter()
        .filter(|s| s.section == "Video")
        .enumerate()
        .map(|(idx, stream)| {
            let f = &stream.fields;
            let duration_str = find_field(f, "Duration");
            let duration_ms = parse_duration_to_ms(&duration_str);
            let frame_rate_raw = find_field(f, "Frame rate");
            let frame_rate = if frame_rate_raw.is_empty() {
                find_field(f, "Original frame rate")
            } else {
                frame_rate_raw
            };

            DetailedVideoStream {
                index: idx as u32,
                // 大众级
                width: parse_number(&find_field(f, "Width")),
                height: parse_number(&find_field(f, "Height")),
                display_aspect_ratio: find_field(f, "Display aspect ratio"),
                frame_rate: format_frame_rate(&frame_rate),
                // 入门级
                codec: find_field(f, "Format"),
                bit_rate: find_field(f, "Bit rate"),
                frame_rate_mode: {
                    let mode = find_field(f, "Frame rate mode");
                    if mode.contains("Variable") {
                        "VFR".to_string()
                    } else if mode.is_empty() {
                        String::new()
                    } else {
                        "CFR".to_string()
                    }
                },
                bit_depth: find_field(f, "Bit depth"),
                hdr_format: find_field(f, "HDR format"),
                scan_type: find_field(f, "Scan type"),
                // 进阶级
                format_profile: find_field(f, "Format profile"),
                chroma_subsampling: find_field(f, "Chroma subsampling"),
                color_space: find_field(f, "Color space"),
                color_primaries: find_field(f, "Color primaries"),
                transfer_characteristics: find_field(f, "Transfer characteristics"),
                matrix_coefficients: find_field(f, "Matrix coefficients"),
                stream_size: find_field(f, "Stream size"),
                bits_per_pixel_frame: find_field(f, "Bits/(Pixel*Frame)"),
                language: find_field(f, "Language"),
                // 专业级
                cabac: find_field(f, "Format settings, CABAC"),
                format_settings_ref_frames: find_field(f, "Format settings, Reference frames"),
                encoded_library: find_field(f, "Encoded library"),
                encoded_library_settings: find_field(f, "Encoded library settings"),
                codec_id: find_field(f, "Codec ID"),
                duration: format_duration(duration_ms),
                duration_ms,
            }
        })
        .collect()
}

/// 从 Audio 段解析详细音频流
fn parse_detailed_audio_streams(streams: &[ParsedStream]) -> Vec<DetailedAudioStream> {
    streams
        .iter()
        .filter(|s| s.section == "Audio")
        .enumerate()
        .map(|(idx, stream)| {
            let f = &stream.fields;
            let duration_str = find_field(f, "Duration");
            let duration_ms = parse_duration_to_ms(&duration_str);

            DetailedAudioStream {
                index: idx as u32,
                // 大众级
                channels: find_field(f, "Channel(s)"),
                channel_layout: find_field(f, "Channel layout"),
                sample_rate: find_field(f, "Sampling rate"),
                // 入门级
                codec: find_field(f, "Format"),
                bit_rate: find_field(f, "Bit rate"),
                bit_rate_mode: find_field(f, "Bit rate mode"),
                is_default: find_field(f, "Default").eq_ignore_ascii_case("yes"),
                // 进阶级
                language: find_field(f, "Language"),
                title: find_field(f, "Title"),
                stream_size: find_field(f, "Stream size"),
                format_profile: find_field(f, "Format profile"),
                compression_mode: find_field(f, "Compression mode"),
                duration: format_duration(duration_ms),
                duration_ms,
                codec_id: find_field(f, "Codec ID"),
            }
        })
        .collect()
}

/// 从 Text 段解析详细字幕流
fn parse_detailed_text_streams(streams: &[ParsedStream]) -> Vec<DetailedTextStream> {
    streams
        .iter()
        .filter(|s| s.section == "Text")
        .enumerate()
        .map(|(idx, stream)| {
            let f = &stream.fields;
            DetailedTextStream {
                index: idx as u32,
                format: find_field(f, "Format"),
                codec_id: find_field(f, "Codec ID"),
                language: find_field(f, "Language"),
                title: find_field(f, "Title"),
                is_default: find_field(f, "Default").eq_ignore_ascii_case("yes"),
            }
        })
        .collect()
}

/// 获取完整的视频详细元数据（用于视频体检）
pub fn get_detailed_video_meta(path: &Path) -> Option<DetailedVideoMeta> {
    let mi = MediaInfoHandle::open(path)?;
    let full_info = mi.get_inform();
    if full_info.is_empty() {
        return None;
    }

    let streams = parse_mediainfo_text(&full_info);
    if streams.is_empty() {
        return None;
    }

    let general = parse_general_info(&streams);
    let video_streams = parse_detailed_video_streams(&streams);
    let audio_streams = parse_detailed_audio_streams(&streams);
    let mut text_streams = parse_detailed_text_streams(&streams);

    // 如果文本解析未找到字幕流，用 XML 输出解析
    if text_streams.is_empty() {
        let xml = mi.get_xml();
        text_streams = parse_text_streams_from_xml(&xml);
    }

    Some(DetailedVideoMeta {
        general,
        video_streams,
        audio_streams,
        text_streams,
    })
}

/// 获取视频的 MediaInfo XML 原始输出
pub fn get_video_xml(path: &Path) -> Option<String> {
    let mi = MediaInfoHandle::open(path)?;
    let xml = mi.get_xml();
    if xml.is_empty() { None } else { Some(xml) }
}

/// 从 MediaInfo XML 输出中解析 Text 流
fn parse_text_streams_from_xml(xml: &str) -> Vec<DetailedTextStream> {
    let mut streams = Vec::new();
    let mut idx = 0u32;
    let mut pos = 0;

    while let Some(start) = xml[pos..].find("<track type=\"Text\"") {
        let abs_start = pos + start;
        let end = xml[abs_start..].find("</track>").map(|e| abs_start + e);
        let Some(end) = end else { break };
        let block = &xml[abs_start..end];

        let get_xml_val = |tag: &str| -> String {
            let open = format!("<{tag}>");
            let close = format!("</{tag}>");
            if let Some(s) = block.find(&open) {
                let val_start = s + open.len();
                if let Some(e) = block[val_start..].find(&close) {
                    return block[val_start..val_start + e].to_string();
                }
            }
            String::new()
        };

        streams.push(DetailedTextStream {
            index: idx,
            format: get_xml_val("Format"),
            codec_id: get_xml_val("Codec_ID"),
            language: get_xml_val("Language"),
            title: get_xml_val("Title"),
            is_default: get_xml_val("Default").eq_ignore_ascii_case("yes"),
        });
        idx += 1;
        pos = end + 8; // skip "</track>"
    }

    streams
}
