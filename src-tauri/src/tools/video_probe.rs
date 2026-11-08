//! 视频元数据探测（不含 Tauri 依赖）
//!
//! 桌面端「视频体检 → 导入」和 `tools v <目录>` CLI 共用这里的实现，
//! 保证两边提取字段、缓存键、失败判定完全一致。

use crate::models::{DetailedVideoMeta, VideoInfoItem};
use crate::tools::{detect_media_type_from_path, media_cache, mediainfo, MediaType};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const MAX_SCAN_FILES: usize = 50_000;
pub const MAX_SCAN_DEPTH: usize = 32;

/// 解析 MediaInfo 输出的码率字符串（如 "10.0 Mbps"、"320 kb/s"）为 Mbps 数值
pub fn parse_bitrate_to_mbps(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || s == "-" {
        return None;
    }
    // MediaInfo 的数字有时带千分位空格（如 "8 000 kb/s"），先去掉所有空白再解析。
    let number = |raw: &str| -> Option<f64> {
        let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
        cleaned.parse::<f64>().ok()
    };
    if let Some(rest) = s.strip_suffix("Mbps") {
        return number(rest);
    }
    let is_kb = s.ends_with("kb/s") || s.ends_with("Kbps");
    if is_kb {
        return number(&s[..s.len() - 4]).map(|v| v / 1000.0);
    }
    if let Some(rest) = s.strip_suffix("bps") {
        return number(rest).map(|v| v / 1_000_000.0);
    }
    None
}

/// 展开输入路径，只保留视频文件（目录默认递归）。
pub fn collect_video_paths(input: &[String], recursive: bool) -> Result<Vec<PathBuf>, String> {
    let mut result = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for path_str in input {
        let p = PathBuf::from(path_str);
        if !p.exists() {
            continue;
        }

        if p.is_file() {
            if detect_media_type_from_path(&p) == Some(MediaType::Video) && seen.insert(p.clone()) {
                result.push(p);
            }
            continue;
        }

        if !p.is_dir() {
            continue;
        }

        let files: Box<dyn Iterator<Item = PathBuf>> = if recursive {
            Box::new(
                WalkDir::new(&p)
                    .max_depth(MAX_SCAN_DEPTH + 1)
                    .follow_links(false)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .map(|e| e.into_path()),
            )
        } else {
            Box::new(
                std::fs::read_dir(&p)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|path| path.is_file()),
            )
        };

        for path in files {
            if detect_media_type_from_path(&path) != Some(MediaType::Video) {
                continue;
            }
            if seen.insert(path.clone()) {
                result.push(path);
            }
            if result.len() > MAX_SCAN_FILES {
                return Err(format!(
                    "扫描文件数超过上限（{}），请缩小范围后重试",
                    MAX_SCAN_FILES
                ));
            }
        }
    }

    result.sort();
    Ok(result)
}

type FlatFields = (
    Option<f64>,
    Option<u32>,
    Option<u32>,
    Option<f64>,
    Option<String>,
    Option<String>,
);

/// 从详细元数据派生前端表格直接使用的扁平字段。
pub fn derive_flat_fields(detail: &DetailedVideoMeta) -> FlatFields {
    let dur = if detail.general.duration_ms > 0 {
        Some(detail.general.duration_ms as f64 / 1000.0)
    } else {
        detail.video_streams.first().and_then(|s| {
            if s.duration_ms > 0 {
                Some(s.duration_ms as f64 / 1000.0)
            } else {
                None
            }
        })
    };
    let vs = detail.video_streams.first();
    let bitrate = parse_bitrate_to_mbps(&detail.general.overall_bit_rate)
        .or_else(|| vs.and_then(|s| parse_bitrate_to_mbps(&s.bit_rate)));
    let frame_rate = vs.and_then(|s| {
        if s.frame_rate.is_empty() || s.frame_rate == "-" {
            None
        } else {
            Some(s.frame_rate.clone())
        }
    });

    (
        dur,
        vs.map(|s| s.width),
        vs.map(|s| s.height),
        bitrate,
        vs.map(|s| s.codec.clone()),
        frame_rate,
    )
}

/// 提取单个视频的详细元数据。
///
/// 缓存键为 `detailed:<path>`，与桌面端一致；失败项不写缓存。
pub fn probe_video_item(idx: u64, path: &Path) -> VideoInfoItem {
    let path_str = path.to_string_lossy().to_string();
    let md = std::fs::metadata(path).ok();
    let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime_ms = md
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64);
    let cache_key = format!("detailed:{}", path_str);

    if let Some(mtime) = mtime_ms {
        if let Some(cached) = media_cache::get_cached_item(&cache_key, mtime) {
            if let Ok(mut item) = serde_json::from_value::<VideoInfoItem>(cached) {
                item.id = idx;
                return item;
            }
        }
    }

    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    // 某些损坏/特殊封装的视频第一次打开可能拿不到流信息，重试一次。
    let detail =
        mediainfo::get_detailed_video_meta(path).or_else(|| mediainfo::get_detailed_video_meta(path));

    let (duration_sec, width, height, bitrate_mbps, codec, frame_rate) = detail
        .as_ref()
        .map(derive_flat_fields)
        .unwrap_or((None, None, None, None, None, None));

    let item = VideoInfoItem {
        id: idx,
        name,
        path: path_str,
        size,
        status: if detail.is_some() { "success" } else { "error" }.to_string(),
        reason: if detail.is_none() {
            Some("无法解析视频元数据".to_string())
        } else {
            None
        },
        detail,
        duration_sec,
        width,
        height,
        bitrate_mbps,
        codec,
        frame_rate,
    };

    if item.status == "success" {
        if let (Some(mtime), Ok(value)) = (mtime_ms, serde_json::to_value(&item)) {
            media_cache::set_cached_item(&cache_key, mtime, &value);
        }
    }

    item
}

#[cfg(test)]
mod tests {
    use super::parse_bitrate_to_mbps;

    #[test]
    fn parses_common_bitrate_units() {
        assert_eq!(parse_bitrate_to_mbps("10.0 Mbps"), Some(10.0));
        assert_eq!(parse_bitrate_to_mbps("320 kb/s"), Some(0.32));
        assert_eq!(parse_bitrate_to_mbps("8 000 kb/s"), Some(8.0));
        assert_eq!(parse_bitrate_to_mbps("1 000 000 bps"), Some(1.0));
        assert_eq!(parse_bitrate_to_mbps("-"), None);
        assert_eq!(parse_bitrate_to_mbps(""), None);
    }
}
