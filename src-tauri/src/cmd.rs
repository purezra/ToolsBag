use crate::detector;
use crate::converter;
use crate::models::{
  AnalyzeRequest, AnalyzeResult, CodebookConfig, Convert3Request, Convert3Result, ConvertRequest, ConvertResult,
  EpubConvertRequest, EpubConvertResult, FileEntry, MediaPreviewRequest, MediaPreviewResult, RepairRequest, RepairResult, SecureAccount, TraverseRequest,
  TraverseResult, VaultEntries, VaultStatus, DevicesPayload, VaultxExportRequest, VaultxExportResult,
  VaultxImportRequest, VaultxPreviewRequest, VaultxPreviewResult, WebdavConfig, WebdavSyncStatus, ChangeLogEntry, SyncResult,
};
use crate::repairer;
use crate::tools::{file_traverse, image_batch, mediainfo};
use crate::codebook;
use crate::webdav;
use std::path::PathBuf;
use tauri::AppHandle;
use walkdir::WalkDir;

#[tauri::command]
pub fn analyze_folder(app: AppHandle, req: AnalyzeRequest) -> Result<AnalyzeResult, String> {
  detector::analyze(app, req)
}

#[tauri::command]
pub fn repair_images(app: AppHandle, req: RepairRequest) -> Result<RepairResult, String> {
  repairer::repair(app, req)
}

#[tauri::command]
pub fn convert_to_pdf(app: AppHandle, req: ConvertRequest) -> Result<ConvertResult, String> {
  converter::convert(app, req)
}

#[tauri::command]
pub fn list_images(input_dir: PathBuf, recursive: bool) -> Result<Vec<FileEntry>, String> {
  let mut files = Vec::new();
  for entry in WalkDir::new(&input_dir)
    .follow_links(false)
    .into_iter()
    .filter_map(|e| e.ok())
  {
    if entry.file_type().is_file() {
      let path = entry.path().to_path_buf();
      if let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()) {
        if ["jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp"].contains(&ext.as_str()) {
          let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
          files.push(FileEntry {
            name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            path,
            format: ext,
            size: meta.len(),
          });
        }
      }
    }
    if !recursive && entry.depth() > 0 {
      continue;
    }
  }
  Ok(files)
}

#[tauri::command]
pub fn list_images_tool3(input_dir: PathBuf, recursive: bool) -> Result<Vec<FileEntry>, String> {
  image_batch::list_images(input_dir, recursive)
}

#[tauri::command]
pub fn convert_tool3(app: AppHandle, req: Convert3Request) -> Result<Convert3Result, String> {
  image_batch::convert(app, req)
}

#[tauri::command]
pub fn convert_to_epub(app: AppHandle, req: EpubConvertRequest) -> Result<EpubConvertResult, String> {
  image_batch::convert_to_epub(app, req)
}

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

#[tauri::command]
pub fn codebook_status(app: AppHandle) -> Result<VaultStatus, String> {
  codebook::vault_status(app)
}

#[tauri::command]
pub fn codebook_init(app: AppHandle, password: String, device_name: Option<String>) -> Result<VaultStatus, String> {
  codebook::initialize_vault(app, password, device_name)
}

#[tauri::command]
pub fn codebook_unlock(app: AppHandle, password: String) -> Result<VaultStatus, String> {
  codebook::unlock_vault(app, password)
}

#[tauri::command]
pub fn codebook_lock() -> Result<(), String> {
  codebook::lock_vault()
}

#[tauri::command]
pub fn codebook_list(app: AppHandle) -> Result<VaultEntries, String> {
  codebook::list_entries(app)
}

#[tauri::command]
pub fn codebook_list_devices(app: AppHandle) -> Result<DevicesPayload, String> {
  codebook::list_devices(app)
}

#[tauri::command]
pub fn codebook_save(app: AppHandle, entry: SecureAccount) -> Result<SecureAccount, String> {
  codebook::save_entry(app, entry)
}

#[tauri::command]
pub fn codebook_delete(app: AppHandle, id: String) -> Result<VaultEntries, String> {
  codebook::delete_entry(app, id)
}

#[tauri::command]
pub fn codebook_restore(app: AppHandle, id: String) -> Result<VaultEntries, String> {
  codebook::restore_entry(app, id)
}

#[tauri::command]
pub fn codebook_save_config(app: AppHandle, config: CodebookConfig) -> Result<CodebookConfig, String> {
  codebook::save_config(app, config)
}

#[tauri::command]
pub fn codebook_change_password(app: AppHandle, new_password: String) -> Result<VaultStatus, String> {
  codebook::change_master_password(app, new_password)
}

#[tauri::command]
pub fn codebook_revoke_device(app: AppHandle, target_id: String) -> Result<DevicesPayload, String> {
  codebook::revoke_device(app, target_id)
}

#[tauri::command]
pub fn codebook_export_vaultx(app: AppHandle, req: VaultxExportRequest) -> Result<VaultxExportResult, String> {
  codebook::export_vaultx(app, req)
}

#[tauri::command]
pub fn codebook_import_vaultx(app: AppHandle, req: VaultxImportRequest) -> Result<VaultEntries, String> {
  codebook::import_vaultx(app, req)
}

#[tauri::command]
pub fn codebook_preview_vaultx(app: AppHandle, req: VaultxPreviewRequest) -> Result<VaultxPreviewResult, String> {
  codebook::preview_vaultx(app, req)
}

// WebDAV Sync Commands
#[tauri::command]
pub fn webdav_save_config(app: AppHandle, config: WebdavConfig) -> Result<(), String> {
  webdav::save_webdav_config(&app, &config)
}

#[tauri::command]
pub fn webdav_load_config(app: AppHandle) -> Result<Option<WebdavConfig>, String> {
  Ok(webdav::load_webdav_config(&app))
}

#[tauri::command]
pub fn webdav_test_connection(config: WebdavConfig) -> Result<(), String> {
  webdav::test_webdav_connection(&config)
}

#[tauri::command]
pub fn webdav_get_status(app: AppHandle) -> Result<WebdavSyncStatus, String> {
  webdav::get_sync_status(&app)
}

#[tauri::command]
pub fn webdav_start_sync(app: AppHandle) -> Result<SyncResult, String> {
  webdav::sync_now(&app)
}

#[tauri::command]
pub fn webdav_sync_entry(app: AppHandle, entry: SecureAccount) -> Result<(), String> {
  webdav::sync_entry_to_remote(&app, &entry)
}

#[tauri::command]
pub fn webdav_pull_changes(app: AppHandle) -> Result<Vec<crate::models::SyncConflict>, String> {
  webdav::pull_changes(&app)
}

#[tauri::command]
pub fn webdav_get_changelog(app: AppHandle) -> Result<Vec<ChangeLogEntry>, String> {
  Ok(webdav::get_changelog(&app))
}

#[tauri::command]
pub fn webdav_start_polling(app: AppHandle) -> Result<(), String> {
  webdav::start_polling(app);
  Ok(())
}

#[tauri::command]
pub fn webdav_stop_polling() -> Result<(), String> {
  webdav::stop_polling();
  Ok(())
}

#[tauri::command]
pub fn webdav_stop_sync() -> Result<(), String> {
  webdav::stop_polling();
  Ok(())
}
