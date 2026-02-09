use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeRequest {
  pub input_dir: PathBuf,
  pub recursive: bool,
  pub force_preprocess: bool,
  pub cache_max: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeResult {
  pub total_files: usize,
  pub problem_files: usize,
  pub compatible_files: usize,
  pub log_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImageIssue {
  pub path: PathBuf,
  pub reasons: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RepairRequest {
  pub input_dir: PathBuf,
  pub output_dir: Option<PathBuf>,
  pub only_problem: bool,
  pub cache_max: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RepairResult {
  pub repaired: usize,
  pub copied: usize,
  pub failed: usize,
  pub output_dir: PathBuf,
  pub log_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConvertRequest {
  pub input_dir: PathBuf,
  #[serde(default)]
  pub output_dir: Option<PathBuf>,
  #[serde(default)]
  pub batch_size: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConvertResult {
  pub batches: usize,
  pub output_path: PathBuf,
  pub log_path: PathBuf,
  pub problems: Vec<ImageIssue>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ProgressPayload {
  pub id: Uuid,
  pub stage: String,
  pub current: usize,
  pub total: usize,
  pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
  pub name: String,
  pub path: PathBuf,
  pub format: String,
  pub size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Convert3Request {
  pub input_dir: PathBuf,
  #[serde(default)]
  pub output_dir: Option<PathBuf>,
  #[serde(default)]
  pub batch_size: Option<usize>,
  #[serde(default = "default_lossless_merge")]
  pub lossless_merge: bool,
}

fn default_lossless_merge() -> bool {
  true
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Convert3Result {
  pub batches: usize,
  pub output_path: PathBuf,
  pub original_bytes: u64,
  pub output_bytes: u64,
  pub problems: Vec<ImageIssue>,
  pub log_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TraverseRequest {
  pub input_dir: PathBuf,
  #[serde(default)]
  pub output_dir: Option<PathBuf>,
  #[serde(default)]
  pub organize_by_format: bool,
  #[serde(default)]
  pub min_size_mb: Option<u64>,
  #[serde(default)]
  pub max_size_mb: Option<u64>,
  #[serde(default)]
  pub include_patterns: Vec<String>,
  #[serde(default)]
  pub exclude_patterns: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FormatStat {
  pub ext: String,
  pub scanned: usize,
  pub count: usize,
  pub filtered: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TraverseProblem {
  pub path: PathBuf,
  pub error: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TraverseResult {
  pub total_files: usize,
  pub copied: usize,
  pub skipped: usize,
  pub failed: usize,
  pub output_dir: PathBuf,
  pub elapsed_ms: u128,
  pub throughput_bytes: u64,
  pub format_stats: Vec<FormatStat>,
  pub problems: Vec<TraverseProblem>,
  pub log_path: PathBuf,
}

// Codebook secure vault
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
  pub output_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VaultxExportResult {
  pub file_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VaultxImportRequest {
  pub file_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VaultxPreviewRequest {
  pub file_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountDiff {
  pub name: String,
  pub account_identity: String,
  pub diff_type: String, // "added", "modified", "deleted"
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

// VaultX V2 规范结构

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

// WebDAV Online Sync Structures (PRD Section 2)
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
  pub status: String, // active / expired / revoked
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct SyncPullResult {
  pub updated_entries: Vec<SecureAccount>,
  pub conflicts: Vec<SyncConflict>,
  pub new_version: u64,
}
