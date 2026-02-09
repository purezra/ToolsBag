use crate::crypto::{
  encode_b64, derive_master_key, derive_file_keys,
  encrypt_with_key, decrypt_with_key, encrypt_with_device, decrypt_with_device,
  wrap_device_key, unwrap_device_key,
  EncryptedBlob, ENC_VERSION,
};
use rand::RngCore;
use crate::models::{
  AccountDiff, CodebookConfig, DeviceSummary, DevicesPayload, KdfParams, SecureAccount, VaultEntries, VaultStatus,
  VaultxExportRequest, VaultxExportResult, VaultxImportRequest, VaultxPreviewRequest, VaultxPreviewResult,
  VaultxV2Attachment, VaultxV2Dat, VaultxV2Entry, VaultxV2Meta,
};
use crate::utils::ensure_dir;
use crate::webdav;
use chrono::Utc;
use ed25519_dalek::Keypair;
use hmac::{digest::KeyInit, Hmac, Mac};
use rand::rngs::OsRng as RandOsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
  collections::HashSet,
  fs::{self, File},
  io::{Read, Write},
  path::{Path, PathBuf},
  sync::Mutex,
};
use tauri::{AppHandle, Manager};
use uuid::Uuid;
use zeroize::Zeroize;

type HmacSha256 = Hmac<Sha256>;

const SYNC_HINT: &str = "【密码本同步提醒】\n你的一台已授权设备发生了数据变更";

#[derive(Default)]
struct VaultState {
  master_key: Option<[u8; 32]>,
  device_key: Option<[u8; 32]>,
  device_id: Option<String>,
  device_name: Option<String>,
  global_version: u64,
}

static STATE: once_cell::sync::Lazy<Mutex<VaultState>> = once_cell::sync::Lazy::new(|| Mutex::new(VaultState::default()));

#[derive(Serialize, Deserialize, Clone)]
struct DeviceMeta {
  id: String,
  name: String,
  public_key: String,
  wrapped_device_key: EncryptedBlob,
}

#[derive(Serialize, Deserialize, Clone)]
struct MetaFile {
  version: u32,
  global_version: u64,
  min_cloud_version: u64,
  created_at: String,
  updated_at: String,
  kdf: KdfParams,
  device: DeviceMeta,
}

