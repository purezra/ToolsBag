use crate::error::{AppError, AppResult};
use crate::models::{VideoInfoImportResponse, VideoInfoItem};
use crate::tools::{detect_media_type_from_path, media_cache, mediainfo, MediaType};
use crate::utils::emit_progress;
use exif;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    io::{BufReader, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;
use walkdir::WalkDir;

const MAX_SCAN_FILES: usize = 50_000;
const MAX_SCAN_DEPTH: usize = 32;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPerformanceRecord {
    mode: String,
    input_count: usize,
    total: usize,
    success: usize,
    failed: usize,
    metadata_ms: f64,
    display_ms: f64,
    total_ms: f64,
    cached: bool,
}

#[tauri::command]
pub fn record_media_performance(
    app: AppHandle,
    record: MediaPerformanceRecord,
) -> Result<String, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join("media-performance.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    let recorded_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let entry = serde_json::json!({
        "recordedAtMs": recorded_at_ms,
        "record": record,
    });
    writeln!(file, "{}", entry).map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// 解析 MediaInfo 输出的码率字符串（如 "10.0 Mbps"、"320 kb/s"）为 Mbps 数值
fn parse_bitrate_to_mbps(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || s == "-" {
        return None;
    }
    if let Some(rest) = s.strip_suffix("Mbps") {
        return rest.trim().parse::<f64>().ok();
    }
    let (kb_suffix, kb_len) = if s.ends_with("kb/s") || s.ends_with("Kbps") {
        (true, 4)
    } else {
        (false, 0)
    };
    if kb_suffix {
        let rest = &s[..s.len() - kb_len];
        return rest.trim().parse::<f64>().ok().map(|v| v / 1000.0);
    }
    if let Some(rest) = s.strip_suffix("bps") {
        return rest.trim().parse::<f64>().ok().map(|v| v / 1_000_000.0);
    }
    None
}

type VideoProbeResult = (
    Option<f64>,
    Option<u32>,
    Option<u32>,
    Option<f64>,
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

#[derive(Serialize, Deserialize)]
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
    pub arrival_time_ms: Option<u64>,
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
                    arrival_time_ms: None,
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
    let path_str = path.to_string_lossy().to_string();
    let md = std::fs::metadata(path)?;
    let created_ms = md
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);
    let mtime_ms = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);
    let arrival_time_ms = created_ms.into_iter().chain(mtime_ms).max();

    // 缓存命中则直接返回（覆盖缓存中的旧 id）
    if let Some(mtime) = mtime_ms {
        if let Some(cached) = media_cache::get_cached_item(&path_str, mtime) {
            if let Ok(mut item) = serde_json::from_value::<MediaItem>(cached) {
                item.id = idx;
                item.arrival_time_ms = arrival_time_ms;
                return Ok(item);
            }
        }
    }

    let media_type = detect_media_type_from_path(path)
        .ok_or_else(|| AppError::UnsupportedFormat(path.to_string_lossy().to_string()))?;
    let size = md.len();
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".into());

    let item = if media_type == MediaType::Video {
        // 轻量提取：只取重命名需要的扁平字段
        let (duration, width, height, bitrate, err) = probe_video(path);
        MediaItem {
            id: idx,
            name,
            path: path_str.clone(),
            size,
            media_type: media_type.as_str().to_string(),
            duration_sec: duration,
            width,
            height,
            bitrate_mbps: bitrate,
            arrival_time_ms,
            device: None,
            taken_at: None,
            focal_length: None,
            status: "success".to_string(),
            reason: err,
        }
    } else {
        // 图片：MediaInfo 轻量提取 + kamadak-exif 补充
        let (width, height, device, taken_at, focal_length, err) = probe_image(path);
        MediaItem {
            id: idx,
            name,
            path: path_str.clone(),
            size,
            media_type: media_type.as_str().to_string(),
            duration_sec: None,
            width,
            height,
            bitrate_mbps: None,
            arrival_time_ms,
            device,
            taken_at,
            focal_length,
            status: "success".to_string(),
            reason: err,
        }
    };

    // 写入缓存
    if let Some(mtime) = mtime_ms {
        if let Ok(value) = serde_json::to_value(&item) {
            media_cache::set_cached_item(&path_str, mtime, &value);
        }
    }

    Ok(item)
}

/// 视频轻量提取：只用 MediaInfo get_video_meta，不走深度提取
fn probe_video(path: &Path) -> VideoProbeResult {
    if !mediainfo::is_mediainfo_available() {
        return (None, None, None, None, Some("MediaInfo 不可用".to_string()));
    }
    let meta = match mediainfo::get_video_meta(path) {
        Some(m) => m,
        None => {
            return (
                None,
                None,
                None,
                None,
                Some("MediaInfo 提取失败".to_string()),
            )
        }
    };
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
    (duration, Some(meta.width), Some(meta.height), bitrate, None)
}

