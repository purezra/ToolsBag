//! 统一加密模块
//!
//! 提供 AES-GCM 加密、HMAC 校验、密钥派生等功能
//! 供 codebook 和 webdav 模块共用

use aes_gcm::{
    aead::{Aead, KeyInit as AesKeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose, Engine};
use hkdf::Hkdf;
use hmac::{digest::KeyInit, Hmac, Mac};
use rand::{rngs::OsRng as RandOsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::error::{AppError, AppResult};
use crate::models::KdfParams;

pub type HmacSha256 = Hmac<Sha256>;

/// 当前加密版本号
pub const ENC_VERSION: u32 = 1042;

/// 加密后的数据块
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedBlob {
    pub ciphertext: String,
    pub iv: String,
    pub version: u32,
    pub hmac: String,
}

// ==================== Base64 编解码 ====================

/// Base64 解码
pub fn decode_b64(data: &str) -> AppResult<Vec<u8>> {
    general_purpose::STANDARD
        .decode(data)
        .map_err(|e| AppError::Decryption(format!("Base64解码失败: {}", e)))
}

/// Base64 编码
pub fn encode_b64(data: &[u8]) -> String {
    general_purpose::STANDARD.encode(data)
}

// ==================== 随机数生成 ====================

/// 生成随机字节
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    RandOsRng.fill_bytes(&mut buf);
    buf
}

/// 生成随机 IV (12 字节)
pub fn random_iv() -> [u8; 12] {
    random_bytes()
}

/// 生成随机盐 (16 字节)
#[allow(dead_code)]
pub fn random_salt() -> [u8; 16] {
    random_bytes()
}

/// 生成随机密钥 (32 字节)
#[allow(dead_code)]
pub fn random_key() -> [u8; 32] {
    random_bytes()
}

// ==================== 密钥派生 ====================

/// 使用 Argon2id 从密码派生主密钥
pub fn derive_master_key(password: &str, params: &KdfParams) -> AppResult<[u8; 32]> {
    let salt = decode_b64(&params.salt)?;
    let argon_params = Params::new(params.mem_cost, params.time_cost, params.parallelism, None)
        .map_err(|e| AppError::KeyDerivation(e.to_string()))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut mk = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), &salt, &mut mk)
        .map_err(|e| AppError::KeyDerivation(e.to_string()))?;
    Ok(mk)
}

/// 使用 HKDF 从设备密钥派生文件加密密钥和 MAC 密钥
pub fn derive_file_keys(device_key: &[u8], context: &str) -> AppResult<([u8; 32], [u8; 32])> {
    let hk = Hkdf::<Sha256>::new(None, device_key);
    let mut okm = [0u8; 64];
    hk.expand(context.as_bytes(), &mut okm)
        .map_err(|e| AppError::KeyDerivation(e.to_string()))?;
    let mut enc = [0u8; 32];
    let mut mac = [0u8; 32];
    enc.copy_from_slice(&okm[..32]);
    mac.copy_from_slice(&okm[32..]);
    Ok((enc, mac))
}

/// 使用 HKDF 派生同步密钥（WebDAV 专用）
pub fn derive_sync_keys(master_key: &[u8], context: &str) -> AppResult<([u8; 32], [u8; 32])> {
    let hk = Hkdf::<Sha256>::new(None, master_key);
    let mut okm = [0u8; 64];
    hk.expand(format!("webdav-sync:{context}").as_bytes(), &mut okm)
        .map_err(|e| AppError::KeyDerivation(e.to_string()))?;
    let mut enc = [0u8; 32];
    let mut mac = [0u8; 32];
    enc.copy_from_slice(&okm[..32]);
    mac.copy_from_slice(&okm[32..]);
    Ok((enc, mac))
}

// ==================== 加密解密 ====================

/// 使用对称密钥和 MAC 密钥加密数据
pub fn encrypt_with_key(
    symmetric: &[u8],
    mac_key: &[u8],
    plaintext: &[u8],
) -> AppResult<EncryptedBlob> {
    let iv = random_iv();

    let cipher =
        Aes256Gcm::new_from_slice(symmetric).map_err(|e| AppError::Encryption(e.to_string()))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&iv), plaintext)
        .map_err(|e| AppError::Encryption(e.to_string()))?;

    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(mac_key)
        .map_err(|e| AppError::Encryption(e.to_string()))?;
    mac.update(&ENC_VERSION.to_be_bytes());
    mac.update(&iv);
    mac.update(&ciphertext);
    let hmac = mac.finalize().into_bytes();

    Ok(EncryptedBlob {
        ciphertext: encode_b64(&ciphertext),
        iv: encode_b64(&iv),
        version: ENC_VERSION,
        hmac: encode_b64(&hmac),
    })
}

