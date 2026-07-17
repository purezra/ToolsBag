use chrono::Local;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

pub fn timestamped_log(dir: &Path, prefix: &str) -> PathBuf {
    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
    dir.join(format!("{prefix}_{ts}.log"))
}

const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(80);

static PROGRESS_LAST_EMIT: Mutex<Option<HashMap<(String, Uuid), Instant>>> =
    Mutex::new(None);

pub fn emit_progress(
    app: &AppHandle,
    stage: &str,
    current: usize,
    total: usize,
    message: &str,
    id: Uuid,
) -> bool {
    let is_first = current == 0;
    let is_last = current >= total;

    {
        let mut guard = PROGRESS_LAST_EMIT.lock().unwrap_or_else(|e| e.into_inner());
        let map = guard.get_or_insert_with(HashMap::new);
        let key = (stage.to_string(), id);

        if is_last {
            map.remove(&key);
        } else if !is_first {
            let now = Instant::now();
            match map.get(&key) {
                Some(last) if now.duration_since(*last) < PROGRESS_MIN_INTERVAL => {
                    return false;
                }
                _ => {
                    map.insert(key, now);
                }
            }
            if map.len() > 256 {
                map.retain(|_, last| last.elapsed() < Duration::from_secs(600));
            }
        } else {
            map.insert(key, Instant::now());
        }
    }

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
    true
}
