use crate::codebook;
use crate::crypto::{decode_b64, encode_b64, encrypt_for_sync, decrypt_from_sync};
use crate::models::{
  SecureAccount, WebdavConfig, WebdavSyncStatus,
  OnlineEntry, OnlineMeta, OnlineDevices, SyncConflict, ChangeLogEntry,
  OnlineSyncEntry, ManifestEntry, SyncResult
};
use chrono::Utc;
use parking_lot::RwLock;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::{
  collections::HashMap,
  fs,
  path::PathBuf,
  sync::Arc,
  thread,
  time::Duration,
};
use tauri::{AppHandle, Manager};

const WEBDAV_VAULT_PATH: &str = "/webdav/vault";
const MANIFEST_PATH: &str = "/webdav/vault/entries/manifest.json";
const POLL_INTERVAL_SECS: u64 = 10;
const SYNC_SCHEMA: &str = "vaultx.online.meta.v1";
const ALLOWED_PATHS: [&str; 6] = [
  "/webdav/vault/entries/",
  "/webdav/vault/entries/manifest.json",
  "/webdav/vault/attachments/",
  "/webdav/vault/online.meta.json",
  "/webdav/vault/devices.json",
  "/webdav/vault",
];

#[derive(Clone)]
struct SyncState {
  config: Option<WebdavConfig>,
  master_key: Option<[u8; 32]>,
  device_id: Option<String>,
  device_name: Option<String>,
  last_sync_version: u64,
  polling_active: bool,
}

impl Default for SyncState {
  fn default() -> Self {
    Self {
      config: None,
      master_key: None,
      device_id: None,
      device_name: None,
      last_sync_version: 0,
      polling_active: false,
    }
  }
}

static SYNC_STATE: once_cell::sync::Lazy<Arc<RwLock<SyncState>>> =
  once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(SyncState::default())));

fn assert_vault_path(path: &str) -> Result<(), String> {
  if path == "/webdav" || path == "/webdav/" {
    return Ok(());
  }
  if path == WEBDAV_VAULT_PATH || path == format!("{}/", WEBDAV_VAULT_PATH) {
    return Ok(());
  }
  for allowed in ALLOWED_PATHS.iter() {
    if path.starts_with(allowed) {
      return Ok(());
    }
  }
  Err("VaultXSafetyFirewall: æ­¢æ­¢è®¿é—®éžæŽˆæƒç›®å½•".into())
}







fn now_ts() -> i64 {
  Utc::now().timestamp()
}

fn parse_ts_iso(s: &str) -> i64 {
  chrono::DateTime::parse_from_rfc3339(s)
    .map(|dt| dt.timestamp())
    .unwrap_or_else(|_| now_ts())
}

struct WebdavClient {
  client: Client,
  base_url: String,
  username: String,
  password: String,
}

impl WebdavClient {
  fn new(config: &WebdavConfig) -> Result<Self, String> {
    let client = Client::builder()
      .timeout(Duration::from_secs(30))
      .build()
      .map_err(|e| e.to_string())?;
    let base_url = config.server_url.trim_end_matches('/').to_string();
    Ok(Self {
      client,
      base_url,
      username: config.username.clone(),
      password: config.password.clone(),
    })
  }

  fn full_path(&self, path: &str) -> String {
    format!("{}{}", self.base_url, path)
  }

  fn get(&self, path: &str) -> Result<Vec<u8>, String> {
    assert_vault_path(path)?;
    let url = self.full_path(path);
    let resp = self.client
      .get(&url)
      .basic_auth(&self.username, Some(&self.password))
      .send()
      .map_err(|e| e.to_string())?;
    if resp.status() == StatusCode::NOT_FOUND {
      return Err("文件不存在".into());
    }
    if !resp.status().is_success() {
      return Err(format!("GET失败: {}", resp.status()));
    }
    resp.bytes().map(|b| b.to_vec()).map_err(|e| e.to_string())
  }

