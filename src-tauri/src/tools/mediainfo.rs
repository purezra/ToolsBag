//! MediaInfo.dll 动态加载封装
//! 用于获取视频、音频、图片的详细元数据

use crate::models::{AudioMeta, ImageMeta, VideoMeta};
use libloading::{Library, Symbol};
use once_cell::sync::OnceCell;
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::Mutex;

// 嵌入 MediaInfo.dll 到二进制
static MEDIAINFO_DLL_BYTES: &[u8] = include_bytes!("../../MediaInfo.dll");

// MediaInfo 函数类型定义 - 使用 system 调用约定 (Windows stdcall)
type MediaInfoNew = unsafe extern "system" fn() -> *mut std::ffi::c_void;
type MediaInfoDelete = unsafe extern "system" fn(*mut std::ffi::c_void);
type MediaInfoOpenW = unsafe extern "system" fn(*mut std::ffi::c_void, *const u16) -> usize;
type MediaInfoClose = unsafe extern "system" fn(*mut std::ffi::c_void);
// MediaInfo_Inform 用于获取完整信息
type MediaInfoInform = unsafe extern "system" fn(*mut std::ffi::c_void, usize) -> *const u16;

static MEDIAINFO_LIB: OnceCell<Option<Library>> = OnceCell::new();
static MEDIAINFO_PATH: OnceCell<Mutex<Option<String>>> = OnceCell::new();

/// 获取 MediaInfo.dll 的加载路径
pub fn get_mediainfo_path() -> Option<String> {
  MEDIAINFO_PATH.get()?.lock().ok()?.clone()
}

/// 释放嵌入的 DLL 到临时目录
fn extract_embedded_dll() -> Option<std::path::PathBuf> {
  let temp_dir = std::env::temp_dir();
  let dll_path = temp_dir.join("toolsbag_mediainfo.dll");
  
  // 如果已存在且大小匹配，直接使用
  if dll_path.exists() {
    if let Ok(meta) = std::fs::metadata(&dll_path) {
      if meta.len() == MEDIAINFO_DLL_BYTES.len() as u64 {
        return Some(dll_path);
      }
    }
  }
  
  // 写入 DLL
  match std::fs::write(&dll_path, MEDIAINFO_DLL_BYTES) {
    Ok(_) => {
      println!("[MediaInfo] Extracted DLL to: {:?}", dll_path);
      Some(dll_path)
    }
    Err(e) => {
      println!("[MediaInfo] Failed to extract DLL: {}", e);
      None
    }
  }
}

