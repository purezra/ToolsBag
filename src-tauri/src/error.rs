//! 统一错误处理模块
//!
//! 提供全局错误类型，替代分散的 `.map_err(|e| e.to_string())` 调用

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

// 实现 Tauri 命令所需的 Into<InvokeError>
impl From<AppError> for String {
    fn from(err: AppError) -> Self {
        err.to_string()
    }
}

/// 便捷类型别名
pub type AppResult<T> = Result<T, AppError>;

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
