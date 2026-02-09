use crate::detector::inspect_image;
use crate::models::{RepairRequest, RepairResult};
use crate::utils::{emit_progress, ensure_dir, timestamped_log};
use image::codecs::jpeg::JpegEncoder;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use uuid::Uuid;
use walkdir::WalkDir;

pub fn repair(app: AppHandle, req: RepairRequest) -> Result<RepairResult, String> {
  let id = Uuid::new_v4();
  let input = req.input_dir.clone();
  let output_dir = req
    .output_dir
    .clone()
    .unwrap_or_else(|| input.join("_fixed"));
  ensure_dir(&output_dir).map_err(|e| e.to_string())?;
  let log_path = timestamped_log(&output_dir, "repair_log");

  let mut files: Vec<PathBuf> = Vec::new();
  for entry in WalkDir::new(&input).into_iter().filter_map(|e| e.ok()) {
    if entry.file_type().is_file() {
      files.push(entry.path().to_path_buf());
    }
  }

  let total = files.len();
  let repaired = AtomicUsize::new(0);
  let copied = AtomicUsize::new(0);
  let failed = AtomicUsize::new(0);
  let output_dir = Arc::new(output_dir);
  let app = Arc::new(app);

  files.par_iter().enumerate().for_each(|(idx, path)| {
    emit_progress(
      &app,
      "repair",
      idx + 1,
      total,
      path.to_string_lossy().as_ref(),
      id,
    );
    let out_path = output_dir.join(
      path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string()
        + ".jpg",
    );

    let (problem, _reasons) = inspect_image(path);
    if !problem && req.only_problem {
      if std::fs::copy(path, &out_path).is_ok() {
        copied.fetch_add(1, Ordering::Relaxed);
      } else {
        failed.fetch_add(1, Ordering::Relaxed);
      }
      return;
    }

    match image::open(path) {
      Ok(img) => {
        let rgb = img.to_rgb8();
        let file = std::fs::File::create(&out_path);
        match file {
          Ok(f) => {
            let mut enc = JpegEncoder::new_with_quality(f, 95);
            if enc
              .encode(&rgb, rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
              .is_ok()
            {
              repaired.fetch_add(1, Ordering::Relaxed);
            } else {
              failed.fetch_add(1, Ordering::Relaxed);
            }
          }
          Err(_) => {
            failed.fetch_add(1, Ordering::Relaxed);
          }
        }
      }
      Err(_) => {
        failed.fetch_add(1, Ordering::Relaxed);
      }
    }
  });

  let log_content = format!(
    "Input: {:?}\nOutput: {:?}\nTotal: {total}\nRepaired: {}\nCopied: {}\nFailed: {}\n",
    input,
    output_dir,
    repaired.load(Ordering::Relaxed),
    copied.load(Ordering::Relaxed),
    failed.load(Ordering::Relaxed)
  );
  let _ = std::fs::write(&log_path, log_content);

  Ok(RepairResult {
    repaired: repaired.load(Ordering::Relaxed),
    copied: copied.load(Ordering::Relaxed),
    failed: failed.load(Ordering::Relaxed),
    output_dir: Arc::try_unwrap(output_dir).unwrap_or_else(|a| (*a).clone()),
    log_path,
  })
}