  fn put(&self, path: &str, data: Vec<u8>) -> Result<(), String> {
    assert_vault_path(path)?;
    let url = self.full_path(path);
    let resp = self.client
      .put(&url)
      .basic_auth(&self.username, Some(&self.password))
      .body(data)
      .send()
      .map_err(|e| e.to_string())?;
    if !resp.status().is_success() && resp.status() != StatusCode::CREATED {
      return Err(format!("PUT失败: {}", resp.status()));
    }
    Ok(())
  }

  #[allow(dead_code)]
  fn delete(&self, path: &str) -> Result<(), String> {
    assert_vault_path(path)?;
    let url = self.full_path(path);
    let resp = self.client
      .delete(&url)
      .basic_auth(&self.username, Some(&self.password))
      .send()
      .map_err(|e| e.to_string())?;
    if !resp.status().is_success() && resp.status() != StatusCode::NOT_FOUND {
      return Err(format!("DELETE失败: {}", resp.status()));
    }
    Ok(())
  }

  fn mkcol(&self, path: &str) -> Result<(), String> {
    assert_vault_path(path)?;
    let url = self.full_path(path);
    let resp = self.client
      .request(reqwest::Method::from_bytes(b"MKCOL").unwrap(), &url)
      .basic_auth(&self.username, Some(&self.password))
      .send()
      .map_err(|e| e.to_string())?;
    if resp.status() == StatusCode::METHOD_NOT_ALLOWED {
      return Ok(());
    }
    if !resp.status().is_success() && resp.status() != StatusCode::CREATED {
      return Err(format!("MKCOL失败: {}", resp.status()));
    }
    Ok(())
  }

  fn exists(&self, path: &str) -> bool {
    if assert_vault_path(path).is_err() {
      return false;
    }
    let url = self.full_path(path);
    self.client
      .head(&url)
      .basic_auth(&self.username, Some(&self.password))
      .send()
      .map(|r| r.status().is_success())
      .unwrap_or(false)
  }

  fn ensure_vault_dirs(&self) -> Result<(), String> {
    self.mkcol("/webdav")?;
    self.mkcol(WEBDAV_VAULT_PATH)?;
    self.mkcol(&format!("{}/entries", WEBDAV_VAULT_PATH))?;
    self.mkcol(&format!("{}/attachments", WEBDAV_VAULT_PATH))?;
    Ok(())
  }
}

fn validate_remote_structure(client: &WebdavClient) -> Result<(), String> {
  let required = vec![
    WEBDAV_VAULT_PATH.to_string(),
    format!("{}/entries", WEBDAV_VAULT_PATH),
    format!("{}/attachments", WEBDAV_VAULT_PATH),
  ];
  for path in required {
    if !client.exists(&path) {
      return Err(format!("WebDAVStructureValidator: ç¼ºå°‘å¿…è¦ç›®å½• {path}"));
    }
  }
  Ok(())
}

fn changelog_path(app: &AppHandle) -> PathBuf {
  let resolver = app.path_resolver();
  resolver
    .app_local_data_dir()
    .unwrap_or_else(|| resolver.app_data_dir().unwrap_or_else(|| PathBuf::from(".")))
    .join("sync_changelog.json")
}

fn load_changelog(app: &AppHandle) -> Vec<ChangeLogEntry> {
  let path = changelog_path(app);
  if path.exists() {
    fs::read_to_string(&path)
      .ok()
      .and_then(|s| serde_json::from_str(&s).ok())
      .unwrap_or_default()
  } else {
    Vec::new()
  }
}

fn append_changelog(app: &AppHandle, entry: ChangeLogEntry) {
  let path = changelog_path(app);
  let mut log = load_changelog(app);
  log.push(entry);
  if log.len() > 500 {
    let start = log.len().saturating_sub(500);
    log = log.split_off(start);
  }
  let _ = fs::write(&path, serde_json::to_string_pretty(&log).unwrap_or_default());
}

pub fn record_audit_log(app: &AppHandle, device_id: &str, action: &str, entry_id: &str, entry_title: &str) {
  append_changelog(app, ChangeLogEntry {
    timestamp: now_ts(),
    device_id: device_id.to_string(),
    action: action.to_string(),
    entry_id: entry_id.to_string(),
    entry_title: entry_title.to_string(),
  });
}