#[derive(Serialize, Deserialize, Clone)]
struct DeviceRecord {
  id: String,
  name: String,
  public_key: String,
  added_at: String,
  revoked_at: Option<String>,
  revoked: bool,
  #[serde(default)]
  private_key: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct DevicesFile {
  current: String,
  devices: Vec<DeviceRecord>,
}

#[derive(Serialize, Deserialize, Clone)]
struct EntryIndex {
  id: String,
  deleted: bool,
  updated_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct IndexFile {
  entries: Vec<EntryIndex>,
  config: CodebookConfig,
}

fn now_iso() -> String {
  Utc::now().to_rfc3339()
}

fn vault_root(app: &AppHandle) -> PathBuf {
  let resolver = app.path_resolver();
  resolver
    .app_local_data_dir()
    .unwrap_or_else(|| resolver.app_data_dir().unwrap_or_else(|| PathBuf::from("vault")))
    .join("vault")
}

fn entries_dir(root: &Path) -> PathBuf {
  root.join("entries")
}

fn images_dir(root: &Path) -> PathBuf {
  root.join("images")
}

fn recycle_dir(root: &Path) -> PathBuf {
  root.join("recycle_bin")
}

fn meta_path(root: &Path) -> PathBuf {
  root.join("meta.json")
}

fn index_path(root: &Path) -> PathBuf {
  root.join("index.json.enc")
}

fn devices_path(root: &Path) -> PathBuf {
  root.join("devices.json.enc")
}

fn version_guard_path(root: &Path) -> PathBuf {
  root.join("version.lock")
}

fn load_meta(root: &Path) -> Result<MetaFile, String> {
  let data = fs::read_to_string(meta_path(root)).map_err(|e| e.to_string())?;
  serde_json::from_str(&data).map_err(|e| e.to_string())
}

fn persist_meta(root: &Path, meta: &MetaFile) -> Result<(), String> {
  let content = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
  fs::write(meta_path(root), content).map_err(|e| e.to_string())
}

fn load_guard(root: &Path) -> u64 {
  if let Ok(data) = fs::read_to_string(version_guard_path(root)) {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
      return json["last_seen_version"].as_u64().unwrap_or(0);
    }
  }
  0
}

fn persist_guard(root: &Path, version: u64) {
  let _ = fs::write(
    version_guard_path(root),
    serde_json::json!({ "last_seen_version": version }).to_string(),
  );
}

fn set_state(master_key: [u8; 32], device_key: [u8; 32], meta: &MetaFile) {
  let mut guard = STATE.lock().expect("state poisoned");
  guard.master_key = Some(master_key);
  guard.device_key = Some(device_key);
  guard.device_id = Some(meta.device.id.clone());
  guard.device_name = Some(meta.device.name.clone());
  guard.global_version = meta.global_version;
}

fn update_state_version(version: u64) {
  if let Ok(mut guard) = STATE.lock() {
    guard.global_version = version;
  }
}

fn clear_state() {
  let mut guard = STATE.lock().expect("state poisoned");
  if let Some(mut mk) = guard.master_key.take() {
    mk.zeroize();
  }
  if let Some(mut dk) = guard.device_key.take() {
    dk.zeroize();
  }
  guard.device_id = None;
  guard.device_name = None;
  guard.global_version = 0;
}

fn require_state() -> Result<([u8; 32], [u8; 32], String), String> {
  let guard = STATE.lock().map_err(|_| "state lock error".to_string())?;
  let dk = guard
    .device_key
    .ok_or_else(|| "当前为锁定状态".to_string())?;
  let mk = guard
    .master_key
    .ok_or_else(|| "主密钥不存在".to_string())?;
  let device_id = guard
    .device_id
    .clone()
    .ok_or_else(|| "设备信息缺失".to_string())?;
  Ok((dk, mk, device_id))
}

fn load_devices(root: &Path, dk: &[u8]) -> Result<DevicesFile, String> {
  let blob = read_encrypted_blob(&devices_path(root))?;
  let bytes = decrypt_with_device(dk, "devices", &blob)?;
  serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn persist_devices(root: &Path, dk: &[u8], devices: &DevicesFile) -> Result<(), String> {
  let payload = serde_json::to_vec(devices).map_err(|e| e.to_string())?;
  let blob = encrypt_with_device(dk, "devices", &payload)?;
  write_encrypted_blob(&devices_path(root), &blob)
}

fn devices_to_payload(file: &DevicesFile) -> DevicesPayload {
  DevicesPayload {
    current: file.current.clone(),
    devices: file
      .devices
      .iter()
      .map(|d| DeviceSummary {
        id: d.id.clone(),
        name: d.name.clone(),
        public_key: d.public_key.clone(),
        added_at: d.added_at.clone(),
        revoked_at: d.revoked_at.clone(),
        revoked: d.revoked,
      })
      .collect(),
  }
}

fn load_index(root: &Path, dk: &[u8]) -> Result<IndexFile, String> {
  let blob = read_encrypted_blob(&index_path(root))?;
  let bytes = decrypt_with_device(dk, "index", &blob)?;
  serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn persist_index(root: &Path, dk: &[u8], index: &IndexFile) -> Result<(), String> {
  let payload = serde_json::to_vec(index).map_err(|e| e.to_string())?;
  let blob = encrypt_with_device(dk, "index", &payload)?;
  write_encrypted_blob(&index_path(root), &blob)
}

fn read_encrypted_blob(path: &Path) -> Result<EncryptedBlob, String> {
  let data = fs::read_to_string(path).map_err(|e| e.to_string())?;
  serde_json::from_str(&data).map_err(|e| e.to_string())
}

fn write_encrypted_blob(path: &Path, blob: &EncryptedBlob) -> Result<(), String> {
  let content = serde_json::to_string_pretty(blob).map_err(|e| e.to_string())?;
  fs::write(path, content).map_err(|e| e.to_string())
}

fn derive_entry_context(id: &str) -> String {
  format!("entry:{id}")
}

fn ensure_dirs(root: &Path) -> Result<(), String> {
  ensure_dir(&root.to_path_buf()).map_err(|e| e.to_string())?;
  ensure_dir(&entries_dir(root)).map_err(|e| e.to_string())?;
  ensure_dir(&images_dir(root)).map_err(|e| e.to_string())?;
  ensure_dir(&recycle_dir(root)).map_err(|e| e.to_string())
}

pub fn vault_status(app: AppHandle) -> Result<VaultStatus, String> {
  let root = vault_root(&app);
  let meta_exists = meta_path(&root).exists();
  let guard = STATE.lock().map_err(|_| "state lock error".to_string())?;
  let locked = guard.device_key.is_none();
  drop(guard);
  if !meta_exists {
    return Ok(VaultStatus {
      initialized: false,
      locked: true,
      device_id: None,
      device_name: None,
      global_version: 0,
      kdf: None,
    });
  }
  let meta = load_meta(&root)?;
  Ok(VaultStatus {
    initialized: true,
    locked,
    device_id: Some(meta.device.id),
    device_name: Some(meta.device.name),
    global_version: meta.global_version,
    kdf: Some(meta.kdf),
  })
}

pub fn initialize_vault(app: AppHandle, password: String, device_name: Option<String>) -> Result<VaultStatus, String> {
  let root = vault_root(&app);
  if meta_path(&root).exists() {
    return Err("已存在保险库，请先解锁".into());
  }
  ensure_dirs(&root)?;

  // derive master key
  let mut salt = [0u8; 16];
  RandOsRng.fill_bytes(&mut salt);
  let kdf = KdfParams {
    algorithm: "argon2id".into(),
    salt: encode_b64(&salt),
    mem_cost: 64 * 1024,
    time_cost: 3,
    parallelism: 2,
  };
  let master_key = derive_master_key(&password, &kdf)?;

  // generate device key + identity
  let mut dk = [0u8; 32];
  RandOsRng.fill_bytes(&mut dk);
  let keypair = Keypair::generate(&mut RandOsRng);
  let device_id = Uuid::new_v4().to_string();
  let device_name = device_name.unwrap_or_else(|| "PC".into());

  let wrapped = wrap_device_key(&master_key, &dk)?;
  let meta = MetaFile {
    version: ENC_VERSION,
    global_version: 1,
    min_cloud_version: 1,
    created_at: now_iso(),
    updated_at: now_iso(),
    kdf: kdf.clone(),
    device: DeviceMeta {
      id: device_id.clone(),
      name: device_name.clone(),
      public_key: encode_b64(&keypair.public.to_bytes()),
      wrapped_device_key: wrapped,
    },
  };

  let devices = DevicesFile {
    current: device_id.clone(),
    devices: vec![DeviceRecord {
      id: device_id.clone(),
      name: device_name.clone(),
      public_key: encode_b64(&keypair.public.to_bytes()),
      added_at: now_iso(),
      revoked_at: None,
      revoked: false,
      private_key: Some(encode_b64(&keypair.secret.to_bytes())),
    }],
  };

  let index = IndexFile {
    entries: Vec::new(),
    config: CodebookConfig {
      user_identities: Vec::new(),
      identity_presets: Vec::new(),
      service_tags: vec!["机场后台".into(), "邮箱类APP".into(), "手机APP".into(), "PC APP".into()],
    },
  };

  persist_meta(&root, &meta)?;
  persist_devices(&root, &dk, &devices)?;
  persist_index(&root, &dk, &index)?;
  persist_guard(&root, meta.global_version);
  set_state(master_key, dk, &meta);
  webdav::hydrate_sync_config(&app);
  webdav::init_sync_state(master_key, device_id.clone(), device_name.clone());

  Ok(VaultStatus {
    initialized: true,
    locked: false,
    device_id: Some(device_id),
    device_name: Some(device_name),
    global_version: meta.global_version,
    kdf: Some(kdf),
  })
}

pub fn unlock_vault(app: AppHandle, password: String) -> Result<VaultStatus, String> {
  let root = vault_root(&app);
  if !meta_path(&root).exists() {
    return Err("请先初始化保险库".into());
  }
  let meta = load_meta(&root)?;
  let guard_version = load_guard(&root);
  if meta.global_version < guard_version {
    return Err("检测到版本回退，拒绝加载".into());
  }
  let master_key = derive_master_key(&password, &meta.kdf)?;
  let dk = unwrap_device_key(&master_key, &meta.device.wrapped_device_key)?;

  // verify device status
  let devices = load_devices(&root, &dk)?;
  let current = devices
    .devices
    .iter()
    .find(|d| d.id == meta.device.id)
    .ok_or_else(|| "设备列表缺失当前设备".to_string())?;
  if current.revoked {
    return Err("当前设备已被吊销，无法解密新数据".into());
  }

  persist_guard(&root, meta.global_version);
  set_state(master_key, dk, &meta);
  webdav::hydrate_sync_config(&app);
  webdav::init_sync_state(master_key, meta.device.id.clone(), meta.device.name.clone());
  Ok(VaultStatus {
    initialized: true,
    locked: false,
    device_id: Some(meta.device.id),
    device_name: Some(meta.device.name),
    global_version: meta.global_version,
    kdf: Some(meta.kdf),
  })
}

pub fn lock_vault() -> Result<(), String> {
  clear_state();
  webdav::clear_sync_state();
  Ok(())
}

pub fn list_devices(app: AppHandle) -> Result<DevicesPayload, String> {
  let root = vault_root(&app);
  let (dk, _, _) = require_state()?;
  let devices = load_devices(&root, &dk)?;
  Ok(devices_to_payload(&devices))
}

pub fn list_entries(app: AppHandle) -> Result<VaultEntries, String> {
  let root = vault_root(&app);
  let (dk, _, _) = require_state()?;
  let index = load_index(&root, &dk)?;
  let mut active = Vec::new();
  let mut deleted = Vec::new();

  for entry in index.entries.iter() {
    let path = entries_dir(&root).join(format!("{}.json.enc", entry.id));
    if !path.exists() {
      continue;
    }
    let blob = read_encrypted_blob(&path)?;
    let bytes = decrypt_with_device(&dk, &derive_entry_context(&entry.id), &blob)?;
    let acc: SecureAccount = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if acc.deleted {
      deleted.push(acc);
    } else {
      active.push(acc);
    }
  }

  Ok(VaultEntries {
    active,
    deleted,
    config: index.config,
    global_version: STATE
      .lock()
      .ok()
      .map(|s| s.global_version)
      .unwrap_or(0),
  })
}

fn write_entry(root: &Path, dk: &[u8], account: &SecureAccount) -> Result<(), String> {
  let payload = serde_json::to_vec(account).map_err(|e| e.to_string())?;
  let blob = encrypt_with_device(dk, &derive_entry_context(&account.id), &payload)?;
  let path = entries_dir(root).join(format!("{}.json.enc", account.id));
  write_encrypted_blob(&path, &blob)?;
  if account.deleted {
    let recycle_path = recycle_dir(root).join(format!("{}.json.enc", account.id));
    write_encrypted_blob(&recycle_path, &blob)?;
  }
  Ok(())
}

fn update_index_with_entry(index: &mut IndexFile, account: &SecureAccount) {
  if let Some(existing) = index.entries.iter_mut().find(|e| e.id == account.id) {
    existing.deleted = account.deleted;
    existing.updated_at = account.updated_at.clone();
  } else {
    index.entries.push(EntryIndex {
      id: account.id.clone(),
      deleted: account.deleted,
      updated_at: account.updated_at.clone(),
    });
  }
}

fn bump_version(meta: &mut MetaFile) {
  meta.global_version = meta.global_version.saturating_add(1);
  meta.updated_at = now_iso();
  if meta.global_version > meta.min_cloud_version {
    meta.min_cloud_version = meta.global_version;
  }
}

fn sanitize_tags(tags: &[String]) -> Vec<String> {
  let mut uniq = HashSet::new();
  tags
    .iter()
    .filter_map(|t| {
      let v = t.trim();
      if v.is_empty() {
        None
      } else {
        let lower = v.to_string();
        if uniq.contains(&lower) {
          None
        } else {
          uniq.insert(lower.clone());
          Some(v.to_string())
        }
      }
    })
    .collect()
}

fn normalize_account(mut account: SecureAccount) -> SecureAccount {
  account.tags = sanitize_tags(&account.tags);
  account
}

pub fn save_entry(app: AppHandle, mut account: SecureAccount) -> Result<SecureAccount, String> {
  let root = vault_root(&app);
  let (dk, _mk, _) = require_state()?;
  let mut meta = load_meta(&root)?;
  let mut index = load_index(&root, &dk)?;

  let now = now_iso();
  if account.id.is_empty() {
    account.id = Uuid::new_v4().to_string();
    account.created_at = now.clone();
  }
  account.updated_at = now;
  account = normalize_account(account);

  // ensure config contains identity/tag presets
  if !account.account_identity.is_empty()
    && !index.config.user_identities.contains(&account.account_identity)
  {
    index.config.user_identities.push(account.account_identity.clone());
  }
  for tag in account.tags.iter() {
    if !index.config.service_tags.contains(tag) {
      index.config.service_tags.push(tag.clone());
    }
  }

  write_entry(&root, &dk, &account)?;
  update_index_with_entry(&mut index, &account);

  bump_version(&mut meta);
  persist_index(&root, &dk, &index)?;
  persist_meta(&root, &meta)?;
  persist_guard(&root, meta.global_version);
  update_state_version(meta.global_version);

  app.emit_all("codebook-sync", SYNC_HINT).ok();
  Ok(account)
}

pub fn delete_entry(app: AppHandle, id: String) -> Result<VaultEntries, String> {
  let root = vault_root(&app);
  let (dk, _mk, _) = require_state()?;
  let mut meta = load_meta(&root)?;
  let mut index = load_index(&root, &dk)?;

  let entry_idx = index.entries.iter_mut().find(|e| e.id == id).ok_or("未找到目标条目")?;
  entry_idx.deleted = true;
  entry_idx.updated_at = now_iso();

  let path = entries_dir(&root).join(format!("{}.json.enc", id));
  if path.exists() {
    let blob = read_encrypted_blob(&path)?;
    let mut acc: SecureAccount =
      serde_json::from_slice(&decrypt_with_device(&dk, &derive_entry_context(&id), &blob)?)
        .map_err(|e| e.to_string())?;
    acc.deleted = true;
    acc.updated_at = entry_idx.updated_at.clone();
    write_entry(&root, &dk, &acc)?;
  }

  bump_version(&mut meta);
  persist_index(&root, &dk, &index)?;
  persist_meta(&root, &meta)?;
  persist_guard(&root, meta.global_version);
  update_state_version(meta.global_version);
  app.emit_all("codebook-sync", SYNC_HINT).ok();
  list_entries(app)
}

pub fn restore_entry(app: AppHandle, id: String) -> Result<VaultEntries, String> {
  let root = vault_root(&app);
  let (dk, _mk, _) = require_state()?;
  let mut meta = load_meta(&root)?;
  let mut index = load_index(&root, &dk)?;

  let entry_idx = index.entries.iter_mut().find(|e| e.id == id).ok_or("未找到目标条目")?;
  entry_idx.deleted = false;
  entry_idx.updated_at = now_iso();

  let path = entries_dir(&root).join(format!("{}.json.enc", id));
  if path.exists() {
    let blob = read_encrypted_blob(&path)?;
    let mut acc: SecureAccount =
      serde_json::from_slice(&decrypt_with_device(&dk, &derive_entry_context(&id), &blob)?)
        .map_err(|e| e.to_string())?;
    acc.deleted = false;
    acc.updated_at = entry_idx.updated_at.clone();
    write_entry(&root, &dk, &acc)?;
  }

  bump_version(&mut meta);
  persist_index(&root, &dk, &index)?;
  persist_meta(&root, &meta)?;
  persist_guard(&root, meta.global_version);
  update_state_version(meta.global_version);
  app.emit_all("codebook-sync", SYNC_HINT).ok();
  list_entries(app)
}

pub fn save_config(app: AppHandle, config: CodebookConfig) -> Result<CodebookConfig, String> {
  let root = vault_root(&app);
  let (dk, _mk, _) = require_state()?;
  let mut meta = load_meta(&root)?;
  let mut index = load_index(&root, &dk)?;
  index.config = config.clone();
  bump_version(&mut meta);
  persist_index(&root, &dk, &index)?;
  persist_meta(&root, &meta)?;
  persist_guard(&root, meta.global_version);
  update_state_version(meta.global_version);
  Ok(config)
}

pub fn change_master_password(app: AppHandle, new_password: String) -> Result<VaultStatus, String> {
  let root = vault_root(&app);
  let (dk, _, _) = require_state()?;
  let mut salt = [0u8; 16];
  RandOsRng.fill_bytes(&mut salt);
  let kdf = KdfParams {
    algorithm: "argon2id".into(),
    salt: encode_b64(&salt),
    mem_cost: 64 * 1024,
    time_cost: 3,
    parallelism: 2,
  };
  let master_key = derive_master_key(&new_password, &kdf)?;

  let mut meta = load_meta(&root)?;
  meta.kdf = kdf.clone();
  meta.device.wrapped_device_key = wrap_device_key(&master_key, &dk)?;
  bump_version(&mut meta);
  persist_meta(&root, &meta)?;
  persist_guard(&root, meta.global_version);
  set_state(master_key, dk, &meta);
  Ok(VaultStatus {
    initialized: true,
    locked: false,
    device_id: Some(meta.device.id),
    device_name: Some(meta.device.name),
    global_version: meta.global_version,
    kdf: Some(kdf),
  })
}

pub fn revoke_device(app: AppHandle, target_id: String) -> Result<DevicesPayload, String> {
  let root = vault_root(&app);
  let (dk, _mk, current_id) = require_state()?;
  let mut devices = load_devices(&root, &dk)?;
  let is_current = {
    let target = devices
      .devices
      .iter_mut()
      .find(|d| d.id == target_id)
      .ok_or_else(|| "未找到目标设备".to_string())?;
    target.revoked = true;
    target.revoked_at = Some(now_iso());
    target.id == current_id
  };
  if is_current {
    clear_state();
  }
  persist_devices(&root, &dk, &devices)?;

  let mut meta = load_meta(&root)?;
  bump_version(&mut meta);
  persist_meta(&root, &meta)?;
  persist_guard(&root, meta.global_version);
  if !is_current {
    update_state_version(meta.global_version);
  }
  app.emit_all("codebook-sync", SYNC_HINT).ok();
  Ok(devices_to_payload(&devices))
}

fn is_current_device_revoked(root: &Path, dk: &[u8], device_id: &str) -> Result<bool, String> {
  let devices = load_devices(root, dk)?;
  Ok(devices
    .devices
    .iter()
    .find(|d| d.id == device_id)
    .map(|d| d.revoked)
    .unwrap_or(false))
}

// VaultX V2 导出 - 生成符合规范的ZIP文件
pub fn export_vaultx(app: AppHandle, req: VaultxExportRequest) -> Result<VaultxExportResult, String> {
  use zip::write::FileOptions;
  use zip::ZipWriter;
  use std::collections::HashMap;

  let root = vault_root(&app);
  let (dk, mk, device_id) = require_state()?;
  if is_current_device_revoked(&root, &dk, &device_id)? {
    return Err("设备已被吊销，禁止导出".into());
  }

  let entries = list_entries(app.clone())?;
  let meta = load_meta(&root)?;
  let index = load_index(&root, &dk)?;
  let now_ts = Utc::now().timestamp();

  // 构建V2 entries并收集附件
  let mut v2_entries = Vec::new();
  let mut attachments: HashMap<String, VaultxV2Attachment> = HashMap::new();

  let all_entries: Vec<_> = entries.active.iter().chain(entries.deleted.iter()).collect();
  for acc in all_entries {
    let mut attach_ids = Vec::new();
    // 处理图片附件
    for (i, img_data) in acc.images.iter().enumerate() {
      if img_data.starts_with("data:image") {
        let attach_id = format!("{}-attach-{}", acc.id, i);
        let parts: Vec<&str> = img_data.splitn(2, ',').collect();
        if parts.len() == 2 {
          let mime = if parts[0].contains("png") { "image/png" } else { "image/jpeg" };
          attachments.insert(attach_id.clone(), VaultxV2Attachment {
            mime: mime.to_string(),
            filename: format!("image_{}.{}", i, if mime == "image/png" { "png" } else { "jpg" }),
            data: parts[1].to_string(),
          });
          attach_ids.push(attach_id);
        }
      }
    }

    let created_ts = chrono::DateTime::parse_from_rfc3339(&acc.created_at)
      .map(|dt| dt.timestamp())
      .unwrap_or(now_ts);
    let updated_ts = chrono::DateTime::parse_from_rfc3339(&acc.updated_at)
      .map(|dt| dt.timestamp())
      .unwrap_or(now_ts);

    v2_entries.push(VaultxV2Entry {
      id: acc.id.clone(),
      title: acc.name.clone(),
      username: acc.account_identity.clone(),
      password: acc.password.clone(),
      url: acc.url.clone(),
      notes: acc.notes.clone(),
      tags: acc.tags.clone(),
      icon_id: None,
      attachments: attach_ids,
      created_at: created_ts,
      updated_at: updated_ts,
      deleted: acc.deleted,
    });
  }

  // 构建V2 dat结构
  let v2_dat = VaultxV2Dat {
    schema: "vaultx.v2".to_string(),
    created_at: now_ts,
    updated_at: now_ts,
    entries: v2_entries,
    identity_presets: index.config.identity_presets.clone(),
    service_tags: index.config.service_tags.clone(),
  };

  // 序列化并加密dat
  let dat_json = serde_json::to_vec(&v2_dat).map_err(|e| e.to_string())?;
  let (dat_enc, dat_mac) = derive_file_keys(&mk, "vaultx-dat")?;
  let dat_blob = encrypt_with_key(&dat_enc, &dat_mac, &dat_json)?;
  let dat_bytes = serde_json::to_vec(&dat_blob).map_err(|e| e.to_string())?;

  // 序列化并加密attach
  let attach_json = serde_json::to_vec(&attachments).map_err(|e| e.to_string())?;
  let (attach_enc, attach_mac) = derive_file_keys(&mk, "vaultx-attach")?;
  let attach_blob = encrypt_with_key(&attach_enc, &attach_mac, &attach_json)?;
  let attach_bytes = serde_json::to_vec(&attach_blob).map_err(|e| e.to_string())?;

  // 计算哈希
  let mut dat_hasher = Sha256::new();
  dat_hasher.update(&dat_bytes);
  let dat_hash = encode_b64(&dat_hasher.finalize());

  let mut attach_hasher = Sha256::new();
  attach_hasher.update(&attach_bytes);
  let attach_hash = encode_b64(&attach_hasher.finalize());

  // 构建meta (明文)
  let v2_meta = VaultxV2Meta {
    schema: "vaultx.meta.v2".to_string(),
    device_id: device_id.clone(),
    device_name: meta.device.name.clone(),
    platform: "windows".to_string(),
    vault_version: meta.global_version,
    last_sync_at: now_ts,
    attach_hash: attach_hash.clone(),
    dat_hash: dat_hash.clone(),
  };
  let meta_bytes = serde_json::to_vec_pretty(&v2_meta).map_err(|e| e.to_string())?;

  // 生成签名 (HMAC-SHA256 over dat + attach + meta)
  let (_, sig_mac) = derive_file_keys(&mk, "vaultx-sig")?;
  let mut sig_hmac = <HmacSha256 as KeyInit>::new_from_slice(&sig_mac).map_err(|e| e.to_string())?;
  sig_hmac.update(&dat_bytes);
  sig_hmac.update(&attach_bytes);
  sig_hmac.update(&meta_bytes);
  let sig_bytes = sig_hmac.finalize().into_bytes();

  // 创建ZIP文件
  let output_dir = req.output_dir.unwrap_or_else(|| PathBuf::from("."));
  ensure_dir(&output_dir).map_err(|e| e.to_string())?;
  let filename = format!("vaultx_backup_{}.zip", chrono::Local::now().format("%Y%m%d_%H%M%S"));
  let path = output_dir.join(&filename);

  let file = File::create(&path).map_err(|e| e.to_string())?;
  let mut zip = ZipWriter::new(file);
  let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

  zip.start_file("vaultx/vaultx.dat", options).map_err(|e| e.to_string())?;
  zip.write_all(&dat_bytes).map_err(|e| e.to_string())?;

  zip.start_file("vaultx/vaultx.attach", options).map_err(|e| e.to_string())?;
  zip.write_all(&attach_bytes).map_err(|e| e.to_string())?;

  zip.start_file("vaultx/vaultx.meta", options).map_err(|e| e.to_string())?;
  zip.write_all(&meta_bytes).map_err(|e| e.to_string())?;

  zip.start_file("vaultx/vaultx.sig", options).map_err(|e| e.to_string())?;
  zip.write_all(&sig_bytes).map_err(|e| e.to_string())?;

  zip.finish().map_err(|e| e.to_string())?;
  webdav::record_audit_log(
    &app,
    &device_id,
    "export",
    "vaultx",
    &path
      .file_name()
      .and_then(|n| n.to_str())
      .unwrap_or_else(|| filename.as_str())
  );
  Ok(VaultxExportResult { file_path: path })
}

// VaultX V2 导入 - 读取符合规范的ZIP文件
pub fn import_vaultx(app: AppHandle, req: VaultxImportRequest) -> Result<VaultEntries, String> {
  use zip::ZipArchive;
  use std::collections::HashMap;

  let root = vault_root(&app);
  let (dk, mk, device_id) = require_state()?;
  if is_current_device_revoked(&root, &dk, &device_id)? {
    return Err("设备已被吊销，禁止导入".into());
  }

  // 读取ZIP文件
  let file = File::open(&req.file_path).map_err(|e| e.to_string())?;
  let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

  // 读取各个文件内容
  let mut dat_bytes = Vec::new();
  let mut attach_bytes = Vec::new();
  let mut meta_bytes = Vec::new();
  let mut sig_bytes = Vec::new();

  for i in 0..archive.len() {
    let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
    let name = file.name().to_string();
    let mut content = Vec::new();
    file.read_to_end(&mut content).map_err(|e| e.to_string())?;

    if name.ends_with("vaultx.dat") {
      dat_bytes = content;
    } else if name.ends_with("vaultx.attach") {
      attach_bytes = content;
    } else if name.ends_with("vaultx.meta") {
      meta_bytes = content;
    } else if name.ends_with("vaultx.sig") {
      sig_bytes = content;
    }
  }

  if dat_bytes.is_empty() || meta_bytes.is_empty() {
    return Err("无效的VaultX文件：缺少必要组件".into());
  }

  // 1. 验证签名
  let (_, sig_mac) = derive_file_keys(&mk, "vaultx-sig")?;
  let mut sig_hmac = <HmacSha256 as KeyInit>::new_from_slice(&sig_mac).map_err(|e| e.to_string())?;
  sig_hmac.update(&dat_bytes);
  sig_hmac.update(&attach_bytes);
  sig_hmac.update(&meta_bytes);
  sig_hmac.verify_slice(&sig_bytes).map_err(|_| "签名验证失败，文件可能被篡改")?;

  // 2. 读取meta并验证哈希
  let v2_meta: VaultxV2Meta = serde_json::from_slice(&meta_bytes).map_err(|e| e.to_string())?;

  let mut dat_hasher = Sha256::new();
  dat_hasher.update(&dat_bytes);
  let computed_dat_hash = encode_b64(&dat_hasher.finalize());
  if computed_dat_hash != v2_meta.dat_hash {
    return Err("数据文件哈希校验失败".into());
  }

  if !attach_bytes.is_empty() {
    let mut attach_hasher = Sha256::new();
    attach_hasher.update(&attach_bytes);
    let computed_attach_hash = encode_b64(&attach_hasher.finalize());
    if computed_attach_hash != v2_meta.attach_hash {
      return Err("附件文件哈希校验失败".into());
    }
  }

  // 3. 解密dat
  let dat_blob: EncryptedBlob = serde_json::from_slice(&dat_bytes).map_err(|e| e.to_string())?;
  let (dat_enc, dat_mac) = derive_file_keys(&mk, "vaultx-dat")?;
  let dat_json = decrypt_with_key(&dat_enc, &dat_mac, &dat_blob)?;
  let v2_dat: VaultxV2Dat = serde_json::from_slice(&dat_json).map_err(|e| e.to_string())?;

  // 4. 解密attach
  let attachments: HashMap<String, VaultxV2Attachment> = if !attach_bytes.is_empty() {
    let attach_blob: EncryptedBlob = serde_json::from_slice(&attach_bytes).map_err(|e| e.to_string())?;
    let (attach_enc, attach_mac) = derive_file_keys(&mk, "vaultx-attach")?;
    let attach_json = decrypt_with_key(&attach_enc, &attach_mac, &attach_blob)?;
    serde_json::from_slice(&attach_json).map_err(|e| e.to_string())?
  } else {
    HashMap::new()
  };

  // 5. 转换为本地格式并保存
  for v2_entry in v2_dat.entries {
    // 重建图片数据
    let mut images = Vec::new();
    for attach_id in &v2_entry.attachments {
      if let Some(attach) = attachments.get(attach_id) {
        let data_url = format!("data:{};base64,{}", attach.mime, attach.data);
        images.push(data_url);
      }
    }

    let created_at = chrono::DateTime::from_timestamp(v2_entry.created_at, 0)
      .map(|dt| dt.to_rfc3339())
      .unwrap_or_else(|| now_iso());
    let updated_at = chrono::DateTime::from_timestamp(v2_entry.updated_at, 0)
      .map(|dt| dt.to_rfc3339())
      .unwrap_or_else(|| now_iso());

    let account = SecureAccount {
      id: v2_entry.id,
      name: v2_entry.title,
      account_identity: v2_entry.username,
      password: v2_entry.password,
      tags: v2_entry.tags,
      notes: v2_entry.notes,
      url: v2_entry.url,
      images,
      created_at,
      updated_at,
      deleted: v2_entry.deleted,
    };
    let _ = save_entry(app.clone(), account)?;
  }

  // 6. 保存配置
  let config = CodebookConfig {
    user_identities: Vec::new(),
    identity_presets: v2_dat.identity_presets,
    service_tags: v2_dat.service_tags,
  };
  let _ = save_config(app.clone(), config)?;

  app.emit_all("codebook-sync", SYNC_HINT).ok();
  webdav::record_audit_log(
    &app,
    &device_id,
    "import",
    "vaultx",
    &req
      .file_path
      .file_name()
      .and_then(|n| n.to_str())
      .map(|s| s.to_string())
      .unwrap_or_else(|| req.file_path.to_string_lossy().to_string())
  );
  list_entries(app)
}

// 读取VaultX V2 ZIP文件内容
fn read_vaultx_v2_dat(mk: &[u8], file_path: &Path) -> Result<(VaultxV2Dat, VaultxV2Meta), String> {
  use zip::ZipArchive;

  let file = File::open(file_path).map_err(|e| e.to_string())?;
  let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

  let mut dat_bytes = Vec::new();
  let mut attach_bytes = Vec::new();
  let mut meta_bytes = Vec::new();
  let mut sig_bytes = Vec::new();

  for i in 0..archive.len() {
    let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
    let name = file.name().to_string();
    let mut content = Vec::new();
    file.read_to_end(&mut content).map_err(|e| e.to_string())?;

    if name.ends_with("vaultx.dat") {
      dat_bytes = content;
    } else if name.ends_with("vaultx.attach") {
      attach_bytes = content;
    } else if name.ends_with("vaultx.meta") {
      meta_bytes = content;
    } else if name.ends_with("vaultx.sig") {
      sig_bytes = content;
    }
  }

  if dat_bytes.is_empty() || meta_bytes.is_empty() {
    return Err("无效的VaultX文件：缺少必要组件".into());
  }

  // 验证签名
  let (_, sig_mac) = derive_file_keys(mk, "vaultx-sig")?;
  let mut sig_hmac = <HmacSha256 as KeyInit>::new_from_slice(&sig_mac).map_err(|e| e.to_string())?;
  sig_hmac.update(&dat_bytes);
  sig_hmac.update(&attach_bytes);
  sig_hmac.update(&meta_bytes);
  sig_hmac.verify_slice(&sig_bytes).map_err(|_| "签名验证失败，文件可能被篡改")?;

  // 读取meta
  let v2_meta: VaultxV2Meta = serde_json::from_slice(&meta_bytes).map_err(|e| e.to_string())?;

  // 验证哈希
  let mut dat_hasher = Sha256::new();
  dat_hasher.update(&dat_bytes);
  let computed_dat_hash = encode_b64(&dat_hasher.finalize());
  if computed_dat_hash != v2_meta.dat_hash {
    return Err("数据文件哈希校验失败".into());
  }

  // 解密dat
  let dat_blob: EncryptedBlob = serde_json::from_slice(&dat_bytes).map_err(|e| e.to_string())?;
  let (dat_enc, dat_mac) = derive_file_keys(mk, "vaultx-dat")?;
  let dat_json = decrypt_with_key(&dat_enc, &dat_mac, &dat_blob)?;
  let v2_dat: VaultxV2Dat = serde_json::from_slice(&dat_json).map_err(|e| e.to_string())?;

  Ok((v2_dat, v2_meta))
}

pub fn preview_vaultx(app: AppHandle, req: VaultxPreviewRequest) -> Result<VaultxPreviewResult, String> {
  let root = vault_root(&app);
  let (dk, mk, device_id) = require_state()?;
  if is_current_device_revoked(&root, &dk, &device_id)? {
    return Err("设备已被吊销".into());
  }

  let (v2_dat, _v2_meta) = read_vaultx_v2_dat(&mk, &req.file_path)?;
  let local_entries = list_entries(app)?;

  let local_active: Vec<_> = local_entries.active.iter().filter(|a| !a.deleted).collect();
  let import_active: Vec<_> = v2_dat.entries.iter().filter(|a| !a.deleted).collect();

  let mut diffs = Vec::new();

  // Find differences
  for import_acc in &import_active {
    let local_match = local_active.iter().find(|a| {
      a.name == import_acc.title && a.account_identity == import_acc.username
    });
    match local_match {
      Some(local_acc) => {
        let mut changes = Vec::new();
        if local_acc.password != import_acc.password {
          changes.push("密码不同".to_string());
        }
        if local_acc.tags != import_acc.tags {
          changes.push("标签不同".to_string());
        }
        if local_acc.notes != import_acc.notes {
          changes.push("备注不同".to_string());
        }
        if local_acc.url != import_acc.url {
          changes.push("链接不同".to_string());
        }
        if !changes.is_empty() {
          diffs.push(AccountDiff {
            name: import_acc.title.clone(),
            account_identity: import_acc.username.clone(),
            diff_type: "modified".to_string(),
            details: Some(changes.join(", ")),
          });
        }
      }
      None => {
        diffs.push(AccountDiff {
          name: import_acc.title.clone(),
          account_identity: import_acc.username.clone(),
          diff_type: "added".to_string(),
          details: None,
        });
      }
    }
  }

  // Find deleted (in local but not in import)
  for local_acc in &local_active {
    let in_import = import_active.iter().any(|a| {
      a.title == local_acc.name && a.username == local_acc.account_identity
    });
    if !in_import {
      diffs.push(AccountDiff {
        name: local_acc.name.clone(),
        account_identity: local_acc.account_identity.clone(),
        diff_type: "deleted".to_string(),
        details: Some("导入文件中不存在".to_string()),
      });
    }
  }

  // Check config changes
  let config_changed = local_entries.config.identity_presets != v2_dat.identity_presets
    || local_entries.config.service_tags != v2_dat.service_tags;

  let is_same = diffs.is_empty() && !config_changed;

  Ok(VaultxPreviewResult {
    is_same,
    local_count: local_active.len(),
    import_count: import_active.len(),
    diffs,
    config_changed,
  })
}
