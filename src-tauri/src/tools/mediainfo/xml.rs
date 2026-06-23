//! MediaInfo XML 解析与格式转换
//! XML 输出提取、JSON/Markdown 转换

use crate::tools::mediainfo::MediaInfoHandle;
use std::path::Path;

/// 获取视频的 MediaInfo XML 原始输出
pub fn get_video_xml(path: &Path) -> Option<String> {
    let mi = MediaInfoHandle::open(path)?;
    let xml = mi.get_xml();
    if xml.is_empty() {
        None
    } else {
        Some(xml)
    }
}

/// 获取视频的 MediaInfo 完整信息文本（Complete 模式）
pub fn get_complete_info(path: &Path) -> Option<String> {
    let mi = MediaInfoHandle::open(path)?;
    let info = mi.get_complete();
    if info.is_empty() {
        None
    } else {
        Some(info)
    }
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
                xml[val_start..]
                    .find('"')
                    .map(|e| &xml[val_start..val_start + e])
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
            let Some(tag_end_pos) = tag_end_pos else {
                break;
            };
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
