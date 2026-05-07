use crate::models::{
    ChangeLogEntry, SecureAccount, SyncConflict, SyncResult, WebdavConfig, WebdavSyncStatus,
};
use crate::webdav;
use tauri::AppHandle;

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
pub fn webdav_pull_changes(app: AppHandle) -> Result<Vec<SyncConflict>, String> {
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
