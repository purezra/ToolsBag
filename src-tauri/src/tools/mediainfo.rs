//! MediaInfo.dll 动态加载封装
//! 用于获取视频、音频、图片的详细元数据

use crate::models::{AudioMeta, ImageMeta, VideoMeta};
use libloading::Library;
use once_cell::sync::OnceCell;
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// MediaInfo 函数类型定义 - 使用 system 调用约定 (Windows stdcall)
type FnNew = unsafe extern "system" fn() -> *mut std::ffi::c_void;
type FnDelete = unsafe extern "system" fn(*mut std::ffi::c_void);
type FnOpenW = unsafe extern "system" fn(*mut std::ffi::c_void, *const u16) -> usize;
type FnClose = unsafe extern "system" fn(*mut std::ffi::c_void);
type FnInform = unsafe extern "system" fn(*mut std::ffi::c_void, usize) -> *const u16;
type FnOption = unsafe extern "system" fn(*mut std::ffi::c_void, *const u16, *const u16) -> *const u16;

/// 缓存的 MediaInfo 函数指针，避免每次调用重复查找符号
struct MediaInfoSymbols {
    new_fn: FnNew,
    delete_fn: FnDelete,
    open_fn: FnOpenW,
    close_fn: FnClose,
    inform_fn: FnInform,
    option_fn: FnOption,
}

// Safety: 函数指针来自已加载的 DLL，只要 DLL 不卸载就有效，
// 而 Library 存储在 static OnceCell 中永远不会卸载。
unsafe impl Send for MediaInfoSymbols {}
unsafe impl Sync for MediaInfoSymbols {}

