use crate::models::{AnalyzeRequest, AnalyzeResult, ImageIssue};
use crate::utils::{emit_progress, timestamped_log};
use infer;
use once_cell::sync::Lazy;
use rayon::prelude::*;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use uuid::Uuid;
use walkdir::WalkDir;
use image::GenericImageView;

static SUPPORTED: Lazy<Vec<&'static str>> = Lazy::new(|| vec![ 
    // 图片
    "jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp",
    // 视频
    "mp4", "mkv", "mov", "avi", "wmv", "flv", "webm"]);

pub fn analyze(app: AppHandle, req: AnalyzeRequest) -> Result<AnalyzeResult, String> {
  let id = Uuid::new_v4();
  let mut files: Vec<PathBuf> = Vec::new();
  for entry in WalkDir::new(&req.input_dir)
    .follow_links(false)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter(|e| e.file_type().is_file())
  {
    let p = entry.path().to_path_buf();
    if let Some(ext) = p.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()) {
      if SUPPORTED.contains(&ext.as_str()) {
        files.push(p);
      }
    }
    if !req.recursive {
      if entry.depth() > 0 {
        break;
      }
    }
  }

  let total = files.len();
  let log_dir = req.input_dir.join("_analysis_logs");
  let _ = std::fs::create_dir_all(&log_dir);
  let log_path = timestamped_log(&log_dir, "image_analysis_log");
  let mut log = String::new();

  let problems: Vec<ImageIssue> = files
    .par_iter()
    .enumerate()
    .filter_map(|(idx, path)| {
      emit_progress(&app, "analyze", idx + 1, total, path.to_string_lossy().as_ref(), id);
      let (problem, reasons) = inspect_image(path);
      if problem {
        Some(ImageIssue {
          path: path.clone(),
          reasons,
        })
      } else {
        None
      }
    })
    .collect();

  log.push_str(&format!(
    "Total: {total}\nProblems: {}\nCompatible: {}\n",
    problems.len(),
    total.saturating_sub(problems.len())
  ));
  for issue in &problems {
    log.push_str(&format!(
      "{} => {}\n",
      issue.path.to_string_lossy(),
      issue.reasons.join(", ")
    ));
  }
  let _ = std::fs::write(&log_path, log);

  Ok(AnalyzeResult {
    total_files: total,
    problem_files: problems.len(),
    compatible_files: total.saturating_sub(problems.len()),
    log_path,
  })
}

pub fn inspect_image(path: &Path) -> (bool, Vec<String>) {
  let mut reasons = Vec::new();
  // magic check
  let mut header = [0u8; 12];
  if let Ok(mut f) = File::open(path) {
    if f.read(&mut header).is_ok() {
      if infer::get(&header).is_none() {
        reasons.push("文件头异常".into());
      }
    }
  } else {
    reasons.push("无法读取文件".into());
    return (true, reasons);
  }

  let reader = match image::ImageReader::open(path).and_then(|r| r.with_guessed_format()) {
    Ok(r) => r,
    Err(e) => {
      reasons.push(format!("无法打开或识别格式: {e}"));
      return (true, reasons);
    }
  };
  let img = match reader.decode() {
    Ok(img) => img,
    Err(e) => {
      reasons.push(format!("无法解码: {e}"));
      return (true, reasons);
    }
  };

  let color = img.color();
  let has_alpha = color.has_alpha();
  let bits_pp = color.bits_per_pixel() as u32;
  let channels = color.channel_count() as u32;
  let bits_per_channel = if channels > 0 { bits_pp / channels } else { bits_pp };
  let (w, h) = img.dimensions();
  if has_alpha {
    reasons.push("包含透明通道".into());
  }
  if bits_per_channel > 8 {
    reasons.push("位深度>8".into());
  }
  if w as u64 * h as u64 > 100_000_000 {
    reasons.push("分辨率过大".into());
  }

  (reasons.len() > 0, reasons)
}
