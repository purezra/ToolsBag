use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 视频元数据
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct VideoMeta {
    pub width: u32,
    pub height: u32,
    pub bitrate: String,
    pub bitrate_raw: u64,
    pub aspect_ratio: String,
    pub frame_rate: String,
    pub frame_rate_mode: String,
    pub codec: String,
    pub hdr_format: Option<String>,
    pub duration_ms: u64,
    pub duration_str: String,
}

/// 音频元数据
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioMeta {
    pub codec: String,
    pub sample_rate: String,
    pub bitrate: String,
    pub duration_ms: u64,
    pub duration_str: String,
}

/// 图片元数据
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageMeta {
    pub width: u32,
    pub height: u32,
    pub format: String,
}

// ==================== 视频体检 - 详细元数据结构 ====================

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct DetailedVideoStream {
    pub index: u32,
    pub width: u32,
    pub height: u32,
    pub display_aspect_ratio: String,
    pub frame_rate: String,
    pub codec: String,
    pub bit_rate: String,
    pub frame_rate_mode: String,
    pub bit_depth: String,
    pub hdr_format: String,
    pub scan_type: String,
    pub format_profile: String,
    pub chroma_subsampling: String,
    pub color_space: String,
    pub color_primaries: String,
    pub transfer_characteristics: String,
    pub matrix_coefficients: String,
    pub stream_size: String,
    pub bits_per_pixel_frame: String,
    pub language: String,
    pub cabac: String,
    pub format_settings_ref_frames: String,
    pub encoded_library: String,
    pub encoded_library_settings: String,
    pub codec_id: String,
    pub duration: String,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct DetailedAudioStream {
    pub index: u32,
    pub channels: String,
    pub channel_layout: String,
    pub sample_rate: String,
    pub codec: String,
    pub bit_rate: String,
    pub bit_rate_mode: String,
    pub is_default: bool,
    pub language: String,
    pub title: String,
    pub stream_size: String,
    pub format_profile: String,
    pub compression_mode: String,
    pub duration: String,
    pub duration_ms: u64,
    pub codec_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct DetailedTextStream {
    pub index: u32,
    pub format: String,
    pub codec_id: String,
    pub language: String,
    pub title: String,
    pub is_default: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct GeneralInfo {
    pub format: String,
    pub format_long: String,
    pub file_size: u64,
    pub file_size_str: String,
    pub duration: String,
    pub duration_ms: u64,
    pub overall_bit_rate: String,
    pub title: String,
    pub encoded_date: String,
    pub writing_application: String,
    pub codec_id: String,
    pub stream_count: u32,
    pub encoded_library: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct DetailedVideoMeta {
    pub general: GeneralInfo,
    pub video_streams: Vec<DetailedVideoStream>,
    pub audio_streams: Vec<DetailedAudioStream>,
    pub text_streams: Vec<DetailedTextStream>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfoItem {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub status: String,
    pub reason: Option<String>,
    pub detail: Option<DetailedVideoMeta>,
    // 从 detail 派生的扁平字段，供媒体整理视图直接使用
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_sec: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_mbps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfoImportResponse {
    pub items: Vec<VideoInfoItem>,
    pub total: usize,
    pub success: usize,
    pub failed: usize,
}

/// 媒体文件信息
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaFileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub media_type: String,
    pub new_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageMeta>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaPreviewRequest {
    pub input_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaPreviewFile {
    pub path: PathBuf,
    pub name: String,
    pub ext: String,
    pub size: u64,
    pub media_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageMeta>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaPreviewResult {
    pub videos: Vec<MediaPreviewFile>,
    pub audios: Vec<MediaPreviewFile>,
    pub images: Vec<MediaPreviewFile>,
}
