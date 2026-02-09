use crate::models::{FormatStat, MediaFileInfo, MediaPreviewFile, MediaPreviewRequest, MediaPreviewResult, TraverseProblem, TraverseRequest, TraverseResult};
use crate::tools::{mediainfo, detect_media_type};
use crate::utils::{emit_progress, ensure_dir, timestamped_log};
use rayon::prelude::*;
use std::{
  collections::HashMap,
  fs::{self, File},
  io::{BufReader, BufWriter, Read, Write},
  path::{Path, PathBuf},
  sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex,
  },
  time::Instant,
};
use tauri::AppHandle;
use uuid::Uuid;
use walkdir::WalkDir;

/// 判断媒体类型（返回字符串，兼容原有逻辑）
fn get_media_type(ext: &str) -> Option<&'static str> {
  detect_media_type(ext).map(|t| t.as_str())
}

pub fn traverse_and_copy(app: AppHandle, req: TraverseRequest) -> Result<TraverseResult, String> {
  let input = req.input_dir.clone();
  if !input.exists() {
    return Err("输入目录不存在".into());
  }
  if !input.is_dir() {
    return Err("输入路径不是目录".into());
  }

  // 媒体模式下初始化 MediaInfo
  let media_mode = req.media_only_mode;
  if media_mode {
    mediainfo::init_mediainfo();
  }

  let input_name = input.file_name().and_then(|s| s.to_str()).unwrap_or("output");
  let base_dir = input.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
  let default_out = base_dir.join(format!("{input_name}_汇总"));
  let output_dir = req.output_dir.clone().unwrap_or(default_out);
  ensure_dir(&output_dir).map_err(|e| e.to_string())?;
  let log_path = timestamped_log(&output_dir, "traverse_log");

  let min_size = req.min_size_mb.unwrap_or(0) * 1024 * 1024;
  let max_size = req.max_size_mb.unwrap_or(0) * 1024 * 1024;
  
  // 媒体模式下忽略用户的 include_patterns
  let include_patterns: Vec<String> = if media_mode {
    Vec::new()
  } else {
    req.include_patterns
      .iter()
      .map(|s| s.trim().to_lowercase())
      .filter(|s| !s.is_empty())
      .collect()
  };
  let exclude_patterns: Vec<String> = req
    .exclude_patterns
    .iter()
    .map(|s| s.trim().to_lowercase())
    .filter(|s| !s.is_empty())
    .collect();

  let mut files = Vec::new();
  let mut format_map: HashMap<String, usize> = HashMap::new();
  let mut filtered_map: HashMap<String, usize> = HashMap::new();
  let mut scanned_map: HashMap<String, usize> = HashMap::new();
  
  for entry in WalkDir::new(&input).follow_links(false).into_iter().filter_map(|e| e.ok()) {
    if !entry.file_type().is_file() {
      continue;
    }
    let path = entry.path().to_path_buf();
    
    let ext = path
      .extension()
      .and_then(|s| s.to_str())
      .map(|s| s.to_ascii_lowercase())
      .unwrap_or_else(|| "bin".into());
    
    let name = path
      .file_name()
      .and_then(|s| s.to_str())
      .unwrap_or_default()
      .to_lowercase();
    
    // 媒体模式：只接受媒体文件
    if media_mode {
      if get_media_type(&ext).is_none() {
        continue;
      }
    } else {
      // 普通模式：检查 include patterns
      if !include_patterns.is_empty() && !matches_patterns(&include_patterns, &name) {
        continue;
      }
    }
    
    scanned_map.entry(ext.clone()).and_modify(|c| *c += 1).or_insert(1);
    
    let md = match fs::metadata(&path) {
      Ok(m) => m,
      Err(_) => {
        // Metadata read failed - count as filtered
        filtered_map.entry(ext.clone()).and_modify(|c| *c += 1).or_insert(1);
        continue;
      }
    };
    let size = md.len();
    
    // Check exclude patterns
    if matches_patterns(&exclude_patterns, &name) {
      filtered_map.entry(ext.clone()).and_modify(|c| *c += 1).or_insert(1);
      continue;
    }
    
    // Check size filters
    if min_size > 0 && size < min_size {
      filtered_map.entry(ext.clone()).and_modify(|c| *c += 1).or_insert(1);
      continue;
    }
    if max_size > 0 && size > max_size {
      filtered_map.entry(ext.clone()).and_modify(|c| *c += 1).or_insert(1);
      continue;
    }

    let rel = path.strip_prefix(&input).unwrap_or(&path);
    let rel_parent = rel.parent().unwrap_or_else(|| Path::new(""));
    let prefix = if rel_parent.as_os_str().is_empty() {
      "root".to_string()
    } else {
      rel_parent
        .to_string_lossy()
        .replace(['\\', '/'], "_")
        .trim_matches('_')
        .to_string()
    };

    format_map.entry(ext.clone()).and_modify(|c| *c += 1).or_insert(1);

    files.push((path, ext, prefix, size));
  }

  if files.is_empty() {
    return Err("未找到符合条件的文件".into());
  }

  let total = files.len();
  let problems: Arc<Mutex<Vec<TraverseProblem>>> = Arc::new(Mutex::new(Vec::new()));
  let copied = Arc::new(AtomicUsize::new(0));
  let failed = Arc::new(AtomicUsize::new(0));
  let bytes_copied = Arc::new(AtomicU64::new(0));
  let id = Uuid::new_v4();
  let start = Instant::now();

  // 媒体模式下按类型分组，普通模式按格式分组
  let organize = req.organize_by_format || media_mode;
  
  // 媒体模式：创建 video/audio/image 目录
  if media_mode {
    let _ = ensure_dir(&output_dir.join("video"));
    let _ = ensure_dir(&output_dir.join("audio"));
    let _ = ensure_dir(&output_dir.join("image"));
  }
  
  let group_map = if organize && !media_mode { 
    Some(build_group_map(&format_map)) 
  } else { 
    None 
  };

  // 媒体模式：按子文件夹分组计数，用于生成 文件夹_序号 命名
  let mut subfolder_counters: HashMap<(String, String), usize> = HashMap::new(); // (media_type, subfolder) -> count

  let tasks: Vec<(usize, PathBuf, String, String, String, u64)> = files
    .iter()
    .enumerate()
    .map(|(idx, (path, ext, prefix, size))| {
      let original_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
      
      let (dest, new_name) = if media_mode {
        let media_type = get_media_type(ext).unwrap_or("other");
        let key = (media_type.to_string(), prefix.clone());
        let counter = subfolder_counters.entry(key).or_insert(0);
        *counter += 1;
        let new_name = format!("{}_{}.{}", prefix, counter, ext);
        let dest = output_dir.join(media_type).join(&new_name);
        (dest, new_name)
      } else if organize {
        let grp = group_map
          .as_ref()
          .and_then(|m| m.get(ext))
          .cloned()
          .unwrap_or_else(|| ext.clone());
        let new_name = format!("{:05}_{}.{}", idx + 1, original_name, ext);
        let dest = output_dir.join(grp).join(&new_name);
        (dest, new_name)
      } else {
        let new_name = format!("{}_{}.{}", prefix, idx + 1, ext);
        let dest = output_dir.join(&new_name);
        (dest, new_name)
      };
      (idx, path.clone(), ext.clone(), dest.to_string_lossy().to_string(), new_name, *size)
    })
    .collect();

  // Pre-create format/group dirs if needed (非媒体模式)
  if organize && !media_mode {
    if let Some(map) = &group_map {
      for grp in map.values() {
        let _ = ensure_dir(&output_dir.join(grp));
      }
    }
  }

  // 媒体文件信息收集（媒体模式）
  let media_files: Arc<Mutex<Vec<MediaFileInfo>>> = Arc::new(Mutex::new(Vec::new()));

  tasks.par_iter().for_each(|(_idx, src, ext, dest_str, new_name, size)| {
    let dest = PathBuf::from(dest_str);
    let _ = ensure_dir(&dest.parent().unwrap_or_else(|| Path::new("")).to_path_buf());
    let res = copy_fast(src, &dest);
    match res {
      Ok(bytes) => {
        copied.fetch_add(1, Ordering::Relaxed);
        bytes_copied.fetch_add(bytes, Ordering::Relaxed);
        
        // 媒体模式：获取元数据
        if media_mode {
          let media_type = get_media_type(ext).unwrap_or("other");
          let file_name = src.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
          
          let mut info = MediaFileInfo {
            path: dest.clone(),
            name: file_name,
            size: *size,
            media_type: media_type.to_string(),
            new_name: new_name.clone(),
            video: None,
            audio: None,
            image: None,
          };
          
          // 获取元数据
          match media_type {
            "video" => {
              info.video = mediainfo::get_video_meta(&dest);
            }
            "audio" => {
              info.audio = mediainfo::get_audio_meta(&dest);
            }
            "image" => {
              info.image = mediainfo::get_image_meta(&dest);
            }
            _ => {}
          }
          
          if let Ok(mut guard) = media_files.lock() {
            guard.push(info);
          }
        }
      }
      Err(e) => {
        failed.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut guard) = problems.lock() {
          guard.push(TraverseProblem {
            path: src.clone(),
            error: e,
          });
        }
      }
    }
    let current = copied.load(Ordering::Relaxed) + failed.load(Ordering::Relaxed);
    if current % 10 == 0 || current == total {
      emit_progress(
        &app,
        "traverse",
        current,
        total,
        src.to_string_lossy().as_ref(),
        id,
      );
    }
  });

  let elapsed_ms = start.elapsed().as_millis();
  let copied_count = copied.load(Ordering::Relaxed);
  let failed_count = failed.load(Ordering::Relaxed);
  let skipped = total.saturating_sub(copied_count + failed_count);
  let total_bytes = bytes_copied.load(Ordering::Relaxed);
  let throughput_bytes = if elapsed_ms > 0 {
    (total_bytes * 1000 / elapsed_ms as u64).max(1)
  } else {
    0
  };

  let format_stats: Vec<FormatStat> = format_map
    .into_iter()
    .map(|(ext, count)| {
      let scanned = scanned_map.get(&ext).copied().unwrap_or(0);
      let filtered = filtered_map.get(&ext).copied().unwrap_or(0);
      FormatStat { ext, scanned, count, filtered }
    })
    .collect();

  // Build detailed source tracing log grouped by parent folder
  let input_str = input.to_string_lossy().to_string();
  let mut source_groups: HashMap<String, Vec<String>> = HashMap::new();
  for (path, _ext, _prefix, _size) in &files {
    let parent = path.parent()
      .map(|p| {
        let p_str = p.to_string_lossy().to_string();
        // Replace input dir with * for shorter display
        if p_str == input_str {
          "*".to_string()
        } else {
          p_str.replace(&input_str, "*")
        }
      })
      .unwrap_or_else(|| "*".to_string());
    let filename = path.file_name()
      .and_then(|s| s.to_str())
      .unwrap_or("unknown")
      .to_string();
    source_groups.entry(parent).or_default().push(filename);
  }
  
  let mut source_log = String::new();
  let mut sorted_groups: Vec<_> = source_groups.into_iter().collect();
  sorted_groups.sort_by(|a, b| a.0.cmp(&b.0));
  
  for (folder, filenames) in sorted_groups {
    source_log.push_str(&format!("\n{}:\n", folder));
    for name in filenames {
      source_log.push_str(&format!("  {}\n", name));
    }
  }
  
  let log_content = format!(
    "========== 提取汇总 ==========\n\
     输入目录: {:?}\n\
     输出目录: {:?}\n\
     总文件数: {}\n\
     成功复制: {}\n\
     失败: {}\n\
     总字节数: {}\n\
     耗时(ms): {}\n\
     媒体模式: {}\n\n\
     ========== 源文件溯源 =========={}\n",
    input,
    output_dir,
    total,
    copied_count,
    failed_count,
    total_bytes,
    elapsed_ms,
    media_mode,
    source_log
  );
  let _ = fs::write(&log_path, log_content);

  // 获取媒体文件列表
  let media_files_result = if media_mode {
    media_files.lock().map(|g| g.clone()).unwrap_or_default()
  } else {
    Vec::new()
  };

  Ok(TraverseResult {
    total_files: total,
    copied: copied_count,
    skipped,
    failed: failed_count,
    output_dir: output_dir.clone(),
    elapsed_ms,
    throughput_bytes,
    format_stats,
    problems: problems
      .lock()
      .map(|g| g.clone())
      .unwrap_or_default(),
    log_path,
    media_files: media_files_result,
  })
}

