//! 统一错误处理模块
//! 
//! 提供全局错误类型，替代分散的 `.map_err(|e| e.to_string())` 调用

use serde::Serialize;
use thiserror::Error;

/// 应用全局错误类型
#[derive(Error, Debug)]
pub enum AppError {
    // ========== IO 错误 ==========
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("文件不存在: {0}")]
    FileNotFound(String),

    #[error("目录不存在: {0}")]
    DirNotFound(String),

    #[error("路径无效: {0}")]
    InvalidPath(String),

    // ========== 图像处理错误 ==========
    #[error("图像错误: {0}")]
    Image(String),

    #[error("不支持的图片格式: {0}")]
    UnsupportedFormat(String),

    #[error("图片解码失败: {0}")]
    DecodeError(String),

    #[error("PDF生成失败: {0}")]
    PdfError(String),

    // ========== 加密错误 ==========
    #[error("加密错误: {0}")]
    Encryption(String),

    #[error("解密错误: {0}")]
    Decryption(String),

    #[error("HMAC校验失败")]
    HmacVerifyFailed,

    #[error("密钥派生失败: {0}")]
    KeyDerivation(String),

    // ========== Vault 错误 ==========
    #[error("保险库未初始化")]
    VaultNotInitialized,

    #[error("保险库已锁定")]
    VaultLocked,

    #[error("设备已被吊销")]
    DeviceRevoked,

    #[error("版本回退检测")]
    VersionRollback,

    #[error("条目不存在: {0}")]
    EntryNotFound(String),

    // ========== WebDAV 错误 ==========
    #[error("WebDAV未配置")]
    WebdavNotConfigured,

    #[error("WebDAV连接失败: {0}")]
    WebdavConnection(String),

    #[error("WebDAV请求失败: {0}")]
    WebdavRequest(String),

    #[error("远端目录结构无效: {0}")]
    WebdavStructure(String),

    #[error("访问非授权路径: {0}")]
    WebdavPathDenied(String),

    // ========== 序列化错误 ==========
    #[error("JSON序列化错误: {0}")]
    Json(#[from] serde_json::Error),

    // ========== 通用错误 ==========
    #[error("参数无效: {0}")]
    InvalidArgument(String),

    #[error("操作失败: {0}")]
    Operation(String),

    #[error("{0}")]
    Custom(String),
}

/// Tauri 命令返回的序列化错误
#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<AppError> for CommandError {
    fn from(err: AppError) -> Self {
        let code = match &err {
            AppError::Io(_) => "IO_ERROR",
            AppError::FileNotFound(_) => "FILE_NOT_FOUND",
            AppError::DirNotFound(_) => "DIR_NOT_FOUND",
            AppError::InvalidPath(_) => "INVALID_PATH",
            AppError::Image(_) => "IMAGE_ERROR",
            AppError::UnsupportedFormat(_) => "UNSUPPORTED_FORMAT",
            AppError::DecodeError(_) => "DECODE_ERROR",
            AppError::PdfError(_) => "PDF_ERROR",
            AppError::Encryption(_) => "ENCRYPTION_ERROR",
            AppError::Decryption(_) => "DECRYPTION_ERROR",
            AppError::HmacVerifyFailed => "HMAC_VERIFY_FAILED",
            AppError::KeyDerivation(_) => "KEY_DERIVATION_ERROR",
            AppError::VaultNotInitialized => "VAULT_NOT_INITIALIZED",
            AppError::VaultLocked => "VAULT_LOCKED",
            AppError::DeviceRevoked => "DEVICE_REVOKED",
            AppError::VersionRollback => "VERSION_ROLLBACK",
            AppError::EntryNotFound(_) => "ENTRY_NOT_FOUND",
            AppError::WebdavNotConfigured => "WEBDAV_NOT_CONFIGURED",
            AppError::WebdavConnection(_) => "WEBDAV_CONNECTION_ERROR",
            AppError::WebdavRequest(_) => "WEBDAV_REQUEST_ERROR",
            AppError::WebdavStructure(_) => "WEBDAV_STRUCTURE_ERROR",
            AppError::WebdavPathDenied(_) => "WEBDAV_PATH_DENIED",
            AppError::Json(_) => "JSON_ERROR",
            AppError::InvalidArgument(_) => "INVALID_ARGUMENT",
            AppError::Operation(_) => "OPERATION_ERROR",
            AppError::Custom(_) => "CUSTOM_ERROR",
        };
        CommandError {
            code: code.to_string(),
            message: err.to_string(),
        }
    }
}

// 实现 Tauri 命令所需的 Into<InvokeError>
impl From<AppError> for String {
    fn from(err: AppError) -> Self {
        err.to_string()
    }
}

/// 便捷类型别名
pub type AppResult<T> = Result<T, AppError>;

/// 从字符串错误转换的辅助 trait
#[allow(dead_code)]
pub trait IntoAppError<T> {
    fn map_app_err(self, f: impl FnOnce(String) -> AppError) -> AppResult<T>;
}

impl<T, E: ToString> IntoAppError<T> for Result<T, E> {
    fn map_app_err(self, f: impl FnOnce(String) -> AppError) -> AppResult<T> {
        self.map_err(|e| f(e.to_string()))
    }
}

/// 快速创建自定义错误
#[macro_export]
macro_rules! app_err {
    ($($arg:tt)*) => {
        $crate::error::AppError::Custom(format!($($arg)*))
    };
}

/// 快速返回错误
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::error::AppError::Custom(format!($($arg)*)))
    };
}
