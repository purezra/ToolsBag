use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WebdavConfig {
    pub enabled: bool,
    pub server_url: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub auto_sync: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OnlineMeta {
    pub schema: String,
    pub vault_version: u64,
    pub last_updated_by: String,
    pub last_updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OnlineDeviceInfo {
    pub id: String,
    pub name: String,
    pub platform: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub trust_created_at: i64,
    #[serde(default)]
    pub trust_expire_at: i64,
    pub last_seen_at: i64,
    pub revoked: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct OnlineDevices {
    pub devices: Vec<OnlineDeviceInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OnlineEntry {
    pub entry_id: String,
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub icon_id: Option<String>,
    #[serde(default)]
    pub attachments: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted: bool,
    pub version: u64,
    pub last_updated_by: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OnlineSyncEntry {
    pub entry_id: String,
    pub ciphertext: String,
    pub version: u64,
    pub updated_at: i64,
    pub last_updated_by: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub entry_id: String,
    pub entry_title: String,
    pub local_updated_at: i64,
    pub remote_updated_at: i64,
    pub local_updated_by: String,
    pub remote_updated_by: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub entry_id: String,
    pub version: u64,
    pub updated_at: i64,
    pub last_updated_by: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChangeLogEntry {
    pub timestamp: i64,
    pub device_id: String,
    pub action: String,
    pub entry_id: String,
    pub entry_title: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WebdavSyncStatus {
    pub enabled: bool,
    pub connected: bool,
    pub last_sync_version: u64,
    pub remote_version: Option<u64>,
    pub last_updated_by: Option<String>,
    pub last_updated_at: Option<i64>,
    pub polling_active: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: Vec<SyncConflict>,
    pub new_version: u64,
}
