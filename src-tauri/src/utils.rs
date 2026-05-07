use chrono::Local;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

pub fn ensure_dir(path: &PathBuf) -> std::io::Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

pub fn timestamped_log(dir: &Path, prefix: &str) -> PathBuf {
    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
    dir.join(format!("{prefix}_{ts}.log"))
}

pub fn emit_progress(
    app: &AppHandle,
    stage: &str,
    current: usize,
    total: usize,
    message: &str,
    id: Uuid,
) {
    let _ = app.emit(
        "progress-update",
        serde_json::json!({
          "id": id,
          "stage": stage,
          "current": current,
          "total": total.max(1),
          "message": message
        }),
    );
}