fn copy_fast(src: &Path, dest: &Path) -> Result<u64, String> {
  let mut reader = BufReader::with_capacity(64 * 1024, File::open(src).map_err(|e| e.to_string())?);
  let mut writer = BufWriter::with_capacity(64 * 1024, File::create(dest).map_err(|e| e.to_string())?);
  let mut total: u64 = 0;
  let mut buffer = [0u8; 64 * 1024];
  loop {
    let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
    if n == 0 {
      break;
    }
    writer.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
    total += n as u64;
  }
  writer.flush().map_err(|e| e.to_string())?;
  Ok(total)
}

fn build_group_map(format_map: &HashMap<String, usize>) -> HashMap<String, String> {
  let mut result = HashMap::new();

  let add_category = |result: &mut HashMap<String, String>, names: &[&str], single: bool| {
    let mut present: Vec<(String, usize)> = names
      .iter()
      .filter_map(|e| format_map.get(&e.to_string()).map(|c| (e.to_string(), *c)))
      .collect();
    if present.is_empty() {
      return;
    }
    present.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let folder = if single {
      present[0].0.clone()
    } else {
      present.iter().map(|(e, _)| e.clone()).collect::<Vec<_>>().join("_")
    };
    for (ext, _) in present {
      result.insert(ext, folder.clone());
    }
  };

  // grouped categories
  add_category(
    &mut result,
    &["jpg", "jpeg", "png", "bmp", "tiff", "webp"],
    false,
  );
  add_category(
    &mut result,
    &["mp4", "avi", "mov", "wmv", "mkv", "flv", "webm"],
    false,
  );
  add_category(&mut result, &["mp3", "wav", "flac", "ogg", "aac", "m4a"], false);
  add_category(&mut result, &["doc", "docx", "rtf"], false);
  add_category(&mut result, &["xls", "xlsx", "csv"], false);
  add_category(&mut result, &["ppt", "pptx"], false);

  // singles
  add_category(&mut result, &["gif"], true);
  add_category(&mut result, &["ts"], true);
  add_category(&mut result, &["torrent"], true);
  add_category(&mut result, &["pdf"], true);
  add_category(&mut result, &["txt", "md"], false);

  // others one folder each
  for ext in format_map.keys() {
    if !result.contains_key(ext) {
      result.insert(ext.clone(), ext.clone());
    }
  }

  result
}