/// 使用对称密钥和 MAC 密钥解密数据
pub fn decrypt_with_key(
    symmetric: &[u8],
    mac_key: &[u8],
    blob: &EncryptedBlob,
) -> AppResult<Vec<u8>> {
    if blob.version != ENC_VERSION {
        return Err(AppError::Decryption("加密版本不匹配".into()));
    }
    let iv = decode_b64(&blob.iv)?;
    let ciphertext = decode_b64(&blob.ciphertext)?;
    let hmac_bytes = decode_b64(&blob.hmac)?;

    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(mac_key)
        .map_err(|e| AppError::Decryption(e.to_string()))?;
    mac.update(&ENC_VERSION.to_be_bytes());
    mac.update(&iv);
    mac.update(&ciphertext);
    mac.verify_slice(&hmac_bytes)
        .map_err(|_| AppError::HmacVerifyFailed)?;

    let cipher =
        Aes256Gcm::new_from_slice(symmetric).map_err(|e| AppError::Decryption(e.to_string()))?;
    cipher
        .decrypt(Nonce::from_slice(&iv), ciphertext.as_ref())
        .map_err(|e| AppError::Decryption(e.to_string()))
}

/// 使用设备密钥加密（自动派生文件密钥）
pub fn encrypt_with_device(
    device_key: &[u8],
    context: &str,
    plaintext: &[u8],
) -> AppResult<EncryptedBlob> {
    let (enc, mac) = derive_file_keys(device_key, context)?;
    encrypt_with_key(&enc, &mac, plaintext)
}

/// 使用设备密钥解密（自动派生文件密钥）
pub fn decrypt_with_device(
    device_key: &[u8],
    context: &str,
    blob: &EncryptedBlob,
) -> AppResult<Vec<u8>> {
    let (enc, mac) = derive_file_keys(device_key, context)?;
    decrypt_with_key(&enc, &mac, blob)
}

/// 包装设备密钥（使用主密钥加密设备密钥）
pub fn wrap_device_key(master_key: &[u8], device_key: &[u8]) -> AppResult<EncryptedBlob> {
    let (enc, mac) = derive_file_keys(master_key, "device-wrap")?;
    encrypt_with_key(&enc, &mac, device_key)
}

/// 解包设备密钥（使用主密钥解密设备密钥）
pub fn unwrap_device_key(master_key: &[u8], blob: &EncryptedBlob) -> AppResult<[u8; 32]> {
    let (enc, mac) = derive_file_keys(master_key, "device-wrap")?;
    let dk = decrypt_with_key(&enc, &mac, blob)?;
    if dk.len() != 32 {
        return Err(AppError::Decryption("设备密钥长度异常".into()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&dk);
    Ok(out)
}

/// WebDAV 同步加密
pub fn encrypt_for_sync(master_key: &[u8], context: &str, plaintext: &[u8]) -> AppResult<Vec<u8>> {
    let (enc_key, mac_key) = derive_sync_keys(master_key, context)?;
    let blob = encrypt_with_key(&enc_key, &mac_key, plaintext)?;
    serde_json::to_vec(&blob).map_err(|e| AppError::Encryption(e.to_string()))
}

/// WebDAV 同步解密
pub fn decrypt_from_sync(master_key: &[u8], context: &str, encrypted: &[u8]) -> AppResult<Vec<u8>> {
    let blob: EncryptedBlob =
        serde_json::from_slice(encrypted).map_err(|e| AppError::Decryption(e.to_string()))?;
    let (enc_key, mac_key) = derive_sync_keys(master_key, context)?;
    decrypt_with_key(&enc_key, &mac_key, &blob)
}

// ==================== 默认 KDF 参数 ====================

/// 创建默认的 KDF 参数
#[allow(dead_code)]
pub fn default_kdf_params() -> KdfParams {
    KdfParams {
        algorithm: "argon2id".into(),
        salt: encode_b64(&random_salt()),
        mem_cost: 64 * 1024,
        time_cost: 3,
        parallelism: 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_with_key_roundtrips_plaintext() {
        let enc_key = random_key();
        let mac_key = random_key();
        let plaintext = b"toolsbag secret payload";

        let blob = encrypt_with_key(&enc_key, &mac_key, plaintext).expect("encrypt");
        let decrypted = decrypt_with_key(&enc_key, &mac_key, &blob).expect("decrypt");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_rejects_tampered_ciphertext() {
        let enc_key = random_key();
        let mac_key = random_key();
        let plaintext = b"do not accept modified data";
        let mut blob = encrypt_with_key(&enc_key, &mac_key, plaintext).expect("encrypt");
        blob.ciphertext.push('A');

        assert!(decrypt_with_key(&enc_key, &mac_key, &blob).is_err());
    }

    #[test]
    fn sync_encryption_is_bound_to_context() {
        let master_key = random_key();
        let plaintext = b"webdav config payload";
        let encrypted = encrypt_for_sync(&master_key, "webdav-config", plaintext).expect("encrypt");

        let decrypted =
            decrypt_from_sync(&master_key, "webdav-config", &encrypted).expect("decrypt");
        assert_eq!(decrypted, plaintext);
        assert!(decrypt_from_sync(&master_key, "other-context", &encrypted).is_err());
    }
}