/// 初始化 MediaInfo 库
pub fn init_mediainfo() -> bool {
  MEDIAINFO_PATH.get_or_init(|| Mutex::new(None));
  
  MEDIAINFO_LIB.get_or_init(|| {
    // 优先尝试从嵌入的 DLL 加载
    if let Some(embedded_path) = extract_embedded_dll() {
      match unsafe { Library::new(&embedded_path) } {
        Ok(lib) => {
          println!("[MediaInfo] Loaded from embedded: {:?}", embedded_path);
          if let Some(mutex) = MEDIAINFO_PATH.get() {
            if let Ok(mut guard) = mutex.lock() {
              *guard = Some(format!("(内置) {}", embedded_path.display()));
            }
          }
          // 验证函数是否存在
          unsafe {
            if lib.get::<MediaInfoNew>(b"MediaInfo_New\0").is_ok() {
              println!("[MediaInfo] Found MediaInfo_New");
              return Some(lib);
            }
          }
        }
        Err(e) => {
          println!("[MediaInfo] Failed to load embedded: {}", e);
        }
      }
    }
    
    // 回退：尝试从外部路径加载
    let paths = [
      "MediaInfo.dll",
      "./MediaInfo.dll",
      "../MediaInfo.dll",
    ];
    
    for path in paths {
      match unsafe { Library::new(path) } {
        Ok(lib) => {
          println!("[MediaInfo] Loaded from: {}", path);
          if let Some(mutex) = MEDIAINFO_PATH.get() {
            if let Ok(mut guard) = mutex.lock() {
              *guard = Some(path.to_string());
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
          println!("[MediaInfo] Failed to load {}: {}", path, e);
        }
      }
    }
    
    println!("[MediaInfo] DLL not found in any path");
    None
  });
  
  is_mediainfo_available()
}

/// 使用 MediaInfo_Inform 获取完整信息（调试用）
pub fn get_full_info(path: &Path) -> Option<String> {
  let lib = MEDIAINFO_LIB.get()?.as_ref()?;
  
  unsafe {
    let mi_new: Symbol<MediaInfoNew> = lib.get(b"MediaInfo_New\0").ok()?;
    let mi_delete: Symbol<MediaInfoDelete> = lib.get(b"MediaInfo_Delete\0").ok()?;
    let mi_open: Symbol<MediaInfoOpenW> = lib.get(b"MediaInfo_Open\0").ok()?;
    let mi_close: Symbol<MediaInfoClose> = lib.get(b"MediaInfo_Close\0").ok()?;
    let mi_inform: Symbol<MediaInfoInform> = lib.get(b"MediaInfo_Inform\0").ok()?;
    
    let handle = mi_new();
    if handle.is_null() {
      return None;
    }
    
    let path_wide = to_wide_string(&path.to_string_lossy());
    let result = mi_open(handle, path_wide.as_ptr());
    
    if result == 0 {
      mi_delete(handle);
      return None;
    }
    
    // 获取完整信息
    let info_ptr = mi_inform(handle, 0);
    let info = from_wide_ptr(info_ptr);
    
    mi_close(handle);
    mi_delete(handle);
    
    Some(info)
  }
}

/// 检查 MediaInfo 是否可用
pub fn is_mediainfo_available() -> bool {
  MEDIAINFO_LIB.get().map(|opt| opt.is_some()).unwrap_or(false)
}

/// 转换为宽字符串 (Windows UTF-16)
fn to_wide_string(s: &str) -> Vec<u16> {
  OsStr::new(s).encode_wide().chain(once(0)).collect()
}

/// 从宽字符指针读取字符串
unsafe fn from_wide_ptr(ptr: *const u16) -> String {
  if ptr.is_null() {
    return String::new();
  }
  let mut len = 0;
  while *ptr.add(len) != 0 {
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
  let num_str: String = value.chars()
    .take_while(|c| c.is_ascii_digit() || *c == '.')
    .collect();
  
  if let Ok(fps) = num_str.parse::<f64>() {
    if fps == fps.floor() {
      // 整数
      format!("{}", fps as u32)
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
            frame_rate_mode = if value.contains("Variable") { "VFR" } else { "CFR" }.to_string();
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
    if (ratio - 16.0/9.0).abs() < 0.1 {
      aspect_ratio = "16:9".to_string();
    } else if (ratio - 4.0/3.0).abs() < 0.1 {
      aspect_ratio = "4:3".to_string();
    } else if (ratio - 9.0/16.0).abs() < 0.1 {
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
  let num_str: String = s_clean.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
  let num: f64 = num_str.parse().unwrap_or(0.0);
  
  if s_lower.contains("mb/s") || s_lower.contains("mbps") {
    (num * 1_000_000.0) as u64
  } else if s_lower.contains("kb/s") || s_lower.contains("kbps") {
    (num * 1_000.0) as u64
  } else {
    num as u64
  }
}

/// 解析时长到毫秒
fn parse_duration_to_ms(s: &str) -> u64 {
  // 尝试解析 "24 min 44 s" 格式
  let mut total_ms: u64 = 0;
  let parts: Vec<&str> = s.split_whitespace().collect();
  
  let mut i = 0;
  while i < parts.len() {
    if let Ok(num) = parts[i].parse::<u64>() {
      if i + 1 < parts.len() {
        let unit = parts[i + 1].to_lowercase();
        if unit.starts_with("h") {
          total_ms += num * 3600 * 1000;
        } else if unit.starts_with("min") || unit == "mn" {
          total_ms += num * 60 * 1000;
        } else if unit.starts_with("s") && !unit.starts_with("ms") {
          total_ms += num * 1000;
        } else if unit.starts_with("ms") {
          total_ms += num;
        }
        i += 2;
      } else {
        // 可能是毫秒数
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
          if format.is_empty() && (in_image_section || value.contains("JPEG") || value.contains("PNG") || value.contains("GIF") || value.contains("BMP") || value.contains("WebP")) {
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
