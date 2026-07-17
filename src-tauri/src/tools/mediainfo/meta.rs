//! 基础元数据提取
//! 使用 MediaInfo 文本输出解析视频、音频、图片的基础元数据

use crate::models::{AudioMeta, ImageMeta, VideoMeta};
use crate::tools::mediainfo::{parse::*, MediaInfoHandle};
use std::path::Path;

/// 获取视频元数据
pub fn get_video_meta(path: &Path) -> Option<VideoMeta> {
    // 轻量提取：ParseSpeed=0 只读容器头部，避免深度扫描整段文件，批量导入显著提速。
    let mi = MediaInfoHandle::open_fast(path)?;
    let full_info = mi.get_inform();

    if full_info.is_empty() {
        return None;
    }

    // 优先用 Get 接口直接取精确毫秒，绕过 Inform 文本省略秒数的问题
    let duration_ms_precise = mi.get_duration_ms();

    // 解析完整信息
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let mut bitrate_raw: u64 = 0;
    let mut aspect_ratio = String::new();
    let mut frame_rate = String::new();
    let mut frame_rate_mode = "CFR".to_string();
    let mut codec = String::new();
    let mut hdr_format: Option<String> = None;
    let mut duration_ms_text: u64 = 0;

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
                        duration_ms_text = parse_duration_to_ms(value);
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
                        if duration_ms_text == 0 {
                            duration_ms_text = parse_duration_to_ms(value);
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

    // 优先用 Get 接口的精确毫秒值，fallback 到文本解析
    let duration_ms = if duration_ms_precise > 0 {
        duration_ms_precise
    } else {
        duration_ms_text
    };

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

/// 获取音频元数据
pub fn get_audio_meta(path: &Path) -> Option<AudioMeta> {
    let mi = MediaInfoHandle::open_fast(path)?;
    let full_info = mi.get_inform();

    if full_info.is_empty() {
        return None;
    }

    let duration_ms_precise = mi.get_duration_ms();

    let mut codec = String::new();
    let mut sample_rate = String::new();
    let mut bitrate_raw: u64 = 0;
    let mut duration_ms_text: u64 = 0;

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
                        duration_ms_text = parse_duration_to_ms(value);
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
                        if duration_ms_text == 0 {
                            duration_ms_text = parse_duration_to_ms(value);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let duration_ms = if duration_ms_precise > 0 {
        duration_ms_precise
    } else {
        duration_ms_text
    };

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
    // 图片仅需宽高/格式，ParseSpeed=0 读头部即可。
    let mi = MediaInfoHandle::open_fast(path)?;
    let full_info = mi.get_inform();

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