fn matches_patterns(patterns: &[String], name: &str) -> bool {
  if patterns.is_empty() {
    return false;
  }
  patterns.iter().any(|p| wildcard_match(p, name))
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
  wildcard_match_bytes(pattern.as_bytes(), text.as_bytes())
}

fn wildcard_match_bytes(pat: &[u8], text: &[u8]) -> bool {
  let (mut pi, mut ti, mut star) = (0usize, 0usize, None::<usize>);
  while ti < text.len() {
    if pi < pat.len() && (pat[pi] == b'?' || pat[pi] == text[ti]) {
      pi += 1;
      ti += 1;
    } else if pi < pat.len() && pat[pi] == b'*' {
      star = Some(pi);
      pi += 1;
      if pi == pat.len() {
        return true;
      }
    } else if let Some(s) = star {
      pi = s + 1;
      ti += 1;
    } else {
      return false;
    }
  }
  while pi < pat.len() && pat[pi] == b'*' {
    pi += 1;
  }
  pi == pat.len()
}


/// 媒体预览：扫描目录并获取所有媒体文件的元数据
pub fn preview_media(app: AppHandle, req: MediaPreviewRequest) -> Result<MediaPreviewResult, String> {
  let input = req.input_dir.clone();
  if !input.exists() {
    return Err("输入目录不存在".into());
  }
  if !input.is_dir() {
    return Err("输入路径不是目录".into());
  }

  // 初始化 MediaInfo
  mediainfo::init_mediainfo();

  let mut videos: Vec<MediaPreviewFile> = Vec::new();
  let mut audios: Vec<MediaPreviewFile> = Vec::new();
  let mut images: Vec<MediaPreviewFile> = Vec::new();

  // 收集所有媒体文件
  let mut media_files: Vec<(PathBuf, String, String, u64)> = Vec::new(); // (path, ext, media_type, size)
  
  for entry in WalkDir::new(&input).follow_links(false).into_iter().filter_map(|e| e.ok()) {
    if !entry.file_type().is_file() {
      continue;
    }
    let path = entry.path().to_path_buf();
    let ext = path
      .extension()
      .and_then(|s| s.to_str())
      .map(|s| s.to_ascii_lowercase())
      .unwrap_or_default();
    
    if let Some(media_type) = get_media_type(&ext) {
      let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
      media_files.push((path, ext, media_type.to_string(), size));
    }
  }

  let total = media_files.len();
  let id = Uuid::new_v4();

  // 并行获取元数据
  let results: Vec<MediaPreviewFile> = media_files
    .par_iter()
    .enumerate()
    .map(|(idx, (path, ext, media_type, size))| {
      let name = path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
      
      let mut file = MediaPreviewFile {
        path: path.clone(),
        name,
        ext: ext.clone(),
        size: *size,
        media_type: media_type.clone(),
        video: None,
        audio: None,
        image: None,
      };

      // 获取元数据
      match media_type.as_str() {
        "video" => {
          file.video = mediainfo::get_video_meta(path);
        }
        "audio" => {
          file.audio = mediainfo::get_audio_meta(path);
        }
        "image" => {
          file.image = mediainfo::get_image_meta(path);
        }
        _ => {}
      }

      // 发送进度
      if idx % 10 == 0 || idx == total - 1 {
        emit_progress(&app, "media_preview", idx + 1, total, &file.name, id);
      }

      file
    })
    .collect();

  // 分类
  for file in results {
    match file.media_type.as_str() {
      "video" => videos.push(file),
      "audio" => audios.push(file),
      "image" => images.push(file),
      _ => {}
    }
  }

  Ok(MediaPreviewResult {
    videos,
    audios,
    images,
  })
}