static MEDIAINFO_LIB: OnceCell<Option<Library>> = OnceCell::new();
static MEDIAINFO_SYMS: OnceCell<Option<MediaInfoSymbols>> = OnceCell::new();
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

    // 从内嵌资源释放到临时目录（原子写入，避免多进程竞态）
    let temp_dll = std::env::temp_dir().join("toolsbag_mediainfo.dll");
    if !temp_dll.exists() {
        let temp_part = std::env::temp_dir().join(format!("toolsbag_mediainfo_{}.dll", std::process::id()));
        if std::fs::write(&temp_part, EMBEDDED_MEDIAINFO_DLL).is_ok() {
            // rename 是原子操作，如果目标已存在则忽略错误
            let _ = std::fs::rename(&temp_part, &temp_dll);
        }
        // 清理可能残留的临时文件
        let _ = std::fs::remove_file(&temp_part);
    }
    if temp_dll.exists() {
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
                        if lib.get::<FnNew>(b"MediaInfo_New\0").is_ok() {
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

    // 缓存所有函数指针，避免后续每次调用重复查找符号
    MEDIAINFO_SYMS.get_or_init(|| {
        let lib = MEDIAINFO_LIB.get()?.as_ref()?;
        unsafe {
            let new_fn: FnNew = *lib.get::<FnNew>(b"MediaInfo_New\0").ok()?;
            let delete_fn: FnDelete = *lib.get::<FnDelete>(b"MediaInfo_Delete\0").ok()?;
            let open_fn: FnOpenW = *lib.get::<FnOpenW>(b"MediaInfo_Open\0").ok()?;
            let close_fn: FnClose = *lib.get::<FnClose>(b"MediaInfo_Close\0").ok()?;
            let inform_fn: FnInform = *lib.get::<FnInform>(b"MediaInfo_Inform\0").ok()?;
            let option_fn: FnOption = *lib.get::<FnOption>(b"MediaInfo_Option\0").ok()?;
            println!("[MediaInfo] All 6 FFI symbols cached");
            Some(MediaInfoSymbols { new_fn, delete_fn, open_fn, close_fn, inform_fn, option_fn })
        }
    });

    is_mediainfo_available()
}

/// MediaInfo 句柄包装器（RAII）
struct MediaInfoHandle {
    handle: *mut std::ffi::c_void,
}

impl MediaInfoHandle {
    /// 打开文件，返回句柄（使用缓存的函数指针）
    fn open(path: &Path) -> Option<Self> {
        let syms = MEDIAINFO_SYMS.get()?.as_ref()?;
        unsafe {
            let handle = (syms.new_fn)();
            if handle.is_null() {
                return None;
            }

            let path_wide = to_wide_string(&path.to_string_lossy());
            if (syms.open_fn)(handle, path_wide.as_ptr()) == 0 {
                (syms.delete_fn)(handle);
                return None;
            }

            Some(MediaInfoHandle { handle })
        }
    }

    /// 获取 Inform 文本输出
    fn get_inform(&self) -> String {
        let syms = match MEDIAINFO_SYMS.get() {
            Some(Some(s)) => s,
            _ => return String::new(),
        };
        unsafe {
            let opt_key = to_wide_string("Inform");
            let opt_val = to_wide_string("");
            (syms.option_fn)(self.handle, opt_key.as_ptr(), opt_val.as_ptr());

            let info_ptr = (syms.inform_fn)(self.handle, 0);
            from_wide_ptr(info_ptr)
        }
    }

    /// 获取完整信息输出（Complete 模式，包含所有字段）
    fn get_complete(&self) -> String {
        let syms = match MEDIAINFO_SYMS.get() {
            Some(Some(s)) => s,
            _ => return String::new(),
        };
        unsafe {
            // 先清除 Inform 模板，避免残留的自定义模板覆盖 Complete 输出
            let clear_key = to_wide_string("Inform");
            let clear_val = to_wide_string("");
            (syms.option_fn)(self.handle, clear_key.as_ptr(), clear_val.as_ptr());

            // 启用 Complete 模式（等同于 CLI 的 --Full）
            let opt_key = to_wide_string("Complete");
            let opt_val = to_wide_string("1");
            (syms.option_fn)(self.handle, opt_key.as_ptr(), opt_val.as_ptr());

            let info_ptr = (syms.inform_fn)(self.handle, 0);
            from_wide_ptr(info_ptr)
        }
    }

    /// 获取 XML 输出
    fn get_xml(&self) -> String {
        let syms = match MEDIAINFO_SYMS.get() {
            Some(Some(s)) => s,
            _ => return String::new(),
        };
        unsafe {
            let opt_key = to_wide_string("Inform");
            let opt_val = to_wide_string("XML");
            (syms.option_fn)(self.handle, opt_key.as_ptr(), opt_val.as_ptr());

            let info_ptr = (syms.inform_fn)(self.handle, 0);
            from_wide_ptr(info_ptr)
        }
    }
}

impl Drop for MediaInfoHandle {
    fn drop(&mut self) {
        if let Some(Some(syms)) = MEDIAINFO_SYMS.get() {
            unsafe {
                (syms.close_fn)(self.handle);
                (syms.delete_fn)(self.handle);
            }
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
    let mut in_general_section = false;

    for line in full_info.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // 检测段落标题（支持 "Video"、"Video #1" 格式）
        let section_base = if line.contains(':') {
            let prefix = line.split(':').next().unwrap_or("").trim();
            prefix.split('#').next().unwrap_or("").trim()
        } else {
            line.split('#').next().unwrap_or("").trim()
        };

        if section_base == "Video" {
            in_video_section = true;
            in_general_section = false;
            continue;
        } else if section_base == "General" {
            in_video_section = false;
            in_general_section = true;
            continue;
        } else if section_base == "Audio"
            || section_base == "Text"
            || section_base == "Menu"
            || section_base == "Image"
        {
            in_video_section = false;
            in_general_section = false;
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
            } else if in_general_section {
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
/// 支持格式：
/// - MediaInfo 标准格式："2 h 30 min 45 s 123 ms"（空格分隔）
/// - MediaInfo 连写格式："2h 30min 45s 123ms"（数字和单位连写）
/// - 纯毫秒："1502456"
/// - HH:MM:SS 或 HH:MM:SS.mmm
fn parse_duration_to_ms(s: &str) -> u64 {
    let s = s.trim();
    if s.is_empty() {
        return 0;
    }

    // 尝试 HH:MM:SS 或 HH:MM:SS.mmm 格式
    if let Some(ms) = try_parse_hhmmss(s) {
        return ms;
    }

    let mut total_ms: u64 = 0;
    let parts: Vec<&str> = s.split_whitespace().collect();

    let mut i = 0;
    while i < parts.len() {
        let part = parts[i];

        // 尝试解析纯数字
        if let Ok(num) = part.parse::<u64>() {
            if i + 1 < parts.len() {
                let unit = parts[i + 1].to_lowercase();
                let ms = parse_time_unit(num, &unit);
                total_ms = total_ms.saturating_add(ms);
                i += 2;
            } else {
                // 最后一个 token 是纯数字，视为毫秒
                total_ms = total_ms.saturating_add(num);
                i += 1;
            }
        } else if let Some((num, unit)) = split_number_and_unit(part) {
            // 连写格式："2h"、"30min"、"45s"、"123ms"
            let ms = parse_time_unit(num, &unit);
            total_ms = total_ms.saturating_add(ms);
            i += 1;
        } else {
            i += 1;
        }
    }

    total_ms
}

/// 尝试从 "2h"、"30min"、"45s"、"123ms" 中拆分数字和单位
fn split_number_and_unit(s: &str) -> Option<(u64, String)> {
    let num_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if num_end == 0 {
        return None;
    }
    let num_str = &s[..num_end];
    let unit = &s[num_end..];
    if num_str.is_empty() || unit.is_empty() {
        return None;
    }
    let num = num_str.parse::<u64>().ok()?;
    Some((num, unit.to_lowercase()))
}

/// 解析单个时间单位
fn parse_time_unit(num: u64, unit: &str) -> u64 {
    if unit.starts_with("h") {
        num.saturating_mul(3600).saturating_mul(1000)
    } else if unit.starts_with("min") || unit == "mn" {
        num.saturating_mul(60).saturating_mul(1000)
    } else if unit.starts_with("s") && !unit.starts_with("ms") {
        num.saturating_mul(1000)
    } else if unit.starts_with("ms") {
        num
    } else {
        0
    }
}

/// 尝试解析 HH:MM:SS 或 HH:MM:SS.mmm 格式
fn try_parse_hhmmss(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let h: u64 = parts[0].trim().parse().ok()?;
        let m: u64 = parts[1].trim().parse().ok()?;
        let s_part = parts[2].trim();
        // 处理可能的小数秒 "SS.mmm"
        if let Some(dot_pos) = s_part.find('.') {
            let sec: u64 = s_part[..dot_pos].parse().unwrap_or(0);
            let frac_str = &s_part[dot_pos + 1..];
            // 补齐到3位毫秒
            let frac_ms: u64 = match frac_str.len() {
                1 => frac_str.parse::<u64>().unwrap_or(0) * 100,
                2 => frac_str.parse::<u64>().unwrap_or(0) * 10,
                3 => frac_str.parse::<u64>().unwrap_or(0),
                _ => frac_str.parse::<u64>().unwrap_or(0) / 10_u64.pow(frac_str.len() as u32 - 3),
            };
            let total_ms = h * 3600_000 + m * 60_000 + sec * 1000 + frac_ms;
            Some(total_ms)
        } else {
            let sec: u64 = s_part.parse().unwrap_or(0);
            Some(h * 3600_000 + m * 60_000 + sec * 1000)
        }
    } else {
        None
    }
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
    let mut in_general_section = false;

    for line in full_info.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // 检测段落标题（支持 "Audio"、"Audio #1" 格式）
        let section_base = if line.contains(':') {
            let prefix = line.split(':').next().unwrap_or("").trim();
            prefix.split('#').next().unwrap_or("").trim()
        } else {
            line.split('#').next().unwrap_or("").trim()
        };

        if section_base == "Audio" {
            in_audio_section = true;
            in_general_section = false;
            continue;
        } else if section_base == "General" {
            in_audio_section = false;
            in_general_section = true;
            continue;
        } else if section_base == "Video"
            || section_base == "Text"
            || section_base == "Menu"
            || section_base == "Image"
        {
            in_audio_section = false;
            in_general_section = false;
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
            } else if in_general_section {
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

        // 支持 "Image #1" 格式
        let base = line.split('#').next().unwrap_or("").trim();
        if base == "Image" {
            in_image_section = true;
            continue;
        } else if base == "Video" || base == "Audio" || base == "Text" {
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
        // MediaInfo 输出格式可能是 "General"、"Video"、"Text" 或 "Video #1"、"Audio #2" 等
        let section_base = trimmed.split('#').next().unwrap_or("").trim();
        let is_section_header = if trimmed.contains(':') {
            // 带冒号的行：可能是 "Text #1: Chinese" 这种格式
            let prefix = trimmed.split(':').next().unwrap_or("").trim();
            let base = prefix.split('#').next().unwrap_or("").trim();
            matches!(
                base,
                "General" | "Video" | "Audio" | "Text" | "Other" | "Menu" | "Image"
            )
        } else {
            // 不带冒号：可能是 "Audio #1" 或纯 "Audio"
            matches!(
                section_base,
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
            // 提取基础段名（去掉编号和冒号后缀）
            let base_name = if trimmed.contains(':') {
                trimmed.split(':').next().unwrap_or("").trim()
            } else {
                trimmed
            };
            current_section = base_name
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

    // 始终使用 XML 解析字幕流（更可靠，且避免重复读取文件）
    let xml = mi.get_xml();
    let text_streams = parse_text_streams_from_xml(&xml);

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

/// 获取视频的 MediaInfo 完整信息文本（Complete 模式）
pub fn get_complete_info(path: &Path) -> Option<String> {
    let mi = MediaInfoHandle::open(path)?;
    let info = mi.get_complete();
    if info.is_empty() { None } else { Some(info) }
}

/// 获取视频 XML 输出并转为结构化 JSON（对齐 Python 工具格式）
pub fn get_xml_as_json(path: &Path) -> Option<String> {
    let mi = MediaInfoHandle::open(path)?;
    let xml = mi.get_xml();
    if xml.is_empty() {
        return None;
    }
    Some(xml_to_json_str(&xml, path))
}

/// 获取视频 XML 输出并转为 Markdown 表格（对齐 Python 工具格式）
pub fn get_xml_as_markdown(path: &Path) -> Option<String> {
    let mi = MediaInfoHandle::open(path)?;
    let xml = mi.get_xml();
    if xml.is_empty() {
        return None;
    }
    Some(xml_to_markdown_str(&xml, path))
}

// ==================== XML 转换工具函数 ====================

/// 从 XML 中提取所有 track 块
fn extract_tracks(xml: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut tracks = Vec::new();
    let mut pos = 0;

    while let Some(start) = xml[pos..].find("<track") {
        let abs_start = pos + start;
        // 找到 type 属性
        let track_tag_end = xml[abs_start..].find('>').map(|e| abs_start + e);
        let Some(tag_end) = track_tag_end else { break };
        let tag_block = &xml[abs_start..tag_end];

        let track_type = tag_block
            .find("type=\"")
            .and_then(|i| {
                let val_start = abs_start + i + 6;
                xml[val_start..].find('"').map(|e| &xml[val_start..val_start + e])
            })
            .unwrap_or("General")
            .to_string();

        let end = xml[abs_start..].find("</track>").map(|e| abs_start + e);
        let Some(end) = end else { break };
        let block = &xml[tag_end + 1..end];

        let mut fields = Vec::new();
        let mut fpos = 0;
        while let Some(f_start) = block[fpos..].find('<') {
            let abs_f = fpos + f_start;
            if block.as_bytes().get(abs_f + 1) == Some(&b'/') {
                // closing tag, skip
                if let Some(gt) = block[abs_f..].find('>') {
                    fpos = abs_f + gt + 1;
                    continue;
                }
                break;
            }
            let tag_end_pos = block[abs_f..].find('>').map(|e| abs_f + e);
            let Some(tag_end_pos) = tag_end_pos else { break };
            let tag_name = &block[abs_f + 1..tag_end_pos];
            if tag_name.is_empty() || tag_name.contains(' ') {
                fpos = tag_end_pos + 1;
                continue;
            }
            let close_tag = format!("</{}>", tag_name);
            if let Some(c_start) = block[tag_end_pos + 1..].find(&close_tag) {
                let val = &block[tag_end_pos + 1..tag_end_pos + 1 + c_start];
                let val_trimmed = val.trim();
                if !val_trimmed.is_empty() {
                    fields.push((tag_name.to_string(), val_trimmed.to_string()));
                }
                fpos = tag_end_pos + 1 + c_start + close_tag.len();
            } else {
                fpos = tag_end_pos + 1;
            }
        }

        tracks.push((track_type, fields));
        pos = end + 8;
    }

    tracks
}

/// XML 转 JSON 字符串（对齐 Python format_xml2json_export）
fn xml_to_json_str(xml: &str, path: &Path) -> String {
    let tracks = extract_tracks(xml);
    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut out = serde_json::Map::new();
    out.insert("文件名".to_string(), serde_json::Value::String(filename));

    for (track_type, fields) in &tracks {
        let fields_map: serde_json::Map<String, serde_json::Value> = fields
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        let val = serde_json::Value::Object(fields_map);

        match track_type.as_str() {
            "General" => {
                out.insert("基本信息".to_string(), val);
            }
            "Video" => {
                let arr = out
                    .entry("视频流".to_string())
                    .or_insert_with(|| serde_json::Value::Array(Vec::new()));
                if let serde_json::Value::Array(a) = arr {
                    a.push(val);
                }
            }
            "Audio" => {
                let arr = out
                    .entry("音频流".to_string())
                    .or_insert_with(|| serde_json::Value::Array(Vec::new()));
                if let serde_json::Value::Array(a) = arr {
                    a.push(val);
                }
            }
            "Text" => {
                let arr = out
                    .entry("字幕流".to_string())
                    .or_insert_with(|| serde_json::Value::Array(Vec::new()));
                if let serde_json::Value::Array(a) = arr {
                    a.push(val);
                }
            }
            other => {
                let key = format!("其他流_{}", other);
                let arr = out
                    .entry(key)
                    .or_insert_with(|| serde_json::Value::Array(Vec::new()));
                if let serde_json::Value::Array(a) = arr {
                    a.push(val);
                }
            }
        }
    }

    serde_json::to_string_pretty(&serde_json::Value::Object(out)).unwrap_or_default()
}

/// XML 转 Markdown 字符串（对齐 Python format_xml2md_export）
fn xml_to_markdown_str(xml: &str, path: &Path) -> String {
    let tracks = extract_tracks(xml);
    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let filepath = path.to_string_lossy();

    let mut lines: Vec<String> = vec![
        format!("## {}", filename),
        String::new(),
        format!("**路径:** `{}`", filepath),
        String::new(),
    ];

    // 基本信息
    if let Some((_, fields)) = tracks.iter().find(|(t, _)| t == "General") {
        lines.push("### 基本信息".to_string());
        lines.push(String::new());
        lines.push("| 属性 | 值 |".to_string());
        lines.push("|------|-----|".to_string());
        for (k, v) in fields {
            lines.push(format!("| {} | {} |", k, v));
        }
        lines.push(String::new());
    }

    // 视频流
    let mut vi = 0;
    for (track_type, fields) in &tracks {
        if track_type != "Video" {
            continue;
        }
        vi += 1;
        lines.push(format!("### 视频流 #{}", vi));
        lines.push(String::new());
        lines.push("| 属性 | 值 |".to_string());
        lines.push("|------|-----|".to_string());
        for (k, v) in fields {
            lines.push(format!("| {} | {} |", k, v));
        }
        lines.push(String::new());
    }

    // 音频流
    let mut ai = 0;
    for (track_type, fields) in &tracks {
        if track_type != "Audio" {
            continue;
        }
        ai += 1;
        lines.push(format!("### 音频流 #{}", ai));
        lines.push(String::new());
        lines.push("| 属性 | 值 |".to_string());
        lines.push("|------|-----|".to_string());
        for (k, v) in fields {
            lines.push(format!("| {} | {} |", k, v));
        }
        lines.push(String::new());
    }

    // 字幕流
    let text_tracks: Vec<_> = tracks.iter().filter(|(t, _)| t == "Text").collect();
    if !text_tracks.is_empty() {
        for (si, (_, fields)) in text_tracks.iter().enumerate() {
            lines.push(format!("### 字幕流 #{}", si + 1));
            lines.push(String::new());
            lines.push("| 属性 | 值 |".to_string());
            lines.push("|------|-----|".to_string());
            for (k, v) in fields {
                lines.push(format!("| {} | {} |", k, v));
            }
            lines.push(String::new());
        }
    }

    // 其他流
    for (track_type, fields) in &tracks {
        if matches!(track_type.as_str(), "General" | "Video" | "Audio" | "Text") {
            continue;
        }
        lines.push(format!("### {}", track_type));
        lines.push(String::new());
        lines.push("| 属性 | 值 |".to_string());
        lines.push("|------|-----|".to_string());
        for (k, v) in fields {
            lines.push(format!("| {} | {} |", k, v));
        }
        lines.push(String::new());
    }

    lines.join("\n")
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