fn webdav_config_path(app: &AppHandle) -> PathBuf {
  let resolver = app.path_resolver();
  resolver
    .app_local_data_dir()
    .unwrap_or_else(|| resolver.app_data_dir().unwrap_or_else(|| PathBuf::from(".")))
    .join("webdav_config.json")
}

pub fn save_webdav_config(app: &AppHandle, config: &WebdavConfig) -> Result<(), String> {
  let path = webdav_config_path(app);
  let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
  fs::write(&path, json).map_err(|e| e.to_string())?;
  let mut state = SYNC_STATE.write();
  state.config = Some(config.clone());
  Ok(())
}

pub fn load_webdav_config(app: &AppHandle) -> Option<WebdavConfig> {
  let path = webdav_config_path(app);
  if path.exists() {
    fs::read_to_string(&path)
      .ok()
      .and_then(|s| serde_json::from_str(&s).ok())
  } else {
    None
  }
}

pub fn hydrate_sync_config(app: &AppHandle) {
  if let Some(cfg) = load_webdav_config(app) {
    let mut state = SYNC_STATE.write();
    state.config = Some(cfg);
  }
}

pub fn test_webdav_connection(config: &WebdavConfig) -> Result<(), String> {
  let client = WebdavClient::new(config)?;
  client.ensure_vault_dirs()?;
  validate_remote_structure(&client)?;
  Ok(())
}

pub fn init_sync_state(master_key: [u8; 32], device_id: String, device_name: String) {
  let mut state = SYNC_STATE.write();
  state.master_key = Some(master_key);
  state.device_id = Some(device_id);
  state.device_name = Some(device_name);
}

pub fn clear_sync_state() {
  let mut state = SYNC_STATE.write();
  state.master_key = None;
  state.device_id = None;
  state.device_name = None;
  state.polling_active = false;
}

fn require_sync_state() -> Result<(WebdavConfig, [u8; 32], String, String), String> {
  let state = SYNC_STATE.read();
  let config = state.config.clone().ok_or("WebDAV未配置")?;
  let mk = state.master_key.ok_or("未解锁")?;
  let device_id = state.device_id.clone().ok_or("设备ID缺失")?;
  let device_name = state.device_name.clone().ok_or("设备名缺失")?;
  Ok((config, mk, device_id, device_name))
}

fn online_meta_path() -> String {
  format!("{}/online.meta.json", WEBDAV_VAULT_PATH)
}

#[allow(dead_code)]
fn online_devices_path() -> String {
  format!("{}/devices.json", WEBDAV_VAULT_PATH)
}

fn entry_path(id: &str) -> String {
  format!("{}/entries/{}.json", WEBDAV_VAULT_PATH, id)
}

fn manifest_path() -> String {
  MANIFEST_PATH.to_string()
}

#[allow(dead_code)]
fn attachment_path(id: &str) -> String {
  format!("{}/attachments/{}.bin", WEBDAV_VAULT_PATH, id)
}