/// 图片轻量提取：MediaInfo get_image_meta + kamadak-exif 补充 device/takenAt/focalLength
fn probe_image(path: &Path) -> ImageProbeResult {
    // 常见位图格式用纯 Rust image crate 直接读头部尺寸：无 FFI、无完整报告生成，
    // 显著快于 MediaInfo，批量导入图片提速明显。HEIC/HEIF 等 image crate 无法解码
    // 尺寸的格式回退到 MediaInfo。
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let image_crate_supported = matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "tif" | "webp"
    );
    if image_crate_supported {
        if let Ok((w, h)) = image::image_dimensions(path) {
            let (device, taken_at, focal_length) = read_exif(path);
            return (Some(w), Some(h), device, taken_at, focal_length, None);
        }
    }

    // MediaInfo（HEIC/HEIF 或上面读取失败时）
    if mediainfo::is_mediainfo_available() {
        if let Some(img_meta) = mediainfo::get_image_meta(path) {
            // 用 kamadak-exif 补充 EXIF 字段
            let (device, taken_at, focal_length) = read_exif(path);
            return (
                Some(img_meta.width),
                Some(img_meta.height),
                device,
                taken_at,
                focal_length,
                None,
            );
        }
    }
    // MediaInfo 不可用或提取失败：fallback 到 image crate + kamadak-exif
    probe_image_fallback(path)
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
            let path_str = path.to_string_lossy().to_string();
            let md = std::fs::metadata(path).ok();
            let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
            let mtime_ms = md
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as u64);
            let cache_key = format!("detailed:{}", path_str);

            let item = mtime_ms
                .and_then(|mtime| media_cache::get_cached_item(&cache_key, mtime))
                .and_then(|value| serde_json::from_value::<VideoInfoItem>(value).ok())
                .map(|mut cached| {
                    cached.id = idx as u64;
                    cached
                })
                .unwrap_or_else(|| {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".into());
                    let detail = mediainfo::get_detailed_video_meta(path)
                        .or_else(|| mediainfo::get_detailed_video_meta(path));

                    // 从 detail 派生扁平字段
                    let (duration_sec, width, height, bitrate_mbps, codec, frame_rate) =
                        if let Some(ref d) = detail {
                            let dur = if d.general.duration_ms > 0 {
                                Some(d.general.duration_ms as f64 / 1000.0)
                            } else {
                                d.video_streams.first().and_then(|s| {
                                    if s.duration_ms > 0 {
                                        Some(s.duration_ms as f64 / 1000.0)
                                    } else {
                                        None
                                    }
                                })
                            };
                            let vs = d.video_streams.first();
                            let br = parse_bitrate_to_mbps(&d.general.overall_bit_rate)
                                .or_else(|| vs.and_then(|s| parse_bitrate_to_mbps(&s.bit_rate)));
                            let fr = vs.and_then(|s| {
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
                                br,
                                vs.map(|s| s.codec.clone()),
                                fr,
                            )
                        } else {
                            (None, None, None, None, None, None)
                        };

                    let item = VideoInfoItem {
                        id: idx as u64,
                        name,
                        path: path_str.clone(),
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

                    // 失败项不缓存，后续导入仍会重试解析。
                    if item.status == "success" {
                        if let (Some(mtime), Ok(value)) = (mtime_ms, serde_json::to_value(&item)) {
                            media_cache::set_cached_item(&cache_key, mtime, &value);
                        }
                    }
                    item
                });

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

/// 获取视频文件的 MediaInfo XML 原始输出
#[tauri::command]
pub async fn get_video_raw_xml(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = std::path::Path::new(&path);
        if !p.exists() {
            return Err("文件不存在".to_string());
        }
        Ok(mediainfo::get_video_xml(p))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 获取视频文件的 MediaInfo 完整信息文本（Complete 模式）
#[tauri::command]
pub async fn get_video_complete_info(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = std::path::Path::new(&path);
        if !p.exists() {
            return Err("文件不存在".to_string());
        }
        Ok(mediainfo::get_complete_info(p))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 获取视频 XML 输出并转为结构化 JSON
#[tauri::command]
pub async fn get_video_xml_json(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = std::path::Path::new(&path);
        if !p.exists() {
            return Err("文件不存在".to_string());
        }
        Ok(mediainfo::get_xml_as_json(p))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 获取视频 XML 输出并转为 Markdown 表格
#[tauri::command]
pub async fn get_video_xml_markdown(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = std::path::Path::new(&path);
        if !p.exists() {
            return Err("文件不存在".to_string());
        }
        Ok(mediainfo::get_xml_as_markdown(p))
    })
    .await
    .map_err(|e| e.to_string())?
}
