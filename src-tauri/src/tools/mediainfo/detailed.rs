//! 视频体检 - 详细元数据解析
//! 解析 MediaInfo 文本输出，提取 General/Video/Audio 段的详细字段

use crate::models::{
    DetailedAudioStream, DetailedTextStream, DetailedVideoMeta, DetailedVideoStream, GeneralInfo,
};
use crate::tools::mediainfo::parse::*;
use crate::tools::mediainfo::MediaInfoHandle;
use std::path::Path;

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
        let section_base = trimmed.split('#').next().unwrap_or("").trim();
        let is_section_header = if trimmed.contains(':') {
            let prefix = trimmed.split(':').next().unwrap_or("").trim();
            let base = prefix.split('#').next().unwrap_or("").trim();
            matches!(
                base,
                "General" | "Video" | "Audio" | "Text" | "Other" | "Menu" | "Image"
            )
        } else {
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
            current_section = base_name.split('#').next().unwrap_or("").trim().to_string();
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
fn parse_general_info(streams: &[ParsedStream], precise_duration_ms: u64) -> GeneralInfo {
    let general = streams.iter().find(|s| s.section == "General");
    let fields = general.map(|s| s.fields.as_slice()).unwrap_or(&[]);

    let file_size_str = find_field(fields, "File size");
    let file_size = parse_file_size(&file_size_str);
    let duration_str = find_field(fields, "Duration");
    let duration_ms_text = parse_duration_to_ms(&duration_str);
    // 优先用 Get 接口的精确毫秒值，fallback 到文本解析
    let duration_ms = if precise_duration_ms > 0 {
        precise_duration_ms
    } else {
        duration_ms_text
    };

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

/// 从 Video 段解析详细视频流
fn parse_detailed_video_streams(
    streams: &[ParsedStream],
    precise_durations: &[u64],
) -> Vec<DetailedVideoStream> {
    streams
        .iter()
        .filter(|s| s.section == "Video")
        .enumerate()
        .map(|(idx, stream)| {
            let f = &stream.fields;
            let duration_str = find_field(f, "Duration");
            let duration_ms_text = parse_duration_to_ms(&duration_str);
            let duration_ms = if idx < precise_durations.len() && precise_durations[idx] > 0 {
                precise_durations[idx]
            } else {
                duration_ms_text
            };
            let frame_rate_raw = find_field(f, "Frame rate");
            let frame_rate = if frame_rate_raw.is_empty() {
                find_field(f, "Original frame rate")
            } else {
                frame_rate_raw
            };

            DetailedVideoStream {
                index: idx as u32,
                width: parse_number(&find_field(f, "Width")),
                height: parse_number(&find_field(f, "Height")),
                display_aspect_ratio: find_field(f, "Display aspect ratio"),
                frame_rate: format_frame_rate(&frame_rate),
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
                format_profile: find_field(f, "Format profile"),
                chroma_subsampling: find_field(f, "Chroma subsampling"),
                color_space: find_field(f, "Color space"),
                color_primaries: find_field(f, "Color primaries"),
                transfer_characteristics: find_field(f, "Transfer characteristics"),
                matrix_coefficients: find_field(f, "Matrix coefficients"),
                stream_size: find_field(f, "Stream size"),
                bits_per_pixel_frame: find_field(f, "Bits/(Pixel*Frame)"),
                language: find_field(f, "Language"),
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

fn fill_missing_video_core_fields<F>(streams: &mut [DetailedVideoStream], mut get: F)
where
    F: FnMut(usize, &str) -> String,
{
    for (index, stream) in streams.iter_mut().enumerate() {
        if stream.width == 0 {
            stream.width = parse_number(&get(index, "Width"));
        }
        if stream.height == 0 {
            stream.height = parse_number(&get(index, "Height"));
        }
        if stream.codec.is_empty() {
            stream.codec = get(index, "Format");
        }
        if stream.frame_rate.is_empty() {
            stream.frame_rate = format_frame_rate(&get(index, "FrameRate"));
        }
        if stream.hdr_format.is_empty() {
            stream.hdr_format = get(index, "HDR_Format");
        }
    }
}

/// 从 Audio 段解析详细音频流
fn parse_detailed_audio_streams(
    streams: &[ParsedStream],
    precise_durations: &[u64],
) -> Vec<DetailedAudioStream> {
    streams
        .iter()
        .filter(|s| s.section == "Audio")
        .enumerate()
        .map(|(idx, stream)| {
            let f = &stream.fields;
            let duration_str = find_field(f, "Duration");
            let duration_ms_text = parse_duration_to_ms(&duration_str);
            let duration_ms = if idx < precise_durations.len() && precise_durations[idx] > 0 {
                precise_durations[idx]
            } else {
                duration_ms_text
            };

            DetailedAudioStream {
                index: idx as u32,
                channels: find_field(f, "Channel(s)"),
                channel_layout: find_field(f, "Channel layout"),
                sample_rate: find_field(f, "Sampling rate"),
                codec: find_field(f, "Format"),
                bit_rate: find_field(f, "Bit rate"),
                bit_rate_mode: find_field(f, "Bit rate mode"),
                is_default: find_field(f, "Default").eq_ignore_ascii_case("yes"),
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

/// 获取完整的视频详细元数据（用于视频体检）
pub fn get_detailed_video_meta(path: &Path) -> Option<DetailedVideoMeta> {
    let mi = MediaInfoHandle::open(path)?;
    let full_info = mi.get_inform();
    if full_info.is_empty() {
        return None;
    }

    // 优先用 Get 接口直接取精确毫秒，绕过 Inform 文本省略秒数的问题
    let precise_duration_ms = mi.get_duration_ms();

    let streams = parse_mediainfo_text(&full_info);
    if streams.is_empty() {
        return None;
    }

    let general = parse_general_info(&streams, precise_duration_ms);

    // Collect precise per-stream durations via Get API
    let video_count = streams.iter().filter(|s| s.section == "Video").count();
    let audio_count = streams.iter().filter(|s| s.section == "Audio").count();
    let video_precise: Vec<u64> = (0..video_count)
        .map(|i| mi.get_stream_duration_ms(1, i))
        .collect();
    let audio_precise: Vec<u64> = (0..audio_count)
        .map(|i| mi.get_stream_duration_ms(2, i))
        .collect();

    let mut video_streams = parse_detailed_video_streams(&streams, &video_precise);
    fill_missing_video_core_fields(&mut video_streams, |index, parameter| {
        mi.get_stream_value(1, index, parameter)
    });
    let audio_streams = parse_detailed_audio_streams(&streams, &audio_precise);

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

#[cfg(test)]
mod tests {
    use super::fill_missing_video_core_fields;
    use crate::models::DetailedVideoStream;

    #[test]
    fn fills_missing_core_fields_without_overwriting_valid_values() {
        let mut streams = vec![DetailedVideoStream {
            codec: "kept".to_string(),
            ..Default::default()
        }];

        fill_missing_video_core_fields(&mut streams, |_, parameter| match parameter {
            "Width" => "1280".to_string(),
            "Height" => "720".to_string(),
            "Format" => "AVC".to_string(),
            "FrameRate" => "30.000".to_string(),
            "HDR_Format" => "HDR10".to_string(),
            _ => String::new(),
        });

        assert_eq!(streams[0].width, 1280);
        assert_eq!(streams[0].height, 720);
        assert_eq!(streams[0].codec, "kept");
        assert_eq!(streams[0].frame_rate, "30");
        assert_eq!(streams[0].hdr_format, "HDR10");
    }
}
