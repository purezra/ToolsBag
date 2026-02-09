pub mod file_traverse;
pub mod image_batch;
pub mod image_to_pdf;
pub mod media_batch;
pub mod mediainfo;

// ==================== 公共媒体类型检测 ====================

/// 视频扩展名
pub const VIDEO_EXTS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", 
    "ts", "mts", "m2ts", "3gp"
];

/// 音频扩展名
pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "ape", 
    "aiff", "opus", "dts", "ac3"
];

/// 图片扩展名
pub const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "tif", 
    "ico", "heic", "heif", "svg"
];

/// 媒体类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Video,
    Audio,
    Image,
}

impl MediaType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Video => "video",
            MediaType::Audio => "audio",
            MediaType::Image => "image",
        }
    }
}

/// 根据扩展名判断媒体类型
pub fn detect_media_type(ext: &str) -> Option<MediaType> {
    let ext_lower = ext.to_lowercase();
    if VIDEO_EXTS.contains(&ext_lower.as_str()) {
        Some(MediaType::Video)
    } else if AUDIO_EXTS.contains(&ext_lower.as_str()) {
        Some(MediaType::Audio)
    } else if IMAGE_EXTS.contains(&ext_lower.as_str()) {
        Some(MediaType::Image)
    } else {
        None
    }
}

/// 从路径获取媒体类型
pub fn detect_media_type_from_path(path: &std::path::Path) -> Option<MediaType> {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(|ext| detect_media_type(ext))
}