pub fn fetch_online_meta(_app: &AppHandle) -> Result<Option<OnlineMeta>, String> {
  let (config, mk, _, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  match client.get(&online_meta_path()) {
    Ok(encrypted) => {
      let decrypted = decrypt_from_sync(&mk, "online.meta", &encrypted)?;
      let meta: OnlineMeta = serde_json::from_slice(&decrypted).map_err(|e| e.to_string())?;
      Ok(Some(meta))
    }
    Err(e) if e.contains("不存在") => Ok(None),
    Err(e) => Err(e),
  }
}

pub fn push_online_meta(_app: &AppHandle, meta: &OnlineMeta) -> Result<(), String> {
  let (config, mk, _, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  client.ensure_vault_dirs()?;
  let json = serde_json::to_vec(meta).map_err(|e| e.to_string())?;
  let encrypted = encrypt_for_sync(&mk, "online.meta", &json)?;
  client.put(&online_meta_path(), encrypted)
}

#[allow(dead_code)]
pub fn fetch_online_devices(_app: &AppHandle) -> Result<Option<OnlineDevices>, String> {
  let (config, mk, _, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  match client.get(&online_devices_path()) {
    Ok(encrypted) => {
      let decrypted = decrypt_from_sync(&mk, "online.devices", &encrypted)?;
      let devices: OnlineDevices = serde_json::from_slice(&decrypted).map_err(|e| e.to_string())?;
      Ok(Some(devices))
    }
    Err(e) if e.contains("不存在") => Ok(None),
    Err(e) => Err(e),
  }
}

#[allow(dead_code)]
pub fn push_online_devices(_app: &AppHandle, devices: &OnlineDevices) -> Result<(), String> {
  let (config, mk, _, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  let json = serde_json::to_vec(devices).map_err(|e| e.to_string())?;
  let encrypted = encrypt_for_sync(&mk, "online.devices", &json)?;
  client.put(&online_devices_path(), encrypted)
}

pub fn fetch_manifest(_app: &AppHandle) -> Result<Vec<ManifestEntry>, String> {
  let (config, mk, _, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  match client.get(&manifest_path()) {
    Ok(raw) => {
      let decrypted = decrypt_from_sync(&mk, "entries.manifest", &raw)?;
      let entries: Vec<ManifestEntry> = serde_json::from_slice(&decrypted).map_err(|e| e.to_string())?;
      Ok(entries)
    }
    Err(e) if e.contains("不存在") => Ok(Vec::new()),
    Err(e) => Err(e),
  }
}

pub fn push_manifest(_app: &AppHandle, manifest: &[ManifestEntry]) -> Result<(), String> {
  let (config, mk, _, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  let json = serde_json::to_vec(manifest).map_err(|e| e.to_string())?;
  let encrypted = encrypt_for_sync(&mk, "entries.manifest", &json)?;
  client.put(&manifest_path(), encrypted)
}

pub fn fetch_entry(_app: &AppHandle, entry_id: &str) -> Result<Option<OnlineEntry>, String> {
  let (config, mk, _, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  match client.get(&entry_path(entry_id)) {
    Ok(raw) => {
      let sync_entry: OnlineSyncEntry = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
      let cipher_bytes = decode_b64(&sync_entry.ciphertext)?;
      let decrypted = decrypt_from_sync(&mk, &format!("entry:{}", entry_id), &cipher_bytes)?;
      let mut entry: OnlineEntry = serde_json::from_slice(&decrypted).map_err(|e| e.to_string())?;
      entry.version = sync_entry.version;
      entry.updated_at = sync_entry.updated_at;
      entry.last_updated_by = sync_entry.last_updated_by;
      Ok(Some(entry))
    }
    Err(e) if e.contains("不存在") => Ok(None),
    Err(e) => Err(e),
  }
}

pub fn push_entry(_app: &AppHandle, entry: &OnlineEntry) -> Result<(), String> {
  let (config, mk, _, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  let json = serde_json::to_vec(entry).map_err(|e| e.to_string())?;
  let encrypted = encrypt_for_sync(&mk, &format!("entry:{}", entry.entry_id), &json)?;
  let sync_entry = OnlineSyncEntry {
    entry_id: entry.entry_id.clone(),
    ciphertext: encode_b64(&encrypted),
    version: entry.version,
    updated_at: entry.updated_at,
    last_updated_by: entry.last_updated_by.clone(),
  };
  let payload = serde_json::to_vec(&sync_entry).map_err(|e| e.to_string())?;
  client.put(&entry_path(&entry.entry_id), payload)
}

#[allow(dead_code)]
pub fn delete_remote_entry(_app: &AppHandle, entry_id: &str) -> Result<(), String> {
  let (config, _, _, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  client.delete(&entry_path(entry_id))
}

#[allow(dead_code)]
pub fn push_attachment(_app: &AppHandle, attach_id: &str, data: &[u8]) -> Result<(), String> {
  let (config, mk, _, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  let encrypted = encrypt_for_sync(&mk, &format!("attach:{}", attach_id), data)?;
  client.put(&attachment_path(attach_id), encrypted)
}

#[allow(dead_code)]
pub fn fetch_attachment(_app: &AppHandle, attach_id: &str) -> Result<Option<Vec<u8>>, String> {
  let (config, mk, _, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  match client.get(&attachment_path(attach_id)) {
    Ok(encrypted) => {
      let decrypted = decrypt_from_sync(&mk, &format!("attach:{}", attach_id), &encrypted)?;
      Ok(Some(decrypted))
    }
    Err(e) if e.contains("不存在") => Ok(None),
    Err(e) => Err(e),
  }
}

pub fn account_to_online_entry(account: &SecureAccount, device_id: &str) -> OnlineEntry {
  let created_ts = chrono::DateTime::parse_from_rfc3339(&account.created_at)
    .map(|dt| dt.timestamp())
    .unwrap_or_else(|_| now_ts());
  let updated_ts = chrono::DateTime::parse_from_rfc3339(&account.updated_at)
    .map(|dt| dt.timestamp())
    .unwrap_or_else(|_| now_ts());
  OnlineEntry {
    entry_id: account.id.clone(),
    title: account.name.clone(),
    username: account.account_identity.clone(),
    password: account.password.clone(),
    url: account.url.clone(),
    notes: account.notes.clone(),
    tags: account.tags.clone(),
    icon_id: None,
    attachments: Vec::new(),
    created_at: created_ts,
    updated_at: updated_ts,
    deleted: account.deleted,
    version: 1,
    last_updated_by: device_id.to_string(),
  }
}

pub fn online_entry_to_account(entry: &OnlineEntry) -> SecureAccount {
  let created_at = chrono::DateTime::from_timestamp(entry.created_at, 0)
    .map(|dt| dt.to_rfc3339())
    .unwrap_or_default();
  let updated_at = chrono::DateTime::from_timestamp(entry.updated_at, 0)
    .map(|dt| dt.to_rfc3339())
    .unwrap_or_default();
  SecureAccount {
    id: entry.entry_id.clone(),
    name: entry.title.clone(),
    account_identity: entry.username.clone(),
    password: entry.password.clone(),
    url: entry.url.clone(),
    notes: entry.notes.clone(),
    tags: entry.tags.clone(),
    images: Vec::new(),
    created_at,
    updated_at,
    deleted: entry.deleted,
  }
}

pub fn sync_entry_to_remote(app: &AppHandle, account: &SecureAccount) -> Result<(), String> {
  let (config, _mk, device_id, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  client.ensure_vault_dirs()?;
  let mut online_entry = account_to_online_entry(account, &device_id);
  if let Ok(Some(existing)) = fetch_entry(app, &account.id) {
    online_entry.version = existing.version + 1;
  }
  push_entry(app, &online_entry)?;
  let meta = fetch_online_meta(app)?.unwrap_or(OnlineMeta {
    schema: SYNC_SCHEMA.to_string(),
    vault_version: 0,
    last_updated_by: device_id.clone(),
    last_updated_at: now_ts(),
  });
  let new_meta = OnlineMeta {
    schema: SYNC_SCHEMA.to_string(),
    vault_version: meta.vault_version + 1,
    last_updated_by: device_id.clone(),
    last_updated_at: now_ts(),
  };
  push_online_meta(app, &new_meta)?;
  let action = if account.deleted { "deleted" } else { "modified" };
  append_changelog(app, ChangeLogEntry {
    timestamp: now_ts(),
    device_id: device_id.clone(),
    action: action.to_string(),
    entry_id: account.id.clone(),
    entry_title: account.name.clone(),
  });
  Ok(())
}

#[allow(dead_code)]
pub fn resolve_conflict(local: &OnlineEntry, remote: &OnlineEntry) -> (OnlineEntry, bool) {
  if local.updated_at >= remote.updated_at {
    (local.clone(), false)
  } else {
    (remote.clone(), true)
  }
}

pub fn sync_now(app: &AppHandle) -> Result<SyncResult, String> {
  let (config, _mk, device_id, _) = require_sync_state()?;
  if !config.enabled {
    return Err("WebDAV æœªå¯ç”¨".into());
  }
  let client = WebdavClient::new(&config)?;
  client.ensure_vault_dirs()?;
  validate_remote_structure(&client)?;

  let remote_manifest = fetch_manifest(app)?;
  let mut manifest_map: HashMap<String, ManifestEntry> = remote_manifest
    .iter()
    .map(|m| (m.entry_id.clone(), m.clone()))
    .collect();

  let local_entries = codebook::list_entries(app.clone())?;
  let mut local_all = local_entries.active.clone();
  local_all.extend(local_entries.deleted.clone());
  let local_map: HashMap<String, SecureAccount> = local_all
    .iter()
    .map(|e| (e.id.clone(), e.clone()))
    .collect();

  let mut pushed = 0;
  let mut pulled = 0;
  let mut conflicts: Vec<SyncConflict> = Vec::new();

  // Push local -> remote when local is newer or remote missing
  for acc in local_all.iter() {
    let local_ts = parse_ts_iso(&acc.updated_at);
    if let Some(remote_meta) = manifest_map.get(&acc.id) {
      if remote_meta.updated_at > local_ts {
        conflicts.push(SyncConflict {
          entry_id: acc.id.clone(),
          entry_title: acc.name.clone(),
          local_updated_at: local_ts,
          remote_updated_at: remote_meta.updated_at,
          local_updated_by: device_id.clone(),
          remote_updated_by: remote_meta.last_updated_by.clone(),
        });
        continue;
      }
      let mut online_entry = account_to_online_entry(acc, &device_id);
      online_entry.version = remote_meta.version + 1;
      push_entry(app, &online_entry)?;
      manifest_map.insert(
        acc.id.clone(),
        ManifestEntry {
          entry_id: acc.id.clone(),
          version: online_entry.version,
          updated_at: online_entry.updated_at,
          last_updated_by: device_id.clone(),
        },
      );
      pushed += 1;
    } else {
      let mut online_entry = account_to_online_entry(acc, &device_id);
      online_entry.version = 1;
      push_entry(app, &online_entry)?;
      manifest_map.insert(
        acc.id.clone(),
        ManifestEntry {
          entry_id: acc.id.clone(),
          version: online_entry.version,
          updated_at: online_entry.updated_at,
          last_updated_by: device_id.clone(),
        },
      );
      pushed += 1;
    }
  }

  // Pull remote -> local when remote is newer or local missing
  for remote_meta in remote_manifest.iter() {
    let remote_entry = match fetch_entry(app, &remote_meta.entry_id)? {
      Some(e) => e,
      None => continue,
    };
    if let Some(local_acc) = local_map.get(&remote_meta.entry_id) {
      let local_ts = parse_ts_iso(&local_acc.updated_at);
      if remote_meta.updated_at > local_ts {
        // remote newer -> accept remote
        let account = online_entry_to_account(&remote_entry);
        let _ = codebook::save_entry(app.clone(), account)?;
        manifest_map.insert(
          remote_meta.entry_id.clone(),
          ManifestEntry {
            entry_id: remote_meta.entry_id.clone(),
            version: remote_entry.version,
            updated_at: remote_entry.updated_at,
            last_updated_by: remote_entry.last_updated_by.clone(),
          },
        );
        pulled += 1;
      } else if remote_meta.updated_at < local_ts {
        // local newer -> conflict
        conflicts.push(SyncConflict {
          entry_id: remote_meta.entry_id.clone(),
          entry_title: local_acc.name.clone(),
          local_updated_at: local_ts,
          remote_updated_at: remote_meta.updated_at,
          local_updated_by: device_id.clone(),
          remote_updated_by: remote_meta.last_updated_by.clone(),
        });
      }
    } else {
      // local missing -> import remote
      let account = online_entry_to_account(&remote_entry);
      let _ = codebook::save_entry(app.clone(), account)?;
      manifest_map.insert(
        remote_meta.entry_id.clone(),
        ManifestEntry {
          entry_id: remote_meta.entry_id.clone(),
          version: remote_entry.version,
          updated_at: remote_entry.updated_at,
          last_updated_by: remote_entry.last_updated_by.clone(),
        },
      );
      pulled += 1;
    }
  }

  let manifest_vec: Vec<ManifestEntry> = manifest_map.values().cloned().collect();
  push_manifest(app, &manifest_vec)?;

  // refresh meta + sync state
  if let Ok(Some(meta)) = fetch_online_meta(app) {
    let mut state = SYNC_STATE.write();
    state.last_sync_version = meta.vault_version;
  }

  let last_version = SYNC_STATE.read().last_sync_version;
  Ok(SyncResult {
    pushed,
    pulled,
    conflicts,
    new_version: last_version,
  })
}

pub fn pull_changes(app: &AppHandle) -> Result<Vec<SyncConflict>, String> {
  let (config, _mk, _device_id, _) = require_sync_state()?;
  let client = WebdavClient::new(&config)?;
  validate_remote_structure(&client)?;
  let remote_meta = match fetch_online_meta(app)? {
    Some(m) => m,
    None => return Ok(Vec::new()),
  };
  let local_version = {
    let state = SYNC_STATE.read();
    state.last_sync_version
  };
  if remote_meta.vault_version < local_version {
    return Err("WebDAVRollbackGuard: è¿œç«¯ç‰ˆæœ¬å°äºŽæœ¬åœ°æœ€è¿‘ç‰ˆæœ¬ï¼Œæ£€æµ‹åˆ°å›žæ»šæ“ä½œ".into());
  }
  if remote_meta.vault_version <= local_version {
    return Ok(Vec::new());
  }
  let conflicts = Vec::new();
  {
    let mut state = SYNC_STATE.write();
    state.last_sync_version = remote_meta.vault_version;
  }
  Ok(conflicts)
}

pub fn get_sync_status(app: &AppHandle) -> Result<WebdavSyncStatus, String> {
  let state = SYNC_STATE.read();
  let config = state.config.clone();
  let enabled = config.as_ref().map(|c| c.enabled).unwrap_or(false);
  let connected = if enabled {
    config.as_ref().map(|c| {
      WebdavClient::new(c).map(|client| client.exists(WEBDAV_VAULT_PATH)).unwrap_or(false)
    }).unwrap_or(false)
  } else {
    false
  };
  let remote_meta = if enabled && connected {
    drop(state);
    fetch_online_meta(app).ok().flatten()
  } else {
    None
  };
  let last_sync_version = SYNC_STATE.read().last_sync_version;
  Ok(WebdavSyncStatus {
    enabled,
    connected,
    last_sync_version,
    remote_version: remote_meta.as_ref().map(|m| m.vault_version),
    last_updated_by: remote_meta.as_ref().map(|m| m.last_updated_by.clone()),
    last_updated_at: remote_meta.as_ref().map(|m| m.last_updated_at),
    polling_active: SYNC_STATE.read().polling_active,
  })
}

pub fn get_changelog(app: &AppHandle) -> Vec<ChangeLogEntry> {
  load_changelog(app)
}

pub fn start_polling(app: AppHandle) {
  {
    let mut state = SYNC_STATE.write();
    if state.polling_active {
      return;
    }
    state.polling_active = true;
  }
  thread::spawn(move || {
    loop {
      {
        let state = SYNC_STATE.read();
        if !state.polling_active {
          break;
        }
        if state.config.is_none() || !state.config.as_ref().map(|c| c.enabled).unwrap_or(false) {
          break;
        }
      }
      thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));
      if let Ok(conflicts) = pull_changes(&app) {
        if !conflicts.is_empty() {
          let _ = app.emit_all("sync-conflicts", &conflicts);
        }
        let _ = app.emit_all("sync-updated", "");
      }
    }
    let mut state = SYNC_STATE.write();
    state.polling_active = false;
  });
}

pub fn stop_polling() {
  let mut state = SYNC_STATE.write();
  state.polling_active = false;
}
