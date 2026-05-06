use crate::codebook;
use crate::models::{
  CodebookConfig, DevicesPayload, SecureAccount, VaultEntries, VaultStatus,
  VaultxExportRequest, VaultxExportResult, VaultxImportRequest, VaultxPreviewRequest,
  VaultxPreviewResult,
};
use tauri::AppHandle;

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
