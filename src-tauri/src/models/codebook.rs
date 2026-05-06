use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KdfParams {
  pub algorithm: String,
  pub salt: String,
  pub mem_cost: u32,
  pub time_cost: u32,
  pub parallelism: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IdentityPreset {
  pub category: String,
  pub value: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CodebookConfig {
  pub user_identities: Vec<String>,
  pub identity_presets: Vec<IdentityPreset>,
  pub service_tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SecureAccount {
  pub id: String,
  pub name: String,
  pub account_identity: String,
  pub password: String,
  pub tags: Vec<String>,
  pub notes: String,
  pub url: String,
  pub images: Vec<String>,
  pub created_at: String,
  pub updated_at: String,
  pub deleted: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VaultEntries {
  pub active: Vec<SecureAccount>,
  pub deleted: Vec<SecureAccount>,
  pub config: CodebookConfig,
  pub global_version: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
  pub initialized: bool,
  pub locked: bool,
  pub device_id: Option<String>,
  pub device_name: Option<String>,
  pub global_version: u64,
  pub kdf: Option<KdfParams>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
  pub id: String,
  pub name: String,
  pub public_key: String,
  pub added_at: String,
  pub revoked_at: Option<String>,
  pub revoked: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DevicesPayload {
  pub current: String,
  pub devices: Vec<DeviceSummary>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VaultxExportRequest {
  pub output_dir: Option<std::path::PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VaultxExportResult {
  pub file_path: std::path::PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VaultxImportRequest {
  pub file_path: std::path::PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VaultxPreviewRequest {
  pub file_path: std::path::PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountDiff {
  pub name: String,
  pub account_identity: String,
  pub diff_type: String,
  pub details: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VaultxPreviewResult {
  pub is_same: bool,
  pub local_count: usize,
  pub import_count: usize,
  pub diffs: Vec<AccountDiff>,
  pub config_changed: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VaultxV2Entry {
  pub id: String,
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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VaultxV2Dat {
  pub schema: String,
  pub created_at: i64,
  pub updated_at: i64,
  pub entries: Vec<VaultxV2Entry>,
  #[serde(default)]
  pub identity_presets: Vec<IdentityPreset>,
  #[serde(default)]
  pub service_tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VaultxV2Attachment {
  pub mime: String,
  pub filename: String,
  pub data: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VaultxV2Meta {
  pub schema: String,
  pub device_id: String,
  pub device_name: String,
  pub platform: String,
  pub vault_version: u64,
  pub last_sync_at: i64,
  pub attach_hash: String,
  pub dat_hash: String,
}
