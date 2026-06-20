use crate::models::{
    DirectoryPreviewResult, MediaPreviewRequest, MediaPreviewResult, TraverseRequest,
    TraverseResult,
};
use crate::tools::{file_traverse, mediainfo};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

#[tauri::command]
pub async fn traverse_copy(app: AppHandle, req: TraverseRequest) -> Result<TraverseResult, String> {
    tauri::async_runtime::spawn_blocking(move || file_traverse::traverse_and_copy(app, req))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn preview_media(
    app: AppHandle,
    req: MediaPreviewRequest,
) -> Result<MediaPreviewResult, String> {
    tauri::async_runtime::spawn_blocking(move || file_traverse::preview_media(app, req))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn check_mediainfo() -> Result<bool, String> {
    Ok(mediainfo::init_mediainfo())
}

#[tauri::command]
pub fn path_exists(path: String) -> Result<bool, String> {
    Ok(Path::new(&path).exists())
}

#[tauri::command]
pub fn rename_path(source_path: String, target_path: String) -> Result<(), String> {
    let source = PathBuf::from(source_path);
    let target = PathBuf::from(target_path);
    if !source.exists() {
        return Err("源文件不存在".into());
    }
    if source.parent() != target.parent() {
        return Err("仅允许在同一目录内重命名".into());
    }
    std::fs::rename(source, target).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn preview_directory_files(input_dir: PathBuf) -> Result<DirectoryPreviewResult, String> {
    tauri::async_runtime::spawn_blocking(move || file_traverse::preview_directory_files(input_dir))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn write_text_export_file(path: String, contents: String) -> Result<(), String> {
    validate_export_extension(&path, &["csv", "md", "html", "json", "txt"])?;
    if let Some(parent) = Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(path, contents).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_binary_export_file(path: String, bytes: Vec<u8>) -> Result<(), String> {
    validate_export_extension(&path, &["xlsx", "pdf", "zip"])?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

fn validate_export_extension(path: &str, allowed: &[&str]) -> Result<(), String> {
    let ext = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .ok_or_else(|| "导出路径缺少文件扩展名".to_string())?;
    if allowed.contains(&ext.as_str()) {
        Ok(())
    } else {
        Err(format!("不允许导出为 .{} 文件", ext))
    }
}

#[cfg(test)]
mod tests {
    use super::validate_export_extension;

    #[test]
    fn export_extension_validator_accepts_allowed_extension() {
        assert!(validate_export_extension("report.csv", &["csv", "json"]).is_ok());
        assert!(validate_export_extension("REPORT.JSON", &["csv", "json"]).is_ok());
    }

    #[test]
    fn export_extension_validator_rejects_unexpected_extension() {
        assert!(validate_export_extension("report.exe", &["csv", "json"]).is_err());
        assert!(validate_export_extension("report", &["csv", "json"]).is_err());
    }
}
