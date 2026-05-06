use crate::models::{MediaPreviewRequest, MediaPreviewResult, TraverseRequest, TraverseResult};
use crate::tools::{file_traverse, mediainfo};
use tauri::AppHandle;

#[tauri::command]
pub fn traverse_copy(app: AppHandle, req: TraverseRequest) -> Result<TraverseResult, String> {
  file_traverse::traverse_and_copy(app, req)
}

#[tauri::command]
pub fn preview_media(app: AppHandle, req: MediaPreviewRequest) -> Result<MediaPreviewResult, String> {
  file_traverse::preview_media(app, req)
}

#[tauri::command]
pub fn check_mediainfo() -> Result<bool, String> {
  Ok(mediainfo::init_mediainfo())
}
