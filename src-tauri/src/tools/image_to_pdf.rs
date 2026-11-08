//! 图片合并PDF工具 - Rust后端核心模块
//!
//! 功能：
//! - 支持 JPG/JPEG、PNG、WebP、TIFF、BMP、GIF 多格式混合合并
//! - 透明通道一律预合成到白底
//! - 页面尺寸：A4、A3、B5、iPad Pro
//! - 页面方向：横版、竖版、自动
//! - 边距系统
//! - 两种合并模式：
//!     - Lossless：JPEG 原始字节直传（零重编码），其他格式 q=95；
//!       结果超过原图总大小 max_size_ratio（默认 110%）时自动降级到 q=85 / q=75
//!     - Portable：用户在 [10%, 100%] 之间设置目标占比，按经验映射调节质量+缩放

extern crate image as image_crate;

use exif::{In, Reader as ExifReader, Tag, Value};
use image_crate::codecs::png::PngDecoder;
use image_crate::codecs::webp::WebPDecoder;
use image_crate::imageops::FilterType;
use image_crate::{
    ColorType, DynamicImage, GenericImageView, ImageBuffer, ImageDecoder, ImageFormat, ImageReader,
    RgbImage, RgbaImage,
};
use jpeg_decoder::{Decoder as RawJpegDecoder, PixelFormat};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use tauri::Emitter;
use tiff::decoder::{Decoder as TiffDecoder, DecodingResult as TiffDecodingResult};
use walkdir::WalkDir;

const MM_TO_PT: f32 = 2.834_645_7;

/// PDF 页面任一边的硬上限：14400pt = 200 英寸（ISO 32000-1，Page 尺寸约束）。
/// 原图模式下长图 / 全景图会按此上限整体等比缩放，避免产出无效页面。
const MAX_PAGE_POINTS: f64 = 14400.0;
const MAX_PAGE_MM: f64 = MAX_PAGE_POINTS * 25.4 / 72.0; // = 5080 mm

/// 页面尺寸定义（单位：mm）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PageSize {
    A4,
    A3,
    B5,
    #[serde(rename = "ipad_pro")]
    IPadPro,
}

impl PageSize {
    pub fn dimensions(&self) -> (f64, f64) {
        match self {
            PageSize::A4 => (210.0, 297.0),
            PageSize::A3 => (297.0, 420.0),
            PageSize::B5 => (176.0, 250.0),
            PageSize::IPadPro => (160.4, 232.8),
        }
    }

    pub fn short_edge(&self) -> f64 {
        let (w, h) = self.dimensions();
        w.min(h)
    }
}

/// 页面模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PageMode {
    /// 原图大小：每页尺寸 = 图片像素（按 72 DPI 换算），横图横摆竖图竖摆，无边距
    #[default]
    Original,
    /// 固定幅面：所有图片统一到 A4/B5 等尺寸，contain 缩放居中
    Fixed {
        /// 幅面方向：portrait / landscape / auto
        #[serde(default = "default_fixed_orientation")]
        orientation: FixedOrientation,
    },
}

/// 固定幅面的方向选择
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FixedOrientation {
    /// 自动：根据横竖图数量自动选择
    #[default]
    Auto,
    /// 强制竖向（幅面 高 > 宽）
    Portrait,
    /// 强制横向（幅面 宽 > 高）
    Landscape,
}

fn default_fixed_orientation() -> FixedOrientation {
    FixedOrientation::Auto
}

/// 合并模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MergeMode {
    /// 原样模式：JPEG 原始字节直传，其余格式以 Flate 无损嵌入；
    /// 不限制输出体积，也不做任何有损重编码（等价于旧「PDF合成（原样）」）
    Original,
    /// 无损模式：原始 JPEG 直接 passthrough，其他格式 q=95；
    /// 总输出 > 原图总大小 × max_size_ratio 时自动降级
    Lossless {
        #[serde(default = "default_max_size_ratio")]
        max_size_ratio: f32,
    },
    /// 便携模式：目标占比（原图总大小 × target_ratio）
    Portable { target_ratio: f32 },
}

impl Default for MergeMode {
    fn default() -> Self {
        MergeMode::Lossless {
            max_size_ratio: 1.10,
        }
    }
}

fn default_max_size_ratio() -> f32 {
    1.10
}

/// 色彩处理模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ColorMode {
    /// 保真：有 ICC 就原样内嵌（ICCBased），无 ICC 按 sRGB 解释
    #[default]
    Preserve,
    /// 兼容：全部转换到 sRGB 后再编码，各查看器表现一致（广色域被压到 sRGB）
    ToSrgb,
}

/// 图片分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAnalysis {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub is_portrait: bool,
    pub format: String,
    pub has_alpha: bool,
    /// P1: 是否为 APNG（含动画帧，PDF 只取首帧）
    #[serde(default)]
    pub is_apng: bool,
    /// P1: 是否为动画 WebP（含动画帧，PDF 只取首帧）
    #[serde(default)]
    pub is_animated_webp: bool,
    /// P1: 是否为灰度图（可优化为 DeviceGray 色彩空间）
    #[serde(default)]
    pub is_grayscale: bool,
    /// P2: 原始位深（8 或 16），16-bit PNG 会被降采样到 8-bit
    #[serde(default = "default_bit_depth")]
    pub bit_depth: u16,
    /// P2: 是否有 gAMA / sRGB chunk（gamma != 1.0 / sRGB 标记）
    #[serde(default)]
    pub has_gamma: bool,
    /// P2: gamma 值（如有）
    #[serde(default)]
    pub gamma: Option<f64>,
    /// 文件字节大小（用于列表视图展示）
    #[serde(default)]
    pub file_size: u64,
    /// TIFF 帧数（多页 TIFF 的页数，非 TIFF 为 1）。
    /// analyze 阶段统计，用于让预览页数与最终 PDF 页数保持一致。
    #[serde(default = "default_frame_count")]
    pub frame_count: usize,
    /// P2: 是否含 ICC 色彩配置（PNG iCCP / JPEG APP2 / WebP ICCP / TIFF 34675）
    #[serde(default)]
    pub has_icc: bool,
    /// P2: ICC 配置描述（如 "Adobe RGB (1998)", "Display P3", "sRGB" 等）
    #[serde(default)]
    pub icc_profile: Option<String>,
    /// ICC 主版本号（4 = ICC v4，部分老旧 PDF 阅读器支持较差）
    #[serde(default)]
    pub icc_version: Option<u8>,
    /// P2: 从 EXIF 读取的色彩空间标记（相机直出 JPEG 常无内嵌 ICC，只写 EXIF 标记）。
    /// 取值："srgb" / "adobe_rgb" / "uncalibrated"；无标记为 None。
    #[serde(default)]
    pub exif_color_space: Option<String>,
    /// P3: 是否为长图（长宽比 > 5:1），可能生成超长 PDF 页面
    #[serde(default)]
    pub is_long_image: bool,
}

fn default_bit_depth() -> u16 {
    8
}

fn default_frame_count() -> usize {
    1
}

/// 方向分类（用于分析报告）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrientationClass {
    Portrait,
    Landscape,
    Square,
}

/// 长宽比在 [0.95, 1.05] 之间视为方图
fn classify_orientation(w: u32, h: u32) -> OrientationClass {
    if w == 0 || h == 0 {
        return OrientationClass::Landscape;
    }
    let ratio = w as f32 / h as f32;
    if (0.95..=1.05).contains(&ratio) {
        OrientationClass::Square
    } else if h > w {
        OrientationClass::Portrait
    } else {
        OrientationClass::Landscape
    }
}

/// 分辨率档位（按长边像素分桶）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionBucket {
    /// "FHD (1080p)" 之类的展示名
    pub label: String,
    /// 该桶包含的图片数
    pub count: usize,
    /// 该桶的长边下限（用于排序展示）
    pub min_long_edge: u32,
}

/// 按长边对单张图分桶
fn classify_resolution(w: u32, h: u32) -> (&'static str, u32) {
    let long = w.max(h);
    if long < 1280 {
        ("SD (< 720p)", 0)
    } else if long < 1920 {
        ("HD (720p)", 1280)
    } else if long < 2560 {
        ("FHD (1080p)", 1920)
    } else if long < 3840 {
        ("QHD / 2K", 2560)
    } else if long < 5120 {
        ("4K UHD", 3840)
    } else if long < 7680 {
        ("5K", 5120)
    } else {
        ("8K+", 7680)
    }
}

/// 文件夹分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderAnalysis {
    pub images: Vec<ImageAnalysis>,
    /// 实际扫描的目录（路径自动识别后；若用户传入文件，会被改成其父目录）
    pub effective_path: String,
    /// 实际是否递归扫描（粘贴文件路径时强制为 false）
    pub recursive: bool,
    /// 用户传入的是文件，被自动切换到了父目录
    pub auto_switched_from_file: bool,
    pub portrait_count: usize,
    pub landscape_count: usize,
    pub square_count: usize,
    pub suggested_orientation: String,
    pub total_size: u64,
    pub resolution_buckets: Vec<ResolutionBucket>,
    /// 跳过的文件（识别到但暂不支持的格式）
    pub skipped: Vec<SkippedFile>,
    /// P1: APNG 文件数（PDF 只取首帧，动画丢失）
    #[serde(default)]
    pub apng_count: usize,
    /// P1: 动画 WebP 文件数（PDF 只取首帧，动画丢失）
    #[serde(default)]
    pub webp_animated_count: usize,
    /// P2: 16-bit PNG 文件数（降采样到 8-bit，精度有损）
    #[serde(default)]
    pub bit16_count: usize,
    /// P2: 含 gamma 信息的文件数（PDF 不内嵌 ICC/gamma，可能有轻微色偏）
    #[serde(default)]
    pub gamma_count: usize,
    /// P2: 含 ICC 色彩配置的文件数（广色域图片可能有偏色）
    #[serde(default)]
    pub icc_count: usize,
    /// ICC v4 profile 的文件数（老旧阅读器支持较差，建议用兼容模式）
    #[serde(default)]
    pub icc_v4_count: usize,
    /// P3: 长图文件数（长宽比 > 5:1，可能生成超长页面）
    #[serde(default)]
    pub long_image_count: usize,
}

/// 批量模式（按子文件夹合并）的单项：一个子文件夹的分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItemAnalysis {
    /// 文件夹名（PDF 文件名的基础，生成时会净化非法字符）
    pub name: String,
    /// 文件夹绝对路径
    pub path: String,
    /// 该文件夹内所有图片的分析结果
    pub analysis: FolderAnalysis,
}

/// 单页布局
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutResult {
    /// 该页对应的源图片绝对路径，前端按它取缩略图，避免索引错位
    pub image_path: String,
    pub page_width: f64,
    pub page_height: f64,
    pub margin: f64,
    pub image: ImageLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageLayout {
    pub original_width: u32,
    pub original_height: u32,
    pub scaled_width: f64,
    pub scaled_height: f64,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewData {
    pub pages: Vec<LayoutResult>,
    pub page_size: PageSize,
    pub page_mode: PageMode,
    pub margin: f64,
}

/// PDF 生成配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfConfig {
    pub page_size: PageSize,
    pub page_mode: PageMode,
    pub margin: f64,
    pub output_path: String,
    #[serde(default)]
    pub merge_mode: MergeMode,
    /// 是否内嵌 ICC 色彩配置（默认开启）。开启后含 ICC 的图片
    /// 以 ICCBased 色彩空间嵌入 PDF，广色域图片色彩正确。
    #[serde(default)]
    pub embed_icc: Option<bool>,
    /// 色彩处理模式（默认保真）
    #[serde(default)]
    pub color_mode: ColorMode,
}

/// 生成进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationProgress {
    pub current: usize,
    pub total: usize,
    pub current_file: String,
    pub phase: String,
}

/// 生成结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationResult {
    pub success: bool,
    pub output_path: String,
    pub file_size: u64,
    pub original_total_size: u64,
    pub page_count: usize,
    pub elapsed_ms: u64,
    /// 实际使用的合并模式描述（"无损 q=95"、"无损降级 q=85" 等）
    pub mode_used: String,
    /// 输出 / 原始大小比例（1.05 = 105%）
    pub size_ratio: f32,
    /// 是否超过 lossless 模式的目标阈值（无损模式下用户配置的 max_size_ratio）
    pub exceeded_target: bool,
    /// 跳过的文件及原因（超大图、损坏等）
    #[serde(default)]
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

/// 批量生成结果（每个子文件夹一项）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchGenerationResult {
    /// 实际使用的 PDF 文件名（不含 .pdf，可能已被净化/去重）
    pub name: String,
    /// 输出文件绝对路径
    pub output_path: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<GenerationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 批量生成过程中逐条推送的事件负载（pdf_batch_item）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItemEvent {
    /// 对应 `analyze_subfolders_for_pdf` 返回数组中的下标
    pub index: usize,
    pub item: BatchGenerationResult,
}

// ==================== 布局计算 ====================

fn is_portrait_image(width: u32, height: u32) -> bool {
    height > width
}

/// 该图片是否应按 Adobe RGB 解释：无内嵌 ICC，但 EXIF ColorSpace + InteropIndex 标记为 Adobe RGB
/// （相机直出 JPEG 常见，只写 EXIF 标记而不内嵌完整 profile）。
fn assume_adobe_rgb(img: &ImageAnalysis) -> bool {
    !img.has_icc && img.exif_color_space.as_deref() == Some("adobe_rgb")
}

/// 72 DPI 下像素 → 毫米
fn px_to_mm(px: u32) -> f64 {
    px as f64 * 25.4 / 72.0
}

/// 解析幅面方向（auto 模式下根据图片方向决定）
fn resolve_fixed_orientation(
    orientation: FixedOrientation,
    img_width: u32,
    img_height: u32,
) -> FixedOrientation {
    match orientation {
        FixedOrientation::Auto => {
            if is_portrait_image(img_width, img_height) {
                FixedOrientation::Portrait
            } else {
                FixedOrientation::Landscape
            }
        }
        other => other,
    }
}

/// 根据页面模式获取 PDF 页面尺寸（mm）
fn get_page_dimensions(
    page_mode: &PageMode,
    page_size: PageSize,
    img_width: u32,
    img_height: u32,
) -> (f64, f64) {
    match page_mode {
        PageMode::Original => {
            // 原图大小：每页 = 图片像素按 72 DPI 换算
            let mut w = px_to_mm(img_width);
            let mut h = px_to_mm(img_height);
            // 横图横摆、竖图竖摆（像素尺寸本身已蕴含方向）
            // PDF 页面任一边不得超过 14400pt（200 英寸）：超限则整体等比缩放
            let s = (MAX_PAGE_MM / w.max(h)).min(1.0);
            w *= s;
            h *= s;
            (w, h)
        }
        PageMode::Fixed { orientation } => {
            let (base_w, base_h) = page_size.dimensions(); // 竖版尺寸
            let resolved = resolve_fixed_orientation(*orientation, img_width, img_height);
            match resolved {
                FixedOrientation::Portrait => (base_w, base_h),
                FixedOrientation::Landscape => (base_h, base_w),
                FixedOrientation::Auto => (base_w, base_h), // 不应到达
            }
        }
    }
}

fn get_draw_area(page_width: f64, page_height: f64, margin: f64) -> (f64, f64) {
    (page_width - 2.0 * margin, page_height - 2.0 * margin)
}

/// 原图模式无幅面限制，不需要边距校验
fn validate_margin_for_mode(
    margin: f64,
    page_mode: &PageMode,
    page_size: PageSize,
) -> Result<f64, String> {
    match page_mode {
        PageMode::Original => {
            if margin < 0.0 {
                return Err("边距不能为负".to_string());
            }
            Ok(margin)
        }
        PageMode::Fixed { .. } => {
            let short_edge = page_size.short_edge();
            let max_margin = short_edge * 0.4;
            if margin < 0.0 {
                return Err("边距不能为负".to_string());
            }
            if margin > max_margin {
                return Err(format!(
                    "边距不能超过页面短边的40%（最大 {:.1} mm）",
                    max_margin
                ));
            }
            Ok(margin)
        }
    }
}

fn calculate_image_layout(
    img_width: u32,
    img_height: u32,
    page_width: f64,
    page_height: f64,
    margin: f64,
) -> ImageLayout {
    let (w_draw, h_draw) = get_draw_area(page_width, page_height, margin);

    let ratio_img = img_height as f64 / img_width as f64;
    let ratio_draw = h_draw / w_draw;

    let (scaled_width, scaled_height, x, y) = if ratio_draw >= ratio_img {
        let scale = w_draw / img_width as f64;
        let sw = img_width as f64 * scale;
        let sh = img_height as f64 * scale;
        let offset_y = (h_draw - sh) / 2.0;
        (sw, sh, margin, margin + offset_y)
    } else {
        let scale = h_draw / img_height as f64;
        let sw = img_width as f64 * scale;
        let sh = img_height as f64 * scale;
        let offset_x = (w_draw - sw) / 2.0;
        (sw, sh, margin + offset_x, margin)
    };

    ImageLayout {
        original_width: img_width,
        original_height: img_height,
        scaled_width,
        scaled_height,
        x,
        y,
    }
}

fn calculate_page_layout(
    image: &ImageAnalysis,
    page_mode: &PageMode,
    page_size: PageSize,
    margin: f64,
) -> LayoutResult {
    let (page_width, page_height) =
        get_page_dimensions(page_mode, page_size, image.width, image.height);
    let effective_margin = match page_mode {
        PageMode::Original => 0.0, // 原图模式无边距
        PageMode::Fixed { .. } => margin,
    };
    let image_layout = calculate_image_layout(
        image.width,
        image.height,
        page_width,
        page_height,
        effective_margin,
    );
    LayoutResult {
        image_path: image.path.clone(),
        page_width,
        page_height,
        margin: effective_margin,
        image: image_layout,
    }
}

// ==================== 文件扫描 / 自然序 ====================

/// 主流位图格式：全链路支持（解码 → 嵌入 PDF）
const SUPPORTED_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "tif", "tiff", "bmp", "gif",
    // image crate 内置解码 + 已启用 feature
    "tga", "dds", "pbm", "pgm", "ppm", "pam", "ff", "hdr", "qoi", "ico",
];

/// 小众/有坑格式：能识别但跳过，汇总到警告列表
const SKIPPED_FORMATS: &[(&str, &str)] = &[
    ("heic", "HEIC/HEIF（苹果拍照格式，需系统解码器）"),
    ("heif", "HEIC/HEIF（苹果拍照格式，需系统解码器）"),
    ("avif", "AVIF（新一代压缩格式，暂不支持）"),
    ("jxl", "JPEG XL（新一代格式，暂不支持）"),
    ("svg", "SVG（矢量格式，非位图）"),
    ("raw", "相机 RAW（需专业软件转换）"),
    ("cr2", "Canon RAW（需专业软件转换）"),
    ("nef", "Nikon RAW（需专业软件转换）"),
    ("arw", "Sony RAW（需专业软件转换）"),
    ("dng", "DNG RAW（需专业软件转换）"),
    ("exr", "OpenEXR（影视 HDR 格式，需色调映射）"),
    ("jp2", "JPEG2000（暂不支持）"),
    ("j2k", "JPEG2000（暂不支持）"),
    ("psd", "PSD（Photoshop 源文件，非位图图片）"),
    ("ai", "AI（Illustrator 源文件，非位图图片）"),
];

/// 通过文件头 magic number 检测真实格式。
/// 读取前 12 字节即可区分绝大多数格式；扩展名仅作辅助。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetectedFormat {
    Jpeg,
    Png,
    WebP,
    Tiff,
    Bmp,
    Gif,
    Ico,
    /// 支持但低频的小众格式（TGA/DDS/PNM/Farbfeld/HDR/QOI），走 image crate 通用路径
    OtherImage,
    /// PDF 文件伪装成图片
    Pdf,
    /// PSD/AI 等设计源文件
    Psd,
    /// SVG 矢量
    Svg,
    /// JP2/JPEG2000
    Jp2,
    /// 无法识别
    Unknown,
}

fn detect_format_by_magic(path: &Path) -> DetectedFormat {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return DetectedFormat::Unknown,
    };
    let mut buf = [0u8; 12];
    // 读取可能不足 12 字节（极小文件），用实际读取长度判断
    let n = match std::io::Read::read(&mut file, &mut buf) {
        Ok(n) => n,
        Err(_) => return DetectedFormat::Unknown,
    };
    let h = &buf[..n];

    if h.len() >= 3 && h[0] == 0xFF && h[1] == 0xD8 && h[2] == 0xFF {
        return DetectedFormat::Jpeg;
    }
    if h.len() >= 8 && h[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return DetectedFormat::Png;
    }
    if h.len() >= 12 && h[..4] == [0x52, 0x49, 0x46, 0x46] && h[8..12] == [0x57, 0x45, 0x42, 0x50] {
        return DetectedFormat::WebP;
    }
    if h.len() >= 4 && (h[..4] == [0x49, 0x49, 0x2A, 0x00] || h[..4] == [0x4D, 0x4D, 0x00, 0x2A]) {
        return DetectedFormat::Tiff;
    }
    if h.len() >= 2 && h[0] == 0x42 && h[1] == 0x4D {
        return DetectedFormat::Bmp;
    }
    if h.len() >= 4 && h[..4] == [0x47, 0x49, 0x46, 0x38] {
        return DetectedFormat::Gif;
    }
    if h.len() >= 4 && h[..4] == [0x00, 0x00, 0x01, 0x00] {
        return DetectedFormat::Ico;
    }
    if h.len() >= 4 && h[..4] == [0x25, 0x50, 0x44, 0x46] {
        // %PDF — PDF 伪装成图片
        return DetectedFormat::Pdf;
    }
    if h.len() >= 4 && h[..4] == [0x38, 0x42, 0x50, 0x53] {
        // 8BPS — Photoshop PSD
        return DetectedFormat::Psd;
    }
    if h.len() >= 8 && h[..8] == [0x00, 0x00, 0x00, 0x0C, 0x6A, 0x50, 0x20, 0x20] {
        return DetectedFormat::Jp2;
    }
    // SVG 检测：文本开头 <?xml 或 <svg（只检测前 5 字节）
    if h.len() >= 5 {
        let prefix = String::from_utf8_lossy(h);
        if prefix.trim_start().starts_with("<?xml") || prefix.trim_start().starts_with("<svg") {
            return DetectedFormat::Svg;
        }
    }

    // 扩展名辅助：小众格式 image crate 可解码但无明确 magic
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "tga" | "dds" | "pbm" | "pgm" | "ppm" | "pam" | "ff" | "hdr" | "qoi" => {
                return DetectedFormat::OtherImage;
            }
            _ => {}
        }
    }

    DetectedFormat::Unknown
}

impl DetectedFormat {
    /// 是否为可直接处理的图片格式
    fn is_processable(&self) -> bool {
        matches!(
            self,
            DetectedFormat::Jpeg
                | DetectedFormat::Png
                | DetectedFormat::WebP
                | DetectedFormat::Tiff
                | DetectedFormat::Bmp
                | DetectedFormat::Gif
                | DetectedFormat::Ico
                | DetectedFormat::OtherImage
        )
    }

    /// 转为 ImageAnalysis 中的 format 字段值
    fn to_format_string(self) -> Option<String> {
        match self {
            DetectedFormat::Jpeg => Some("jpeg".to_string()),
            DetectedFormat::Png => Some("png".to_string()),
            DetectedFormat::WebP => Some("webp".to_string()),
            DetectedFormat::Tiff => Some("tiff".to_string()),
            DetectedFormat::Bmp => Some("bmp".to_string()),
            DetectedFormat::Gif => Some("gif".to_string()),
            DetectedFormat::Ico => Some("ico".to_string()),
            DetectedFormat::OtherImage => {
                // 从扩展名推断
                None
            }
            _ => None,
        }
    }

    /// 伪装/不支持格式的跳过原因
    fn skip_reason(&self) -> Option<&'static str> {
        match self {
            DetectedFormat::Pdf => Some("文件实际是 PDF，不是图片"),
            DetectedFormat::Psd => Some("文件实际是 PSD/AI 设计源文件，不是位图图片"),
            DetectedFormat::Svg => Some("SVG 是矢量格式，非位图图片"),
            DetectedFormat::Jp2 => Some("JPEG2000 格式暂不支持，请先转换为 JPEG/PNG"),
            _ => None,
        }
    }
}

/// 跳过的文件信息（前端弹窗展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedFile {
    pub path: String,
    pub ext: String,
    pub reason: String,
}

fn detect_image_format(path: &Path) -> Option<String> {
    // 优先用 magic number
    let detected = detect_format_by_magic(path);
    if let Some(fmt) = detected.to_format_string() {
        return Some(fmt);
    }
    if detected == DetectedFormat::OtherImage {
        // 小众格式从扩展名推断
        let ext = path.extension()?.to_str()?.to_lowercase();
        return match ext.as_str() {
            "tga" => Some("tga".to_string()),
            "dds" => Some("dds".to_string()),
            "pbm" | "pgm" | "ppm" | "pam" => Some("pnm".to_string()),
            "ff" => Some("farbfeld".to_string()),
            "hdr" => Some("hdr".to_string()),
            "qoi" => Some("qoi".to_string()),
            _ => None,
        };
    }
    // 回退到扩展名
    let ext = path.extension()?.to_str()?.to_lowercase();
    if SUPPORTED_EXTS.contains(&ext.as_str()) {
        match ext.as_str() {
            "jpg" | "jpeg" => Some("jpeg".to_string()),
            "tif" | "tiff" => Some("tiff".to_string()),
            "pbm" | "pgm" | "ppm" | "pam" => Some("pnm".to_string()),
            "ff" => Some("farbfeld".to_string()),
            other => Some(other.to_string()),
        }
    } else {
        None
    }
}

fn is_supported_ext(ext: &str) -> bool {
    SUPPORTED_EXTS.contains(&ext)
}

/// 检查是否属于"能识别但跳过"的格式
fn check_skipped_format(path: &Path) -> Option<SkippedFile> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    for &(key, reason) in SKIPPED_FORMATS {
        if ext == key {
            return Some(SkippedFile {
                path: path.to_string_lossy().to_string(),
                ext: ext.to_string(),
                reason: reason.to_string(),
            });
        }
    }
    None
}

/// 自然序比较：IMG_2.jpg < IMG_10.jpg
fn natural_compare(a: &OsStr, b: &OsStr) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let sa = a.to_string_lossy();
    let sb = b.to_string_lossy();
    let mut ai = sa.chars().peekable();
    let mut bi = sb.chars().peekable();

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let mut na = String::new();
                    while let Some(&c) = ai.peek() {
                        if c.is_ascii_digit() {
                            na.push(c);
                            ai.next();
                        } else {
                            break;
                        }
                    }
                    let mut nb = String::new();
                    while let Some(&c) = bi.peek() {
                        if c.is_ascii_digit() {
                            nb.push(c);
                            bi.next();
                        } else {
                            break;
                        }
                    }
                    let na_v: u128 = na.parse().unwrap_or(0);
                    let nb_v: u128 = nb.parse().unwrap_or(0);
                    match na_v.cmp(&nb_v) {
                        Ordering::Equal => {
                            // 数字相等再比原字符串长度（处理 "01" vs "1"）
                            match na.len().cmp(&nb.len()) {
                                Ordering::Equal => continue,
                                ord => return ord,
                            }
                        }
                        ord => return ord,
                    }
                } else {
                    let cal = ca.to_ascii_lowercase();
                    let cbl = cb.to_ascii_lowercase();
                    match cal.cmp(&cbl) {
                        Ordering::Equal => {
                            ai.next();
                            bi.next();
                            continue;
                        }
                        ord => return ord,
                    }
                }
            }
        }
    }
}

struct ScanResult {
    images: Vec<PathBuf>,
    skipped: Vec<SkippedFile>,
}

fn scan_folder(folder_path: &Path, recursive: bool) -> Result<ScanResult, String> {
    let mut images = Vec::new();
    let mut skipped = Vec::new();

    let collect_entry = |path: &Path, imgs: &mut Vec<PathBuf>, skip: &mut Vec<SkippedFile>| {
        if !path.is_file() {
            return;
        }
        // 优先用 magic number 检测真实格式
        let detected = detect_format_by_magic(path);
        if detected.is_processable() {
            imgs.push(path.to_path_buf());
            return;
        }
        // 伪装/不支持格式 → 给出明确原因
        if let Some(reason) = detected.skip_reason() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("?")
                .to_string();
            skip.push(SkippedFile {
                path: path.to_string_lossy().to_string(),
                ext,
                reason: reason.to_string(),
            });
            return;
        }
        // magic 未识别 → 退回扩展名判断（兼容无 magic 的小众格式）
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            if is_supported_ext(&ext_lower) {
                imgs.push(path.to_path_buf());
            } else if let Some(sf) = check_skipped_format(path) {
                skip.push(sf);
            }
            // 完全不认识的扩展名 → 静默忽略
        }
    };

    if recursive {
        for entry in WalkDir::new(folder_path)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            collect_entry(entry.path(), &mut images, &mut skipped);
        }
    } else {
        for entry in std::fs::read_dir(folder_path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            collect_entry(&entry.path(), &mut images, &mut skipped);
        }
    }
    // 用整条路径做自然序：递归场景下子目录里的同名编号也能正确排在一起
    images.sort_by(|a, b| natural_compare(a.as_os_str(), b.as_os_str()));
    Ok(ScanResult { images, skipped })
}

/// 路径自动识别：
/// - 不存在 → Err
/// - 是文件 → 取父目录，并强制非递归（用户场景：粘贴了图片路径）
/// - 是目录 → 直接用
fn resolve_input_path(input: &Path) -> Result<(PathBuf, bool /* auto_switched */), String> {
    if !input.exists() {
        return Err(format!("路径不存在：{}", input.display()));
    }
    if input.is_file() {
        let parent = input.parent().ok_or_else(|| "无法取得父目录".to_string())?;
        if !parent.is_dir() {
            return Err("父目录不可访问".into());
        }
        Ok((parent.to_path_buf(), true))
    } else if input.is_dir() {
        Ok((input.to_path_buf(), false))
    } else {
        Err("路径既不是文件也不是文件夹".into())
    }
}

// ==================== 内存保护 ====================

/// 单图解码内存阈值（400 MB ≈ 100Mpx × 4 字节/像素）
const MAX_IMAGE_MEMORY_BYTES: u64 = 400 * 1024 * 1024;
/// 同时解码的大图最大数量（限制 rayon 并发）
const MAX_CONCURRENT_LARGE_DECODES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryCheck {
    Ok,
    Oversized {
        estimated_mb: u64,
        width: u32,
        height: u32,
    },
}

/// 不解码像素，仅读宽高估算内存开销
fn check_image_memory(width: u32, height: u32) -> MemoryCheck {
    let estimated = width as u64 * height as u64 * 4; // RGBA worst case
    if estimated > MAX_IMAGE_MEMORY_BYTES {
        MemoryCheck::Oversized {
            estimated_mb: estimated / (1024 * 1024),
            width,
            height,
        }
    } else {
        MemoryCheck::Ok
    }
}

/// 全局闸门：限制同时解码的大图数量。
/// 用 Mutex + Condvar 阻塞等待——早期版本是 `while !try_acquire { yield_now() }`
/// 的忙等自旋，会把 rayon 的工作线程全部烧在空转上。
struct LargeDecodeGate {
    active: Mutex<usize>,
    cv: Condvar,
}

impl LargeDecodeGate {
    fn global() -> &'static LargeDecodeGate {
        static GATE: OnceLock<LargeDecodeGate> = OnceLock::new();
        GATE.get_or_init(|| LargeDecodeGate {
            active: Mutex::new(0),
            cv: Condvar::new(),
        })
    }

    /// 申请一个解码位；名额满时阻塞（不占 CPU），直到有人释放。
    fn acquire(&'static self) -> LargeDecodeGuard {
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        while *active >= MAX_CONCURRENT_LARGE_DECODES {
            active = self.cv.wait(active).unwrap_or_else(|e| e.into_inner());
        }
        *active += 1;
        LargeDecodeGuard { gate: self }
    }
}

struct LargeDecodeGuard {
    gate: &'static LargeDecodeGate,
}

impl Drop for LargeDecodeGuard {
    fn drop(&mut self) {
        let mut active = self.gate.active.lock().unwrap_or_else(|e| e.into_inner());
        *active = active.saturating_sub(1);
        self.gate.cv.notify_one();
    }
}

// ==================== 图片解码 / 透明处理 / EXIF ====================

fn has_alpha_channel(img: &DynamicImage) -> bool {
    matches!(
        img,
        DynamicImage::ImageRgba8(_)
            | DynamicImage::ImageRgba16(_)
            | DynamicImage::ImageRgba32F(_)
            | DynamicImage::ImageLumaA8(_)
            | DynamicImage::ImageLumaA16(_)
    )
}

/// RGBA → RGB（白底预合成）
fn flatten_rgba_to_white(rgba: &RgbaImage) -> RgbImage {
    let (w, h) = rgba.dimensions();
    let mut rgb = RgbImage::new(w, h);
    rgb.par_chunks_mut(w as usize * 3)
        .enumerate()
        .for_each(|(y, chunk)| {
            for x in 0..w as usize {
                let pixel = rgba.get_pixel(x as u32, y as u32);
                let [r, g, b, a] = pixel.0;
                let alpha = a as f32 / 255.0;
                let inv = 1.0 - alpha;
                let idx = x * 3;
                chunk[idx] = (r as f32 * alpha + 255.0 * inv) as u8;
                chunk[idx + 1] = (g as f32 * alpha + 255.0 * inv) as u8;
                chunk[idx + 2] = (b as f32 * alpha + 255.0 * inv) as u8;
            }
        });
    rgb
}

/// 根据 EXIF orientation 修正分析阶段使用的宽高，确保预览/布局与最终解码结果一致。
/// （process_image 里改成读一次 orientation 复用三处，故直接用 [`apply_orientation_value`] 的语义。）
fn dimensions_after_exif_orientation(
    width: u32,
    height: u32,
    orientation: Option<u32>,
) -> (u32, u32) {
    match orientation {
        Some(5..=8) => (height, width),
        _ => (width, height),
    }
}

fn analyze_image(path: &Path) -> Result<ImageAnalysis, String> {
    let format = detect_image_format(path).ok_or("不支持的图片格式")?;
    let (raw_width, raw_height) =
        image_crate::image_dimensions(path).map_err(|e| format!("无法读取图片尺寸: {}", e))?;
    let (width, height) =
        dimensions_after_exif_orientation(raw_width, raw_height, read_exif_orientation(path));

    // P1+P2: 对 PNG 格式检测 APNG、灰度、16-bit、gamma 元数据
    let png_meta = if format == "png" {
        Some(inspect_png_metadata(path))
    } else {
        None
    };
    let (is_apng, is_grayscale, bit_depth, has_gamma, gamma) = match &png_meta {
        Some(m) => (m.is_apng, m.is_grayscale, m.bit_depth, m.has_gamma, m.gamma),
        None => (false, false, 8, false, None),
    };
    // ICC：一次头部扫描同时拿到「有没有」「叫什么」「哪个版本」。
    // JPEG/PNG/WebP/TIFF 都支持；不再整文件读盘，也不再对同一文件扫两遍。
    let (has_icc, icc_profile, icc_version) = scan_icc_metadata(path, &format);

    // 是否实际含透明通道：
    // - PNG / WebP / JPEG 走精确检测（仅读头部，不解码像素），避免 RGB 照片误报 "Alpha"
    // - 其余格式无低成本确认手段，按格式保守标记（误报仅影响 UI 提示，不影响输出）
    let (is_animated_webp, has_alpha) = match format.as_str() {
        "png" => (
            false,
            png_meta.as_ref().map(|m| m.has_alpha).unwrap_or(false),
        ),
        "webp" => {
            let (alpha, animated) = inspect_webp_info(path);
            (animated, alpha)
        }
        "jpeg" => (false, false),
        other => (
            false,
            matches!(other, "tiff" | "gif" | "ico" | "tga" | "dds" | "farbfeld"),
        ),
    };

    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    // 多页 TIFF 需统计帧数，让预览/页数/进度与实际 PDF 页数一致
    let frame_count = if format == "tiff" {
        count_tiff_frames(path)
    } else {
        1
    };
    // EXIF ColorSpace 标记：相机直出 JPEG 常见（无内嵌 ICC 时用于推断色彩空间）
    let exif_color_space = if format == "jpeg" {
        read_exif_color_space(path)
    } else {
        None
    };

    Ok(ImageAnalysis {
        path: path.to_string_lossy().to_string(),
        width,
        height,
        is_portrait: is_portrait_image(width, height),
        format,
        has_alpha,
        is_apng,
        is_animated_webp,
        is_grayscale,
        bit_depth,
        has_gamma,
        gamma,
        file_size,
        frame_count,
        has_icc,
        icc_profile,
        icc_version,
        exif_color_space,
        is_long_image: width > 0
            && height > 0
            && (width as f32 / height as f32 > 5.0 || height as f32 / width as f32 > 5.0),
    })
}

fn decode_image_fast(path: &Path, guessed_format: ImageFormat) -> Result<DynamicImage, String> {
    // WebP 走专用快速路径
    if matches!(guessed_format, ImageFormat::WebP) {
        let file = File::open(path).map_err(|e| format!("无法打开 WebP: {}", e))?;
        let reader = BufReader::new(file);
        let decoder =
            WebPDecoder::new(reader).map_err(|e| format!("WebP 解码器创建失败: {}", e))?;
        let (w, h) = decoder.dimensions();
        let color = decoder.color_type();
        let total_bytes = decoder.total_bytes() as usize;
        let mut buf = vec![0u8; total_bytes];
        decoder
            .read_image(&mut buf)
            .map_err(|e| format!("WebP 解码失败: {}", e))?;

        return match color {
            ColorType::Rgb8 => ImageBuffer::from_raw(w, h, buf)
                .map(DynamicImage::ImageRgb8)
                .ok_or_else(|| "生成 WebP RGB 图像失败".to_string()),
            ColorType::Rgba8 => ImageBuffer::from_raw(w, h, buf)
                .map(DynamicImage::ImageRgba8)
                .ok_or_else(|| "生成 WebP RGBA 图像失败".to_string()),
            _ => Err("不支持的 WebP 颜色类型".to_string()),
        };
    }

    // ICO / GIF 可能含多帧，取最大帧
    if matches!(guessed_format, ImageFormat::Ico | ImageFormat::Gif) {
        let img = image_crate::open(path)
            .map_err(|e| format!("解码 {:?} 失败: {}", guessed_format, e))?;
        return Ok(img);
    }

    // 通用路径
    ImageReader::open(path)
        .map_err(|e| format!("无法打开图片 {}: {}", path.display(), e))?
        .with_guessed_format()
        .map_err(|e| format!("格式检测失败 {}", e))?
        .decode()
        .map_err(|e| format!("图片解码失败: {}", e))
}

/// 读取 EXIF orientation；返回 None 表示无 EXIF 或 orientation=1（无需处理）
fn read_exif_orientation(path: &Path) -> Option<u32> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif = ExifReader::new().read_from_container(&mut reader).ok()?;
    let raw = exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))?;
    if raw == 1 {
        None
    } else {
        Some(raw)
    }
}

/// 读取 EXIF ColorSpace 标记（配合 InteroperabilityIndex）推断色彩空间。
/// 相机直出 JPEG 常不内嵌 ICC，只在 EXIF 里写 ColorSpace 标记：
/// - 1（sRGB）或 InteropIndex "R98" → "srgb"
/// - 2（未校准）+ InteropIndex "R03" → "adobe_rgb"（Adobe RGB 官方互操作标识）
/// - 2 且无 R03 → "uncalibrated"（无法确认，保守按 sRGB 处理，仅作提示）
/// - 其它/无标记 → None
fn read_exif_color_space(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif = ExifReader::new().read_from_container(&mut reader).ok()?;
    let cs = exif
        .get_field(Tag::ColorSpace, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0));
    match cs {
        Some(1) => Some("srgb".to_string()),
        Some(2) => {
            let interop = exif
                .get_field(Tag::InteroperabilityIndex, In::PRIMARY)
                .and_then(|f| match &f.value {
                    Value::Ascii(parts) => parts
                        .first()
                        .map(|p| String::from_utf8_lossy(p).to_string()),
                    _ => None,
                });
            match interop.as_deref() {
                Some("R98") => Some("srgb".to_string()),
                Some("R03") => Some("adobe_rgb".to_string()),
                _ => Some("uncalibrated".to_string()),
            }
        }
        _ => None,
    }
}

/// 按 EXIF orientation 值对图像进行变换；orientation 为 None（无 EXIF 或为 1）时原样返回，
/// 避免无故 RGBA 化。抽出 value 版本以便多页 TIFF 复用：EXIF 方向对整个文件生效，
/// 每帧解码后套用同一个 orientation 即可，无需重复读盘解析 EXIF。
fn apply_orientation_value(img: DynamicImage, orientation: Option<u32>) -> DynamicImage {
    let Some(orientation) = orientation else {
        return img;
    };
    let base = img.to_rgba8();
    let rotated = match orientation {
        2 => image_crate::imageops::flip_horizontal(&base),
        3 => image_crate::imageops::rotate180(&base),
        4 => image_crate::imageops::flip_vertical(&base),
        5 => image_crate::imageops::flip_horizontal(&image_crate::imageops::rotate90(&base)),
        6 => image_crate::imageops::rotate90(&base),
        7 => image_crate::imageops::flip_horizontal(&image_crate::imageops::rotate270(&base)),
        8 => image_crate::imageops::rotate270(&base),
        _ => return DynamicImage::ImageRgba8(base),
    };
    DynamicImage::ImageRgba8(rotated)
}

/// 仅在 orientation != 1 时进行变换；否则原样返回，避免无故 RGBA 化
fn apply_exif_orientation(img: DynamicImage, path: &Path) -> DynamicImage {
    apply_orientation_value(img, read_exif_orientation(path))
}

fn jpeg_is_cmyk(path: &Path) -> bool {
    if let Ok(file) = std::fs::File::open(path) {
        let mut decoder = RawJpegDecoder::new(BufReader::new(file));
        if decoder.read_info().is_ok() {
            return matches!(
                decoder.info().map(|i| i.pixel_format),
                Some(PixelFormat::CMYK32)
            );
        }
    }
    false
}

/// CMYK JPEG → RGB。
/// 尺寸以解码器自报的原始像素尺寸为准（**不是** EXIF 旋转后的尺寸），
/// 调用方负责在拿到 RGB 图之后再套用 EXIF 方向，避免用转置后的宽高
/// 重新解释原始像素序导致画面被撕裂。
fn convert_cmyk_to_rgb_image(path: &Path) -> Result<RgbImage, String> {
    let mut decoder = RawJpegDecoder::new(std::fs::File::open(path).map_err(|e| e.to_string())?);
    let pixels = decoder
        .decode()
        .map_err(|e| format!("CMYK 解码失败: {}", e))?;
    let info = decoder.info().ok_or("无法获取 JPEG 信息")?;
    if !matches!(info.pixel_format, PixelFormat::CMYK32) {
        return Err("不是 CMYK JPEG".into());
    }
    let mut rgb = Vec::with_capacity(pixels.len() / 4 * 3);
    for chunk in pixels.chunks_exact(4) {
        let c = chunk[0] as f32 / 255.0;
        let m = chunk[1] as f32 / 255.0;
        let y = chunk[2] as f32 / 255.0;
        let k = chunk[3] as f32 / 255.0;
        let r = (1.0 - (c * (1.0 - k) + k)) * 255.0;
        let g = (1.0 - (m * (1.0 - k) + k)) * 255.0;
        let b = (1.0 - (y * (1.0 - k) + k)) * 255.0;
        rgb.extend_from_slice(&[r.round() as u8, g.round() as u8, b.round() as u8]);
    }
    RgbImage::from_raw(info.width as u32, info.height as u32, rgb)
        .ok_or_else(|| "CMYK 转换失败：像素数据与尺寸不匹配".to_string())
}

fn encode_rgb_to_jpeg_processed(rgb: &RgbImage, quality: u8) -> Result<ProcessedImage, String> {
    let (w, h) = rgb.dimensions();
    let mut buffer = Cursor::new(Vec::new());
    let mut encoder =
        image_crate::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, quality);
    encoder
        .encode(rgb.as_raw(), w, h, image_crate::ExtendedColorType::Rgb8)
        .map_err(|e| format!("JPEG编码失败: {}", e))?;
    let mut data = buffer.into_inner();
    inject_app14_ycbcr(&mut data);

    Ok(ProcessedImage {
        data,
        width: w,
        height: h,
        encoding: ImageEncoding::Dct,
        alpha_data: None,
        icc_profile_bytes: None,
    })
}

/// 将原始 RGB 像素数据压缩为 PDF 可直接使用的 FlateDecode 流。
/// 这里就完成 zlib 压缩（而不是留到写 PDF 时），有两个好处：
/// 1) 与 JPEG 路径做「谁更小」比较时比的是压缩后体积，否则 Flate 永远吃亏；
/// 2) 内存里存的是压缩数据，不是 w*h*3 的原始像素。
fn encode_rgb_to_flate_processed(rgb: &RgbImage) -> ProcessedImage {
    let (w, h) = rgb.dimensions();
    ProcessedImage {
        data: deflate_zlib(rgb.as_raw()),
        width: w,
        height: h,
        encoding: ImageEncoding::Flate,
        alpha_data: None,
        icc_profile_bytes: None,
    }
}

/// PNG 内容分类：采样像素统计唯一颜色数，判断是线条艺术还是照片。
/// - 唯一颜色 < 4096 → Flate（线条艺术 / 索引色 / 文字）
/// - 否则 → Dct（照片类内容）
fn classify_png_content(rgb: &RgbImage) -> ImageEncoding {
    let (w, h) = rgb.dimensions();
    let total_pixels = w as usize * h as usize;
    // 采样间隔：每 4 个像素取 1 个，加速统计
    let step = 4;
    let sample_count = total_pixels.div_ceil(step);
    let mut colors = HashSet::with_capacity(sample_count.min(4096));
    let raw = rgb.as_raw();
    for i in (0..total_pixels).step_by(step) {
        let idx = i * 3;
        if idx + 2 < raw.len() {
            // 将 RGB 三字节打包为 u32 的高 24 位作为 key
            let key = (raw[idx] as u32) << 16 | (raw[idx + 1] as u32) << 8 | raw[idx + 2] as u32;
            colors.insert(key);
            // 提前退出：已超过阈值，无需继续
            if colors.len() >= 4096 {
                return ImageEncoding::Dct;
            }
        }
    }
    ImageEncoding::Flate
}

/// PNG 元数据检测结果
#[derive(Debug, Clone, Default)]
struct PngMetadata {
    /// 是否为 APNG（含动画帧）
    is_apng: bool,
    /// 原始位深（8 或 16）
    bit_depth: u16,
    /// 是否为灰度图（ColorType::Luma8 / LumaA8 / Luma16 / LumaA16）
    is_grayscale: bool,
    /// 是否实际含透明通道（ColorType 含 A 分量）
    has_alpha: bool,
    /// 是否有 gAMA / sRGB chunk（gamma != 1.0 / sRGB 标记）
    has_gamma: bool,
    /// gamma 值（如有）
    gamma: Option<f64>,
}

/// 读取 PNG 文件的元数据（不解码像素），用于 APNG 检测、16-bit 警告、灰度优化等。
/// ICC 相关字段已移到 [`scan_icc_metadata`]，此处不再重复扫描 chunk。
fn inspect_png_metadata(path: &Path) -> PngMetadata {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return PngMetadata::default(),
    };
    let reader = BufReader::new(file);
    let decoder = match PngDecoder::new(reader) {
        Ok(d) => d,
        Err(_) => return PngMetadata::default(),
    };

    let is_apng = decoder.is_apng().unwrap_or(false);
    let color_type = decoder.color_type();
    let is_grayscale = matches!(
        color_type,
        ColorType::L8 | ColorType::L16 | ColorType::La8 | ColorType::La16
    );
    let has_alpha = matches!(
        color_type,
        ColorType::La8 | ColorType::La16 | ColorType::Rgba8 | ColorType::Rgba16
    );

    // 获取位深：通过 color_type 推断
    let bit_depth = match color_type {
        ColorType::L16 | ColorType::La16 | ColorType::Rgb16 | ColorType::Rgba16 => 16,
        _ => 8,
    };

    let gamma = decoder.gamma_value().unwrap_or(None);
    let has_gamma = gamma.is_some();

    PngMetadata {
        is_apng,
        bit_depth,
        is_grayscale,
        has_alpha,
        has_gamma,
        gamma,
    }
}

/// 读取 WebP 头部信息（不解码像素）：返回 (是否含透明通道, 是否动画)。
/// 失败时保守返回 (false, false)（has_alpha 误报仅影响 UI 提示，不影响输出）。
fn inspect_webp_info(path: &Path) -> (bool, bool) {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return (false, false),
    };
    match WebPDecoder::new(BufReader::new(file)) {
        Ok(d) => (d.color_type() == ColorType::Rgba8, d.has_animation()),
        Err(_) => (false, false),
    }
}

// ==================== ICC 扫描（只读文件头，不整文件读盘） ====================
//
// 早期实现用 `std::fs::read(path)` 把整个文件读进内存来判 ICC，一张 50MB 的
// JPEG 会被完整读两遍（检测一次、取名一次）。这里改为流式扫描：JPEG 只走
// APP 段、PNG 只走 chunk 头、WebP/TIFF 只读容器目录，遇到大数据段直接 seek 跳过。

/// 分析阶段读取 ICC 的预算：够解析 tag 表和 desc 名称即可，不必读整个 profile。
const ICC_HEADER_BUDGET: usize = 64 * 1024;
/// 单个内嵌 ICC profile 的硬上限，防止畸形文件把内存撑爆。
const ICC_MAX_BYTES: usize = 16 * 1024 * 1024;
/// PNG 文件签名
const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// 从已打开的文件里读一段字节（不改变外部语义，调用后文件位置未定义）。
fn read_range_at(file: &mut File, offset: u64, len: usize) -> Option<Vec<u8>> {
    if len == 0 {
        return Some(Vec::new());
    }
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; len];
    let mut filled = 0usize;
    while filled < len {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }
    buf.truncate(filled);
    Some(buf)
}

/// 打开文件并读取 [offset, offset+len) 区间，len 受 budget 限制。
fn read_range_budgeted(path: &Path, offset: u64, len: usize, budget: usize) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    read_range_at(&mut file, offset, len.min(budget))
}

/// 扫描 JPEG APP 段中的 ICC_PROFILE 分片并按序号重组。
///
/// - 遇到 SOS（图像数据开始）即停止，因此**不会读入扫描数据**
/// - 非 APP2 段的数据用 seek 跳过，最多只读 14 字节段头
/// - `budget` 限制整个 profile 读取的总字节数（跨分片累计）：分析阶段传
///   [`ICC_HEADER_BUDGET`]，生成阶段传 [`ICC_MAX_BYTES`] 取完整 profile
///
/// 返回 `(profile, truncated)`；完全不含 ICC 时返回 None。
fn scan_jpeg_icc(path: &Path, budget: usize) -> Option<(Vec<u8>, bool)> {
    let mut file = File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();
    let mut soi = [0u8; 2];
    file.read_exact(&mut soi).ok()?;
    if soi != [0xFF, 0xD8] {
        return None;
    }

    let mut chunks: Vec<(u8, u8, Vec<u8>)> = Vec::new();
    let mut truncated = false;
    let mut remaining = budget;

    loop {
        let mut marker = [0u8; 2];
        if file.read_exact(&mut marker).is_err() {
            break;
        }
        if marker[0] != 0xFF {
            break; // marker 流损坏
        }
        let m = marker[1];
        if m == 0xFF {
            // 连续 FF 填充：回退一字节重新同步
            if file.seek(SeekFrom::Current(-1)).is_err() {
                break;
            }
            continue;
        }
        if m == 0x01 || (0xD0..=0xD9).contains(&m) {
            if m == 0xD9 {
                break; // EOI
            }
            continue; // 无长度字段的独立标记
        }

        let mut len_buf = [0u8; 2];
        if file.read_exact(&mut len_buf).is_err() {
            break;
        }
        let seg_len = u16::from_be_bytes(len_buf) as u64;
        if seg_len < 2 {
            break;
        }
        let payload_len = seg_len - 2;
        let Ok(payload_start) = file.stream_position() else {
            break;
        };
        if payload_start + payload_len > file_len {
            break; // 段被截断
        }

        if m == 0xE2 && payload_len >= 14 {
            if let Some(ident) = read_range_at(&mut file, payload_start, 14) {
                if ident[..12] == *b"ICC_PROFILE\x00" {
                    let data_len = (payload_len - 14) as usize;
                    let take = data_len.min(remaining);
                    if take < data_len {
                        truncated = true; // profile 未完整读入（仅够解析名称）
                    }
                    remaining = remaining.saturating_sub(take);
                    if let Some(data) = read_range_at(&mut file, payload_start + 14, take) {
                        chunks.push((ident[12], ident[13], data));
                    }
                }
            }
        }

        if m == 0xDA {
            break; // SOS：图像数据开始，ICC 只可能出现在段区域
        }
        if file
            .seek(SeekFrom::Start(payload_start + payload_len))
            .is_err()
        {
            break;
        }
    }

    if chunks.is_empty() {
        return None;
    }
    let total = chunks.iter().map(|c| c.1 as usize).max()?;
    if total == 0 || total > 255 {
        return None;
    }
    let mut assembled: Vec<Option<Vec<u8>>> = (0..total).map(|_| None).collect();
    for (seq, count, bytes) in chunks {
        if seq == 0 || count as usize != total {
            continue;
        }
        let idx = seq as usize - 1;
        if idx < total && assembled[idx].is_none() {
            assembled[idx] = Some(bytes);
        }
    }
    let profile: Vec<u8> = assembled.into_iter().flatten().flatten().collect();
    if profile.is_empty() {
        None
    } else {
        Some((profile, truncated))
    }
}

/// PNG 色彩相关 chunk 的扫描结果（一次遍历拿到全部信息）。
#[derive(Debug, Default, Clone)]
struct PngColorScan {
    /// 含可内嵌的 iCCP 色彩配置
    has_icc: bool,
    /// iCCP 中的 profile 名称（NUL 结尾的 Latin-1 字符串）
    icc_name: Option<String>,
    /// 只声明了 sRGB chunk（无内嵌 profile，等价于「已是 sRGB」，无需特殊处理）
    srgb_tagged: bool,
}

/// 单次遍历 PNG chunk 链，读取色彩相关信息。只读 chunk 头，数据段一律 seek 跳过。
fn scan_png_color_chunks(path: &Path) -> PngColorScan {
    let mut scan = PngColorScan::default();
    let Ok(mut file) = File::open(path) else {
        return scan;
    };
    let Ok(file_len) = file.metadata().map(|m| m.len()) else {
        return scan;
    };
    let mut sig = [0u8; 8];
    if file.read_exact(&mut sig).is_err() || sig != PNG_SIGNATURE {
        return scan;
    }
    loop {
        let mut head = [0u8; 8];
        if file.read_exact(&mut head).is_err() {
            break;
        }
        let chunk_len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as u64;
        let chunk_type = &head[4..8];
        if chunk_type == b"IEND" {
            break;
        }
        let Ok(data_start) = file.stream_position() else {
            break;
        };
        // data + 4 字节 CRC
        if data_start + chunk_len + 4 > file_len {
            break;
        }
        match chunk_type {
            b"iCCP" => {
                scan.has_icc = true;
                if scan.icc_name.is_none() && chunk_len > 0 {
                    let take = (chunk_len as usize).min(256);
                    if let Some(bytes) = read_range_at(&mut file, data_start, take) {
                        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                        let name = String::from_utf8_lossy(&bytes[..end]).trim().to_string();
                        if !name.is_empty() {
                            scan.icc_name = Some(name);
                        }
                    }
                }
            }
            b"sRGB" => scan.srgb_tagged = true,
            _ => {}
        }
        if file
            .seek(SeekFrom::Start(data_start + chunk_len + 4))
            .is_err()
        {
            break;
        }
    }
    scan
}

/// 在 RIFF/WEBP 容器里定位 ICCP chunk，返回 (数据偏移, 长度)。
fn webp_icc_chunk(path: &Path) -> Option<(u64, usize)> {
    let mut file = File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();
    let mut header = [0u8; 12];
    file.read_exact(&mut header).ok()?;
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WEBP" {
        return None;
    }
    let mut pos = 12u64;
    while pos + 8 <= file_len {
        let head = read_range_at(&mut file, pos, 8)?;
        let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as u64;
        let data_start = pos + 8;
        if data_start + size > file_len {
            return None;
        }
        if &head[0..4] == b"ICCP" {
            if size == 0 || size > ICC_MAX_BYTES as u64 {
                return None;
            }
            return Some((data_start, size as usize));
        }
        pos = data_start + size + (size & 1); // chunk 按偶数字节对齐
    }
    None
}

/// 定位 TIFF 的 ICC profile 标签（34675）数据区，返回 (偏移, 长度)。
/// 仅支持经典 TIFF（magic 42），BigTIFF 返回 None。
fn tiff_icc_tag(path: &Path) -> Option<(u64, usize)> {
    let mut file = File::open(path).ok()?;
    let mut head = [0u8; 8];
    file.read_exact(&mut head).ok()?;
    let big_endian = match &head[0..2] {
        b"II" => false,
        b"MM" => true,
        _ => return None,
    };
    let rd16 = |b: &[u8]| -> u16 {
        if big_endian {
            u16::from_be_bytes([b[0], b[1]])
        } else {
            u16::from_le_bytes([b[0], b[1]])
        }
    };
    let rd32 = |b: &[u8]| -> u32 {
        if big_endian {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        }
    };
    if rd16(&head[2..4]) != 42 {
        return None; // BigTIFF 或非法头
    }
    let ifd_off = rd32(&head[4..8]) as u64;
    if ifd_off < 8 {
        return None;
    }
    let count = rd16(&read_range_at(&mut file, ifd_off, 2)?) as usize;
    if count == 0 || count > 4096 {
        return None;
    }
    let entries = read_range_at(&mut file, ifd_off + 2, count * 12)?;
    for i in 0..count {
        let e = entries.get(i * 12..i * 12 + 12)?;
        if rd16(&e[0..2]) != 34675 {
            continue;
        }
        let size = rd32(&e[4..8]) as usize;
        if size <= 4 || size > ICC_MAX_BYTES {
            return None;
        }
        return Some((rd32(&e[8..12]) as u64, size));
    }
    None
}

/// 解析 ICC profile 的 desc tag，返回可读名称。
/// 同时支持 v2 的 textDescriptionType（'desc'）与 v4 的 multiLocalizedUnicodeType（'mluc'）。
fn icc_profile_name(profile: &[u8]) -> Option<String> {
    if !is_icc_profile(profile) {
        return None;
    }
    let count = u32::from_be_bytes(profile.get(128..132)?.try_into().ok()?) as usize;
    if count == 0 || count > 4096 {
        return None;
    }
    for i in 0..count {
        let base = 132 + i * 12;
        let entry = profile.get(base..base + 12)?;
        if &entry[0..4] != b"desc" {
            continue;
        }
        let off = u32::from_be_bytes(entry[4..8].try_into().ok()?) as usize;
        let size = u32::from_be_bytes(entry[8..12].try_into().ok()?) as usize;
        let data = profile.get(off..off.checked_add(size)?)?;
        return parse_icc_desc(data);
    }
    None
}

fn parse_icc_desc(data: &[u8]) -> Option<String> {
    let sig = data.get(0..4)?;
    if sig == b"desc" {
        // textDescriptionType: sig + reserved(4) + asciiCount(4) + ascii[]
        let ascii_len = u32::from_be_bytes(data.get(8..12)?.try_into().ok()?) as usize;
        let ascii = data.get(12..12 + ascii_len.min(256))?;
        return clean_icc_text(ascii);
    }
    if sig == b"mluc" {
        // multiLocalizedUnicodeType: sig + reserved(4) + count(4) + recSize(4) + records[]
        let rec_count = u32::from_be_bytes(data.get(8..12)?.try_into().ok()?) as usize;
        let rec_size = u32::from_be_bytes(data.get(12..16)?.try_into().ok()?) as usize;
        if rec_size < 12 || rec_count == 0 {
            return None;
        }
        let mut fallback: Option<String> = None;
        for i in 0..rec_count.min(64) {
            let start = 16 + i * rec_size;
            let rec = data.get(start..start + 12)?;
            let lang = &rec[0..2];
            let len = u32::from_be_bytes(rec[4..8].try_into().ok()?) as usize;
            let off = u32::from_be_bytes(rec[8..12].try_into().ok()?) as usize;
            let raw = match data.get(off..off.checked_add(len)?) {
                Some(r) => r,
                None => continue, // 预算截断，换下一条记录
            };
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            let text = String::from_utf16_lossy(&units)
                .trim_matches(|c: char| c == '\0' || c.is_whitespace())
                .to_string();
            if text.is_empty() {
                continue;
            }
            if lang == b"en" {
                return Some(text);
            }
            fallback.get_or_insert(text);
        }
        return fallback;
    }
    None
}

fn clean_icc_text(bytes: &[u8]) -> Option<String> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let text = String::from_utf8_lossy(&bytes[..end]).trim().to_string();
    if text.is_empty() || !text.chars().any(|c| c.is_alphanumeric()) {
        None
    } else {
        Some(text)
    }
}

/// ICC header 的版本号（主版本, 次版本），偏移 8..12 是 u8.8.8.8。
fn icc_version(profile: &[u8]) -> Option<u8> {
    profile.get(8).copied()
}

/// 分析阶段统一入口：只读文件头判断是否含可内嵌 ICC，并尽量取出 profile 名称。
/// 返回 (是否有可内嵌的 ICC, profile 名称, ICC 主版本)。
fn scan_icc_metadata(path: &Path, format: &str) -> (bool, Option<String>, Option<u8>) {
    match format {
        "jpeg" => match scan_jpeg_icc(path, ICC_HEADER_BUDGET) {
            Some((profile, truncated)) => {
                let name = if truncated {
                    Some("ICC (embedded)".to_string())
                } else {
                    icc_profile_name(&profile).or_else(|| Some("ICC (embedded)".to_string()))
                };
                (true, name, icc_version(&profile))
            }
            None => (false, None, None),
        },
        "png" => {
            let scan = scan_png_color_chunks(path);
            if scan.has_icc {
                // iCCP 名称是 PNG 侧声明的，够用；不再解压 profile 取版本
                (true, scan.icc_name, None)
            } else {
                (false, None, None)
            }
        }
        "webp" => match webp_icc_chunk(path) {
            Some((off, len)) => match read_range_budgeted(path, off, len, ICC_HEADER_BUDGET) {
                Some(head) => (true, icc_profile_name(&head), icc_version(&head)),
                None => (true, None, None),
            },
            None => (false, None, None),
        },
        "tiff" => match tiff_icc_tag(path) {
            Some((off, len)) => match read_range_budgeted(path, off, len, ICC_HEADER_BUDGET) {
                Some(head) => (true, icc_profile_name(&head), icc_version(&head)),
                None => (true, None, None),
            },
            None => (false, None, None),
        },
        _ => (false, None, None),
    }
}

// ==================== ICC 配置提取 ====================

/// 判断字节流是否为合法 ICC profile：长度足够且偏移 36..40 为 "acsp" 签名。
fn is_icc_profile(data: &[u8]) -> bool {
    data.len() >= 44 && data.get(36..40) == Some(b"acsp")
}

/// 读取 ICC profile header 中的色彩空间（偏移 16..20），返回分量数。
/// - "RGB " → 3，CMYK → 4，GRAY → 1，其它/非法 → 0
fn icc_components(data: &[u8]) -> u8 {
    match data.get(16..20) {
        Some(b"RGB ") => 3,
        Some(b"CMYK") => 4,
        Some(b"GRAY") => 1,
        _ => 0,
    }
}

/// 从 JPEG 字节流中拼接 APP2 ICC_PROFILE 分片，返回完整 ICC profile。
/// 大 profile 会被拆成多个 APP2 段（各含序号与总数），需按序号重组。
fn extract_jpeg_icc_from_bytes(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    let mut chunks: Vec<(u8, u8, Vec<u8>)> = Vec::new(); // (seq, total, bytes)
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            break;
        }
        let marker = data[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        if i + 3 >= data.len() {
            break;
        }
        let seg_len = ((data[i + 2] as usize) << 8) | data[i + 3] as usize;
        if seg_len < 2 || i + 2 + seg_len > data.len() {
            break;
        }
        if marker == 0xE2 {
            let payload = &data[i + 4..i + 2 + seg_len];
            if payload.len() >= 14 && payload[..12] == *b"ICC_PROFILE\x00" {
                chunks.push((payload[12], payload[13], payload[14..].to_vec()));
            }
        }
        if marker == 0xDA {
            break; // SOS：图像数据开始，ICC 只出现在段区域
        }
        i += 2 + seg_len;
    }
    if chunks.is_empty() {
        return None;
    }
    let total = chunks.iter().map(|c| c.1 as usize).max()?;
    let mut assembled: Vec<Vec<u8>> = vec![Vec::new(); total];
    for (seq, count, bytes) in chunks {
        if seq == 0 || (count as usize) != total {
            continue;
        }
        let idx = seq as usize - 1;
        if idx < total && assembled[idx].is_empty() {
            assembled[idx] = bytes;
        }
    }
    let profile: Vec<u8> = assembled.into_iter().flatten().collect();
    if profile.is_empty() || !is_icc_profile(&profile) {
        None
    } else {
        Some(profile)
    }
}

/// 从 PNG 文件的 iCCP chunk 中解压出 ICC profile（zlib 压缩，方法 0）。
fn extract_png_icc_bytes(path: &Path) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let mut sig = [0u8; 8];
    std::io::Read::read_exact(&mut file, &mut sig).ok()?;
    if sig != [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return None;
    }
    loop {
        let mut len_buf = [0u8; 4];
        let mut type_buf = [0u8; 4];
        if std::io::Read::read_exact(&mut file, &mut len_buf).is_err()
            || std::io::Read::read_exact(&mut file, &mut type_buf).is_err()
        {
            break;
        }
        let chunk_len = u32::from_be_bytes(len_buf) as usize;
        if &type_buf == b"iCCP" {
            if chunk_len == 0 || chunk_len > ICC_MAX_BYTES {
                return None;
            }
            let mut data = vec![0u8; chunk_len];
            if std::io::Read::read_exact(&mut file, &mut data).is_err() {
                return None;
            }
            // iCCP data: profile 名称(NUL 结尾) + 1 字节压缩方法(0=zlib) + 压缩后的 profile
            let name_end = data.iter().position(|&b| b == 0)?;
            let comp_start = name_end + 1;
            if comp_start + 1 >= data.len() || data[comp_start] != 0 {
                return None;
            }
            let mut out = Vec::new();
            let mut decoder =
                flate2::read::ZlibDecoder::new(std::io::Cursor::new(&data[comp_start + 1..]));
            decoder.read_to_end(&mut out).ok()?;
            return if is_icc_profile(&out) {
                Some(out)
            } else {
                None
            };
        }
        if &type_buf == b"IEND" {
            break;
        }
        let skip = chunk_len + 4; // 跳过 data + CRC
        if std::io::Seek::seek(&mut file, std::io::SeekFrom::Current(skip as i64)).is_err() {
            break;
        }
    }
    None
}

/// 按格式提取源图片的内嵌 ICC profile。
/// JPEG 走流式 APP2 扫描（不读扫描数据），PNG 解压 iCCP，WebP 读 ICCP，TIFF 读 34675 标签。
fn extract_icc_for_image(path: &Path, format: ImageFormat) -> Option<Vec<u8>> {
    match format {
        ImageFormat::Jpeg => scan_jpeg_icc(path, ICC_MAX_BYTES).map(|(profile, _)| profile),
        ImageFormat::Png => extract_png_icc_bytes(path),
        ImageFormat::WebP => {
            let (off, len) = webp_icc_chunk(path)?;
            read_range_budgeted(path, off, len, ICC_MAX_BYTES)
        }
        ImageFormat::Tiff => {
            let (off, len) = tiff_icc_tag(path)?;
            read_range_budgeted(path, off, len, ICC_MAX_BYTES)
        }
        _ => None,
    }
}

// ==================== Adobe RGB (1998) ICC profile 生成 ====================

fn mat3_mul(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut c = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            for k in 0..3 {
                c[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    c
}

fn mat3_vec(a: &[[f64; 3]; 3], v: &[f64; 3]) -> [f64; 3] {
    let mut r = [0.0; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i] += a[i][j] * v[j];
        }
    }
    r
}

fn mat3_inv(a: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let d = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    let mut inv = [[0.0; 3]; 3];
    inv[0][0] = (a[1][1] * a[2][2] - a[1][2] * a[2][1]) / d;
    inv[0][1] = (a[0][2] * a[2][1] - a[0][1] * a[2][2]) / d;
    inv[0][2] = (a[0][1] * a[1][2] - a[0][2] * a[1][1]) / d;
    inv[1][0] = (a[1][2] * a[2][0] - a[1][0] * a[2][2]) / d;
    inv[1][1] = (a[0][0] * a[2][2] - a[0][2] * a[2][0]) / d;
    inv[1][2] = (a[0][2] * a[1][0] - a[0][0] * a[1][2]) / d;
    inv[2][0] = (a[1][0] * a[2][1] - a[1][1] * a[2][0]) / d;
    inv[2][1] = (a[0][1] * a[2][0] - a[0][0] * a[2][1]) / d;
    inv[2][2] = (a[0][0] * a[1][1] - a[0][1] * a[1][0]) / d;
    inv
}

/// 由基色 + 白点（CIE xy，白点 Y 归一为 1）构造 RGB→XYZ 矩阵。
/// 8 个参数是数学定义本身（6 个基色坐标 + 2 个白点坐标），拆成结构体反而更难读。
#[allow(clippy::too_many_arguments)]
fn rgb_to_xyz_matrix(
    rx: f64,
    ry: f64,
    gx: f64,
    gy: f64,
    bx: f64,
    by: f64,
    wx: f64,
    wy: f64,
) -> [[f64; 3]; 3] {
    // 各基色归一化到 Y=1 的 XYZ
    let m0 = [
        [rx / ry, gx / gy, bx / by],
        [1.0, 1.0, 1.0],
        [
            (1.0 - rx - ry) / ry,
            (1.0 - gx - gy) / gy,
            (1.0 - bx - by) / by,
        ],
    ];
    let white = [wx / wy, 1.0, (1.0 - wx - wy) / wy];
    let s = mat3_vec(&mat3_inv(&m0), &white); // 使 R=G=B=1 映射到白点的缩放系数
    let mut m = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = m0[i][j] * s[j];
        }
    }
    m
}

/// Bradford 色适应：把 RGB→XYZ 矩阵从源白点（src）适应到目标白点（dst）。
fn bradford_adapt(m: &[[f64; 3]; 3], src: &[f64; 3], dst: &[f64; 3]) -> [[f64; 3]; 3] {
    const MB: [[f64; 3]; 3] = [
        [0.8951, 0.2664, -0.1614],
        [-0.7502, 1.7135, 0.0367],
        [0.0389, -0.0685, 1.0296],
    ];
    const MB_INV: [[f64; 3]; 3] = [
        [0.9869929, -0.1470543, 0.1599627],
        [0.4323053, 0.5183603, 0.0492912],
        [-0.0085287, 0.0400428, 0.9684867],
    ];
    let src_cone = mat3_vec(&MB, src);
    let dst_cone = mat3_vec(&MB, dst);
    let mut diag = [[0.0; 3]; 3];
    for i in 0..3 {
        diag[i][i] = dst_cone[i] / src_cone[i];
    }
    let adapt = mat3_mul(&MB_INV, &mat3_mul(&diag, &MB));
    mat3_mul(&adapt, m)
}

/// f64 → ICC 16.16 定点
fn icc_u16_16(v: f64) -> u32 {
    (v * 65536.0).round() as u32
}

/// 构造 Adobe RGB (1998) ICC v2 matrix-shaper profile（D65 白点，Bradford 适应到 D50 PCS）。
/// 用于「无内嵌 ICC 但 EXIF 标记为 Adobe RGB」的相机直出 JPEG，以 ICCBased 色彩空间嵌入 PDF。
/// 色彩参数来自 Adobe RGB (1998) 公开色度：基色 R(0.64,0.33) G(0.21,0.71) B(0.15,0.06)，白点 D65，TRC γ2.2。
fn build_adobe_rgb_profile() -> Vec<u8> {
    build_matrix_shaper_profile(
        "Adobe RGB (1998)",
        "Reconstructed from published Adobe RGB colorimetry",
        (0.6400, 0.3300, 0.2100, 0.7100, 0.1500, 0.0600),
        0.3127,
        0.3290,
        2.2,
    )
}

/// 构造一个 RGB matrix-shaper ICC v2 profile（参数化：基色 / 白点 / TRC γ）。
/// `primaries` 依次为 R(x,y) G(x,y) B(x,y)。
fn build_matrix_shaper_profile(
    desc: &str,
    cprt: &str,
    primaries: (f64, f64, f64, f64, f64, f64),
    wx: f64,
    wy: f64,
    gamma: f64,
) -> Vec<u8> {
    let (rx, ry, gx, gy, bx, by) = primaries;
    let m_d65 = rgb_to_xyz_matrix(rx, ry, gx, gy, bx, by, wx, wy);
    let w65 = [wx / wy, 1.0, (1.0 - wx - wy) / wy];
    let w50 = [0.9642, 1.0, 0.8249];
    let m = bradford_adapt(&m_d65, &w65, &w50);

    // ---- 构建各 tag 的数据块 ----
    let mut tag_data: Vec<([u8; 4], Vec<u8>)> = Vec::new();

    // desc: textDescriptionType（仅 ASCII 段）
    let mut d = Vec::new();
    d.extend_from_slice(b"desc");
    d.extend_from_slice(&0u32.to_be_bytes()); // reserved
    d.extend_from_slice(&(desc.len() as u32).to_be_bytes());
    d.extend_from_slice(desc.as_bytes());
    d.extend_from_slice(&0u32.to_be_bytes()); // unicode count
    d.extend_from_slice(&0u32.to_be_bytes()); // script count
    tag_data.push((*b"desc", d));

    // cprt: textType
    let mut d = Vec::new();
    d.extend_from_slice(b"text");
    d.extend_from_slice(cprt.as_bytes());
    tag_data.push((*b"cprt", d));

    // wtpt: D50 白点
    let mut d = Vec::new();
    d.extend_from_slice(b"XYZ ");
    for v in w50 {
        d.extend_from_slice(&icc_u16_16(v).to_be_bytes());
    }
    tag_data.push((*b"wtpt", d));

    // chad: Bradford D65→D50 色适应矩阵（sf32）
    let mut d = Vec::new();
    d.extend_from_slice(b"sf32");
    let adapt = bradford_adapt(
        &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        &w65,
        &w50,
    );
    for row in adapt {
        for v in row {
            d.extend_from_slice(&((v * 65536.0).round() as i32).to_be_bytes());
        }
    }
    tag_data.push((*b"chad", d));

    // rXYZ / gXYZ / bXYZ：D50-adapted 基色（RGB→XYZ 矩阵的各列）
    let firsts = [0x72u8, 0x67, 0x62]; // 'r' 'g' 'b'
    for (col, first) in firsts.iter().enumerate() {
        let mut d = Vec::new();
        d.extend_from_slice(b"XYZ ");
        for row in &m {
            d.extend_from_slice(&icc_u16_16(row[col]).to_be_bytes());
        }
        tag_data.push(([*first, 0x58, 0x59, 0x5A], d)); // ?XYZ
    }

    // rTRC / gTRC / bTRC：γ（curv 单值，u8Fixed8）
    // curvType 布局是 type + reserved + count + values——别漏掉 reserved，
    // 少了它解析方会从错误偏移读 count，得到垃圾曲线。
    for first in firsts {
        let mut d = Vec::new();
        d.extend_from_slice(b"curv");
        d.extend_from_slice(&0u32.to_be_bytes()); // reserved
        d.extend_from_slice(&1u32.to_be_bytes()); // 1 个值
        d.extend_from_slice(&((gamma * 256.0).round() as u16).to_be_bytes());
        tag_data.push(([first, 0x54, 0x52, 0x43], d)); // ?TRC
    }

    assemble_icc_profile(tag_data)
}

/// 按 ICC 规范组装 profile：128 字节头 + tag 表 + 4 字节对齐的数据区。
fn assemble_icc_profile(tag_data: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    let tag_count = tag_data.len();
    let data_start = 128 + 4 + tag_count * 12;
    let mut offsets = Vec::with_capacity(tag_count);
    let mut data_blocks = Vec::with_capacity(tag_count);
    let mut cursor = data_start;
    for (_, data) in &tag_data {
        let align = (4 - (cursor % 4)) % 4;
        cursor += align;
        offsets.push(cursor);
        cursor += data.len();
        data_blocks.push((align, data.clone()));
    }
    let total_size = cursor;

    let mut out = Vec::with_capacity(total_size);
    out.extend_from_slice(&(total_size as u32).to_be_bytes()); // size
    out.extend_from_slice(&[0u8; 4]); // CMM
    out.extend_from_slice(&0x0210_0000u32.to_be_bytes()); // version 2.1
    out.extend_from_slice(b"mntr"); // device class
    out.extend_from_slice(b"RGB "); // color space
    out.extend_from_slice(b"XYZ "); // PCS
    for v in [2024u16, 1, 1, 0, 0, 0] {
        out.extend_from_slice(&v.to_be_bytes()); // date/time
    }
    out.extend_from_slice(b"acsp");
    out.extend_from_slice(&[0u8; 4]); // platform
    out.extend_from_slice(&[0u8; 4]); // flags
    out.extend_from_slice(&[0u8; 4]); // manufacturer
    out.extend_from_slice(&[0u8; 4]); // model
    out.extend_from_slice(&[0u8; 4]); // attributes
    out.extend_from_slice(&0u32.to_be_bytes()); // rendering intent
    for v in [0.9642, 1.0, 0.8249] {
        out.extend_from_slice(&icc_u16_16(v).to_be_bytes()); // PCS illuminant D50
    }
    out.extend_from_slice(&[0u8; 8]); // creator + profile ID
    out.extend_from_slice(&[0u8; 44]); // reserved
    debug_assert_eq!(out.len(), 128);

    out.extend_from_slice(&(tag_count as u32).to_be_bytes());
    for (i, (sig, data)) in tag_data.iter().enumerate() {
        out.extend_from_slice(sig);
        out.extend_from_slice(&(offsets[i] as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    }
    for (align, data) in &data_blocks {
        out.extend_from_slice(&vec![0u8; *align]);
        out.extend_from_slice(data);
    }
    debug_assert_eq!(out.len(), total_size);
    out
}

/// 缓存的 Adobe RGB (1998) profile 字节（构建一次复用）。
fn adobe_rgb_icc_profile() -> &'static Vec<u8> {
    static PROFILE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    PROFILE.get_or_init(build_adobe_rgb_profile)
}

// ==================== 兼容模式：转换到 sRGB ====================
//
// 保真模式把 ICC 原样内嵌，色彩最准，但部分阅读器对广色域/ICC v4 支持不好。
// 兼容模式在编码前把像素真正换算到 sRGB，输出不带 profile 的 DeviceRGB，
// 各查看器表现一致（代价是超出 sRGB 的颜色被裁剪）。
//
// 只支持矩阵/TRC 型（matrix-shaper）profile——Adobe RGB、Display P3、sRGB、
// ProPhoto 等绝大多数相机/显示器 profile 都是这一类。LUT 型（mft1/mft2/mAB）
// 无法在此实现，遇到时退化为「按 sRGB 解释」并计入警告。

/// 单个通道的色调曲线（设备值 → 线性）
#[derive(Debug, Clone)]
enum ToneCurve {
    /// 幂函数：linear = v^γ
    Gamma(f64),
    /// 查表（0..65535 映射到 0..1）
    Table(Vec<u16>),
    /// ICC parametricCurveType（funcType 0..4）
    Parametric {
        func: u8,
        g: f64,
        a: f64,
        b: f64,
        c: f64,
        d: f64,
        e: f64,
        f: f64,
    },
}

impl ToneCurve {
    fn eval(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        match self {
            ToneCurve::Gamma(g) => x.powf(*g),
            ToneCurve::Table(t) => {
                if t.is_empty() {
                    return x;
                }
                let pos = x * (t.len() - 1) as f64;
                let i = pos.floor() as usize;
                let j = (i + 1).min(t.len() - 1);
                let frac = pos - i as f64;
                let a = t[i] as f64 / 65535.0;
                let b = t[j] as f64 / 65535.0;
                a + (b - a) * frac
            }
            ToneCurve::Parametric {
                func,
                g,
                a,
                b,
                c,
                d,
                e,
                f,
            } => match func {
                0 => x.powf(*g),
                1 if x >= -b / a => (a * x + b).powf(*g),
                1 => 0.0,
                2 if x >= -b / a => (a * x + b).powf(*g) + c,
                2 => *c,
                3 if x >= *d => (a * x + b).powf(*g),
                3 => c * x,
                4 if x >= *d => (a * x + b).powf(*g) + e,
                4 => c * x + f,
                _ => x,
            },
        }
    }

    /// 采样成 256 项 LUT（输入为 8-bit 设备值）
    fn to_lut(&self) -> [f32; 256] {
        let mut lut = [0f32; 256];
        for (i, slot) in lut.iter_mut().enumerate() {
            *slot = self.eval(i as f64 / 255.0) as f32;
        }
        lut
    }
}

/// 从 ICC 二进制里解析出的 matrix-shaper RGB profile
#[derive(Debug, Clone)]
struct RgbMatrixProfile {
    /// 线性 RGB → PCS XYZ(D50)（列为 rXYZ/gXYZ/bXYZ）
    linear_to_xyz: [[f64; 3]; 3],
    trc: [ToneCurve; 3],
}

/// 读取 XYZType 标签（sig "XYZ " + 3 × s15Fixed16）
fn icc_xyz_tag(profile: &[u8], sig: &[u8; 4]) -> Option<[f64; 3]> {
    let data = icc_tag_bytes(profile, sig)?;
    if data.len() < 16 || &data[0..4] != b"XYZ " {
        return None;
    }
    let mut out = [0f64; 3];
    for (i, v) in out.iter_mut().enumerate() {
        let raw = i32::from_be_bytes(data[4 + i * 4..8 + i * 4].try_into().ok()?);
        *v = raw as f64 / 65536.0;
    }
    Some(out)
}

/// 读取 TRC 标签（curv / para）。LUT 型（mft*、mAB）返回 None。
fn icc_trc_tag(profile: &[u8], sig: &[u8; 4]) -> Option<ToneCurve> {
    let data = icc_tag_bytes(profile, sig)?;
    let kind = data.get(0..4)?;
    let sf = |i: usize| -> Option<f64> {
        let raw = i32::from_be_bytes(data.get(i..i + 4)?.try_into().ok()?);
        Some(raw as f64 / 65536.0)
    };
    if kind == b"curv" {
        let count = u32::from_be_bytes(data.get(8..12)?.try_into().ok()?) as usize;
        return match count {
            0 => Some(ToneCurve::Gamma(1.0)),
            1 => {
                let g = u16::from_be_bytes(data.get(12..14)?.try_into().ok()?);
                Some(ToneCurve::Gamma(g as f64 / 256.0))
            }
            n if n <= 4096 => {
                let mut table = Vec::with_capacity(n);
                for i in 0..n {
                    let b = data.get(12 + i * 2..14 + i * 2)?;
                    table.push(u16::from_be_bytes(b.try_into().ok()?));
                }
                Some(ToneCurve::Table(table))
            }
            _ => None,
        };
    }
    if kind == b"para" {
        // parametricCurveType: type(4) + functionType(2) + reserved(2) + parameters(7×s15Fixed16)
        let func = u16::from_be_bytes(data.get(4..6)?.try_into().ok()?) as u8;
        if func > 4 {
            return None;
        }
        // 参数个数随 funcType 变化（1/3/4/5/7），未给出的按 ICC 默认值补齐
        let g = sf(8)?;
        let a = sf(12).unwrap_or(1.0);
        let b = sf(16).unwrap_or(0.0);
        let c = sf(20).unwrap_or(0.0);
        let d = sf(24).unwrap_or(0.0);
        let e = sf(28).unwrap_or(0.0);
        let f = sf(32).unwrap_or(0.0);
        return Some(ToneCurve::Parametric {
            func,
            g,
            a,
            b,
            c,
            d,
            e,
            f,
        });
    }
    None
}

/// 按签名取 tag 数据段。
fn icc_tag_bytes<'a>(profile: &'a [u8], sig: &[u8; 4]) -> Option<&'a [u8]> {
    let count = u32::from_be_bytes(profile.get(128..132)?.try_into().ok()?) as usize;
    if count == 0 || count > 4096 {
        return None;
    }
    for i in 0..count {
        let base = 132 + i * 12;
        let entry = profile.get(base..base + 12)?;
        if &entry[0..4] != sig {
            continue;
        }
        let off = u32::from_be_bytes(entry[4..8].try_into().ok()?) as usize;
        let size = u32::from_be_bytes(entry[8..12].try_into().ok()?) as usize;
        return profile.get(off..off.checked_add(size)?);
    }
    None
}

/// 解析 matrix-shaper RGB profile；非 RGB / 非 XYZ PCS / LUT 型返回 None。
fn parse_rgb_matrix_profile(profile: &[u8]) -> Option<RgbMatrixProfile> {
    if !is_icc_profile(profile) || icc_components(profile) != 3 {
        return None;
    }
    // PCS 必须是 XYZ，否则是 LUT 型 profile，本实现不处理
    if profile.get(20..24)? != b"XYZ " {
        return None;
    }
    let r = icc_xyz_tag(profile, b"rXYZ")?;
    let g = icc_xyz_tag(profile, b"gXYZ")?;
    let b = icc_xyz_tag(profile, b"bXYZ")?;
    let linear_to_xyz = [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]];
    let trc = [
        icc_trc_tag(profile, b"rTRC")?,
        icc_trc_tag(profile, b"gTRC")?,
        icc_trc_tag(profile, b"bTRC")?,
    ];
    Some(RgbMatrixProfile { linear_to_xyz, trc })
}

/// sRGB 的线性 RGB → XYZ(D50) 矩阵（Bradford 适应 D65→D50）。
fn srgb_linear_to_xyz_d50() -> &'static [[f64; 3]; 3] {
    static M: OnceLock<[[f64; 3]; 3]> = OnceLock::new();
    M.get_or_init(|| {
        let m_d65 = rgb_to_xyz_matrix(
            0.6400, 0.3300, 0.3000, 0.6000, 0.1500, 0.0600, 0.3127, 0.3290,
        );
        let w65 = [0.3127 / 0.3290, 1.0, (1.0 - 0.3127 - 0.3290) / 0.3290];
        let w50 = [0.9642, 1.0, 0.8249];
        bradford_adapt(&m_d65, &w65, &w50)
    })
}

/// 线性 sRGB → 8-bit sRGB 的编码 LUT（4096 项，误差 < 1 个量化级）。
fn srgb_encode_lut() -> &'static [u8; 4096] {
    static LUT: OnceLock<[u8; 4096]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = [0u8; 4096];
        for (i, slot) in lut.iter_mut().enumerate() {
            let x = i as f64 / 4095.0;
            let v = if x <= 0.003_130_8 {
                12.92 * x
            } else {
                1.055 * x.powf(1.0 / 2.4) - 0.055
            };
            *slot = (v * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        lut
    })
}

/// 把一张 RGB 图从源 profile 转换到 sRGB（原地修改）。
/// 返回 true 表示确实做了转换（调用方据此决定是否还内嵌原 profile）。
/// 无 profile / profile 不可解析时不改动像素并返回 false。
fn convert_rgb_to_srgb(rgb: &mut RgbImage, icc: Option<&[u8]>) -> bool {
    let Some(profile) = icc else {
        return false; // 无 ICC：本来就按 sRGB 解释
    };
    let Some(src) = parse_rgb_matrix_profile(profile) else {
        return false; // LUT 型或损坏 profile：保守按 sRGB 处理
    };
    let luts = [
        src.trc[0].to_lut(),
        src.trc[1].to_lut(),
        src.trc[2].to_lut(),
    ];
    // 线性源 RGB → 线性 sRGB = (M_srgb)^-1 · M_src
    let m = mat3_mul(&mat3_inv(srgb_linear_to_xyz_d50()), &src.linear_to_xyz);
    let encode = srgb_encode_lut();
    let encode_at = |v: f64| -> u8 {
        let clamped = v.clamp(0.0, 1.0);
        encode[(clamped * 4095.0).round() as usize]
    };

    rgb.pixels_mut().for_each(|pixel| {
        let r = luts[0][pixel.0[0] as usize] as f64;
        let g = luts[1][pixel.0[1] as usize] as f64;
        let b = luts[2][pixel.0[2] as usize] as f64;
        pixel.0 = [
            encode_at(m[0][0] * r + m[0][1] * g + m[0][2] * b),
            encode_at(m[1][0] * r + m[1][1] * g + m[1][2] * b),
            encode_at(m[2][0] * r + m[2][1] * g + m[2][2] * b),
        ];
    });
    true
}

/// Adobe APP14 marker，明确声明 ColorTransform=1（YCbCr → RGB）。
///
/// 16 字节布局：
///   FF EE | 00 0E | "Adobe" | 00 64 | 00 00 | 00 00 | 01
///   marker  length   id        ver     flags0  flags1   color_transform
///
/// 用途：image crate 编出的 JPEG 是 YCbCr 数据但只带 JFIF APP0，没有 APP14。
/// printpdf 0.7 在外面又硬塞 `DecodeParms.ColorTransform=0` 告诉 PDF 阅读器
/// "数据已是 RGB，不要变换"。两者矛盾时严格按规范的阅读器会把 YCbCr 当 RGB 渲染，
/// 表现为暗青色 / 偏色 / 大面积接近黑。注入 APP14 后，APP14 的优先级最高，
/// 强制 PDF 阅读器走正确的 YCbCr→RGB 变换。
const APP14_ADOBE_YCBCR: [u8; 16] = [
    0xFF, 0xEE, // APP14 marker
    0x00, 0x0E, // segment length = 14
    b'A', b'd', b'o', b'b', b'e', // identifier "Adobe"
    0x00, 0x64, // DCTEncodeVersion = 100
    0x00, 0x00, // APP14Flags0
    0x00, 0x00, // APP14Flags1
    0x01, // ColorTransform = 1 (YCbCr)
];

/// 在 JPEG 字节流的 APP segment 区域末尾插入 Adobe APP14（YCbCr）。
///
/// - 不是 JPEG（首两字节非 FFD8）/ 流损坏 → 不动，返回 false
/// - 已存在 APP14（FFEE）→ 不重复插入，返回 false
/// - 否则把 16 字节插在最后一个 APP marker 之后（DQT/DHT/SOF 之前）
fn inject_app14_ycbcr(jpeg: &mut Vec<u8>) -> bool {
    if jpeg.len() < 4 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return false;
    }
    let mut i = 2usize;
    while i + 3 < jpeg.len() {
        if jpeg[i] != 0xFF {
            return false; // marker 流被破坏，保守不动
        }
        let marker = jpeg[i + 1];
        if marker == 0xEE {
            return false; // 已经有 APP14（Photoshop / Acrobat 导出的 JPEG 自带）
        }
        if !(0xE0..=0xEF).contains(&marker) {
            break; // 走出 APP0..APP15 区域，到了 DQT/DHT/SOF/SOS
        }
        let seg_len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
        if seg_len < 2 || i + 2 + seg_len > jpeg.len() {
            return false;
        }
        i += 2 + seg_len;
    }
    jpeg.splice(i..i, APP14_ADOBE_YCBCR.iter().copied());
    true
}

fn convert_tiff_plane_u8(
    data: &[u8],
    color: tiff::ColorType,
    w: u32,
    h: u32,
) -> Result<(RgbImage, Option<Vec<u8>>), String> {
    match color {
        tiff::ColorType::Gray(8) => {
            let mut rgb = Vec::with_capacity(data.len() * 3);
            for v in data {
                rgb.extend_from_slice(&[*v, *v, *v]);
            }
            let img = image_crate::RgbImage::from_raw(w, h, rgb).ok_or("生成灰度 RGB 失败")?;
            Ok((img, None))
        }
        tiff::ColorType::RGB(8) => {
            let img =
                image_crate::RgbImage::from_raw(w, h, data.to_vec()).ok_or("生成 RGB 失败")?;
            Ok((img, None))
        }
        tiff::ColorType::RGBA(8) => {
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            let mut alpha = Vec::with_capacity((w * h) as usize);
            for chunk in data.chunks_exact(4) {
                rgb.extend_from_slice(&chunk[0..3]);
                alpha.push(chunk[3]);
            }
            let img = image_crate::RgbImage::from_raw(w, h, rgb).ok_or("生成 RGBA RGB 失败")?;
            Ok((img, Some(alpha)))
        }
        _ => Err("不支持的 TIFF 颜色类型".into()),
    }
}

// ==================== 处理参数 / passthrough ====================

#[derive(Debug, Clone, Copy)]
struct ProcessParams {
    /// 对原始 JPEG 是否直接 passthrough（零重编码）
    passthrough_jpeg: bool,
    /// 非 JPEG 图像是否强制以 FlateDecode 无损嵌入（不做 JPEG 有损重编码）
    force_flate: bool,
    /// JPEG 编码质量（仅在需要重编码时使用）
    quality: u8,
    /// 像素缩放比例（1.0 = 原尺寸；< 1.0 = 等比缩小）
    scale: f32,
}

impl ProcessParams {
    fn lossless(quality: u8, passthrough: bool) -> Self {
        Self {
            passthrough_jpeg: passthrough,
            force_flate: false,
            quality,
            scale: 1.0,
        }
    }

    /// 原样模式：JPEG 直传，其他格式 Flate 无损，不缩放、不有损重编码。
    fn original() -> Self {
        Self {
            passthrough_jpeg: true,
            force_flate: true,
            quality: 95,
            scale: 1.0,
        }
    }

    fn portable(target_ratio: f32) -> Self {
        // 经验映射：target_ratio ∈ [0.10, 1.00]
        // 100% → q=95, scale=1.00
        //  10% → q=40, scale=0.35
        let r = target_ratio.clamp(0.10, 1.00);
        let t = (r - 0.10) / 0.90; // 0..1
        let quality = (40.0 + t * 55.0).round() as u8; // 40..95
        let scale = 0.35 + t * 0.65; // 0.35..1.00
        Self {
            passthrough_jpeg: false,
            force_flate: false,
            quality,
            scale,
        }
    }

    fn label(&self) -> String {
        if self.force_flate {
            "原样 (JPEG 直传 / 其他 Flate 无损)".to_string()
        } else if self.passthrough_jpeg && (self.scale - 1.0).abs() < 1e-3 {
            format!("无损 (JPEG 直传 / 其他 q={})", self.quality)
        } else if (self.scale - 1.0).abs() < 1e-3 {
            format!("重编码 q={}", self.quality)
        } else {
            format!("重编码 q={} / {:.0}%", self.quality, self.scale * 100.0)
        }
    }
}

/// 图像编码方式：DCT (JPEG 有损) 或 Flate (deflate 无损)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageEncoding {
    /// JPEG DCTDecode — 适合照片类内容
    Dct,
    /// FlateDecode (deflate) — 适合线条艺术 / 索引色 / 文字类 PNG
    Flate,
}

/// 已处理待嵌入的图像数据
struct ProcessedImage {
    /// 编码后的图像字节流
    /// - Dct: JPEG 字节（写入 PDF 时直接作为 DCTDecode 流）
    /// - Flate: 原始 RGB 像素行（写入 PDF 时以 zlib 压缩为 FlateDecode 流）
    data: Vec<u8>,
    width: u32,
    height: u32,
    encoding: ImageEncoding,
    /// Alpha 通道数据（DeviceGray，8-bit），用于 PDF SMask。
    /// Some(alpha_bytes) → PDF 中 RGB 作为主图像，Alpha 作为 SMask 子图像。
    /// None → 图像无透明通道，直接嵌入 RGB。
    alpha_data: Option<Vec<u8>>,
    /// 源图片内嵌的 ICC 配置原始字节（PNG iCCP / JPEG APP2），
    /// 供 PDF ICCBased 色彩空间嵌入。None → 无 ICC 或不支持提取。
    icc_profile_bytes: Option<Vec<u8>>,
}

/// 将 RGBA 图像分离为 RGB + Alpha 两个通道。
/// RGB 走主图像路径（DCT 或 Flate），Alpha 走 SMask 路径（DeviceGray + FlateDecode）。
fn split_rgba_to_rgb_alpha(rgba: &RgbaImage) -> (RgbImage, Vec<u8>) {
    let (w, h) = rgba.dimensions();
    let mut rgb = RgbImage::new(w, h);
    let mut alpha = Vec::with_capacity((w * h) as usize);
    for (rgb_pixel, rgba_pixel) in rgb.pixels_mut().zip(rgba.pixels()) {
        rgb_pixel.0 = [rgba_pixel.0[0], rgba_pixel.0[1], rgba_pixel.0[2]];
        alpha.push(rgba_pixel.0[3]);
    }
    (rgb, alpha)
}

struct PreparedPage {
    processed: ProcessedImage,
    layout: ImageLayout,
    page_width: f64,
    page_height: f64,
}

/// 直接读取 JPEG 原始字节，附带原图尺寸（不解码像素）。
/// 同样会注入 Adobe APP14（YCbCr），以抵消 printpdf 的 ColorTransform=0 写死。
fn jpeg_passthrough(path: &Path, assume_adobe_rgb: bool) -> Result<ProcessedImage, String> {
    let mut file = File::open(path).map_err(|e| format!("无法打开 JPEG: {}", e))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| format!("读取 JPEG 失败: {}", e))?;
    let (w, h) =
        image_crate::image_dimensions(path).map_err(|e| format!("读取 JPEG 尺寸失败: {}", e))?;

    // JPEG 完整性检测：必须有 SOI + EOI
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return Err("JPEG 文件头损坏（缺少 SOI 标记）".to_string());
    }
    // 查找 EOI (FF D9)：从末尾向前搜索，允许尾部填充
    let has_eoi = {
        let mut found = false;
        let mut i = data.len();
        while i >= 2 {
            i -= 2;
            if data[i] == 0xFF && data[i + 1] == 0xD9 {
                found = true;
                break;
            }
            // 跳过尾部填充（0x00 或空白字符）
            if data[i] == 0x00 || data[i] == 0x0A || data[i] == 0x0D || data[i] == 0x20 {
                continue;
            }
            break;
        }
        found
    };
    // ponytail: 缺少 EOI 但能读出尺寸 → 大多数图片查看器能正常显示，追加 EOI 即可合并进 PDF
    if !has_eoi {
        data.extend_from_slice(&[0xFF, 0xD9]);
    }

    inject_app14_ycbcr(&mut data);
    // 从内存中的 JPEG 字节提取 ICC（避免二次读盘）；无内嵌但 EXIF 假定 Adobe RGB 时用内置 profile
    let mut icc = extract_jpeg_icc_from_bytes(&data);
    if icc.is_none() && assume_adobe_rgb {
        icc = Some(adobe_rgb_icc_profile().clone());
    }
    Ok(ProcessedImage {
        data,
        width: w,
        height: h,
        encoding: ImageEncoding::Dct,
        alpha_data: None,
        icc_profile_bytes: icc,
    })
}

/// 统计 TIFF 文件的帧数（多页 TIFF 的页数）。
/// 仅遍历 IFD 链（more_images/next_image），不解码像素，开销很小。
/// 解析失败时返回 1（按单页处理），保证分析阶段不会因 TIFF 损坏而中断。
fn count_tiff_frames(path: &Path) -> usize {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return 1,
    };
    let mut decoder = match TiffDecoder::new(file) {
        Ok(d) => d,
        Err(_) => return 1,
    };
    let mut count = 1usize;
    loop {
        if !decoder.more_images() {
            break;
        }
        if decoder.next_image().is_err() {
            break;
        }
        count += 1;
    }
    count
}

/// 处理多页 TIFF：每页单独压成 JPEG
fn handle_multipage_tiff(
    path: &Path,
    config: &PdfConfig,
    params: &ProcessParams,
) -> Result<Vec<PreparedPage>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut decoder = TiffDecoder::new(file).map_err(|e| e.to_string())?;
    let mut pages = Vec::new();
    let validated_margin =
        validate_margin_for_mode(config.margin, &config.page_mode, config.page_size)?;
    let effective_margin = match &config.page_mode {
        PageMode::Original => 0.0,
        PageMode::Fixed { .. } => validated_margin,
    };
    // EXIF 方向对整个 TIFF 文件生效，读一次后在每帧复用（参考 process_image 的 apply_exif_orientation）
    let exif_orientation = read_exif_orientation(path);

    loop {
        let color = decoder.colortype().unwrap_or(tiff::ColorType::RGB(8));
        let (w, h) = decoder.dimensions().unwrap_or((0, 0));
        let decoded = match decoder.read_image() {
            Ok(v) => v,
            Err(tiff::TiffError::IoError(_)) => break,
            Err(e) => return Err(e.to_string()),
        };

        let (rgb_image, alpha) = match decoded {
            TiffDecodingResult::U8(data) => convert_tiff_plane_u8(&data, color, w, h)?,
            _ => return Err("暂不支持的 TIFF 位深".into()),
        };

        let rgb_image = if let Some(alpha_data) = alpha {
            let mut rgba = RgbaImage::new(w, h);
            for ((pixel, rgb), a) in rgba
                .pixels_mut()
                .zip(rgb_image.pixels())
                .zip(alpha_data.iter())
            {
                pixel.0 = [rgb.0[0], rgb.0[1], rgb.0[2], *a];
            }
            DynamicImage::ImageRgba8(rgba)
        } else {
            DynamicImage::ImageRgb8(rgb_image)
        };
        // 应用 EXIF 方向（与 analyze_image/缩略图保持一致）；旋转后尺寸自动反映到 processed/layout
        let mut oriented = apply_orientation_value(rgb_image, exif_orientation);
        if (params.scale - 1.0).abs() > 1e-3 {
            oriented = resize_dynamic(&oriented, params.scale);
        }
        // 与单图路径保持一致：真 alpha 走 SMask 保留透明，而不是拍平到白底
        let (rgb, alpha_data) = if has_alpha_channel(&oriented) {
            let (r, a) = split_rgba_to_rgb_alpha(&oriented.to_rgba8());
            (r, Some(a))
        } else {
            (oriented.to_rgb8(), None)
        };

        let mut processed = if params.force_flate {
            encode_rgb_to_flate_processed(&rgb)
        } else {
            encode_rgb_to_jpeg_processed(&rgb, params.quality)?
        };
        processed.alpha_data = alpha_data;
        let (page_width, page_height) = get_page_dimensions(
            &config.page_mode,
            config.page_size,
            processed.width,
            processed.height,
        );
        let layout = calculate_image_layout(
            processed.width,
            processed.height,
            page_width,
            page_height,
            effective_margin,
        );

        pages.push(PreparedPage {
            processed,
            layout,
            page_width,
            page_height,
        });

        if decoder.next_image().is_err() {
            break;
        }
    }

    if pages.is_empty() {
        Err("未能读取 TIFF 内容".into())
    } else {
        Ok(pages)
    }
}

fn resize_dynamic(img: &DynamicImage, scale: f32) -> DynamicImage {
    let (w, h) = img.dimensions();
    let new_w = ((w as f32 * scale) as u32).max(1);
    let new_h = ((h as f32 * scale) as u32).max(1);
    img.resize(new_w, new_h, FilterType::Triangle)
}

/// 处理单张图片：根据 params 决定是否走 JPEG passthrough；其余路径解码 → 旋转 → 缩放 → 编码
/// PNG 图片会进行内容分类：线条艺术走 FlateDecode（无损），照片走 DCTDecode（JPEG）
fn process_image(
    path: &Path,
    config: &PdfConfig,
    params: &ProcessParams,
    has_icc: bool,
    assume_adobe_rgb: bool,
    color_plan: &ColorPlan,
) -> Result<Vec<PreparedPage>, String> {
    // ---- 内存预检：读取尺寸但不解码，超大图直接拒绝 ----
    let (raw_w, raw_h) = match image_crate::image_dimensions(path) {
        Ok((w, h)) => (w, h),
        Err(e) => return Err(format!("无法读取图片尺寸 {}: {}", path.display(), e)),
    };
    // EXIF 方向只读一次：整个流程要用三处（算尺寸、判能否直传、真正旋转），
    // 每处各读一遍文件会白白多开 2~3 次盘。
    let exif_orientation = read_exif_orientation(path);
    let (w, h) = match exif_orientation {
        Some(5..=8) => (raw_h, raw_w),
        _ => (raw_w, raw_h),
    };
    match check_image_memory(w, h) {
        MemoryCheck::Oversized { estimated_mb, .. } => {
            return Err(format!(
                "图片过大（约 {}MB，{}×{}像素），建议压缩后再试",
                estimated_mb, w, h
            ));
        }
        MemoryCheck::Ok => {}
    }

    let guessed_format = ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .map(|r| r.format().unwrap_or(ImageFormat::Png))
        .unwrap_or(ImageFormat::Png);

    // 多页 TIFF 单独处理（不能 passthrough）
    if matches!(guessed_format, ImageFormat::Tiff) {
        return handle_multipage_tiff(path, config, params);
    }

    let validated_margin =
        validate_margin_for_mode(config.margin, &config.page_mode, config.page_size)?;
    let effective_margin = match &config.page_mode {
        PageMode::Original => 0.0,
        PageMode::Fixed { .. } => validated_margin,
    };

    let is_jpeg = matches!(guessed_format, ImageFormat::Jpeg);
    // CMYK 判定放在解码之前：既避免「先按 RGB 解一遍再丢弃」的重复解码，
    // 也让 CMYK→RGB 转换发生在 EXIF 旋转之前，不会用旋转后的宽高重新解释原始像素序。
    let is_cmyk = is_jpeg && jpeg_is_cmyk(path);

    // ---- JPEG passthrough 路径 ----
    // 兼容模式要把像素真正转到 sRGB，因此不能直传原始字节
    if is_jpeg
        && !is_cmyk
        && params.passthrough_jpeg
        && (params.scale - 1.0).abs() < 1e-3
        && color_plan.allows_passthrough()
        && exif_orientation.is_none()
    {
        let processed = jpeg_passthrough(path, assume_adobe_rgb)?;
        let (page_width, page_height) = get_page_dimensions(
            &config.page_mode,
            config.page_size,
            processed.width,
            processed.height,
        );
        let layout = calculate_image_layout(
            processed.width,
            processed.height,
            page_width,
            page_height,
            effective_margin,
        );
        return Ok(vec![PreparedPage {
            processed,
            layout,
            page_width,
            page_height,
        }]);
    }

    // ---- 通用路径：解码（CMYK 先行）→ EXIF 方向 → 缩放 → 色彩转换 → 透明分离 → 编码 ----
    let (mut img, source_has_alpha) = if is_cmyk {
        (
            DynamicImage::ImageRgb8(convert_cmyk_to_rgb_image(path)?),
            false,
        )
    } else {
        let decoded = decode_image_fast(path, guessed_format)?;
        // 注意：必须在 EXIF 旋转前记录，旋转会把图像转成 RGBA，
        // 否则纯 JPEG 照片会被误判成「有透明通道」而多带一个全不透明的 SMask。
        let alpha = has_alpha_channel(&decoded);
        (decoded, alpha)
    };
    img = apply_orientation_value(img, exif_orientation);

    if (params.scale - 1.0).abs() > 1e-3 {
        img = resize_dynamic(&img, params.scale);
    }

    let (w, h) = img.dimensions();
    let (rgb, alpha_data) = if source_has_alpha && has_alpha_channel(&img) {
        let (r, a) = split_rgba_to_rgb_alpha(&img.to_rgba8());
        (r, Some(a))
    } else {
        (img.to_rgb8(), None)
    };

    // ---- 兼容模式：把非 sRGB 的像素真正转换到 sRGB ----
    let mut rgb = rgb;
    let mut icc_bytes = if has_icc {
        extract_icc_for_image(path, guessed_format)
    } else {
        None
    };
    if icc_bytes.is_none() && assume_adobe_rgb {
        // 无内嵌 ICC 但 EXIF 标记 Adobe RGB：按内置 profile 处理
        icc_bytes = Some(adobe_rgb_icc_profile().clone());
    }
    let converted_to_srgb = match color_plan {
        ColorPlan::ToSrgb { unconvertible } => {
            let converted = convert_rgb_to_srgb(&mut rgb, icc_bytes.as_deref());
            // 有 profile 却转不动（LUT 型/损坏）→ 计数并在结果里提示
            if !converted && icc_bytes.is_some() {
                unconvertible.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            converted
        }
        ColorPlan::Passthrough => false,
    };

    // ---- 编码分流 ----
    // 原样模式：所有非直传 JPEG 一律 Flate 无损嵌入，绝不做有损重编码。
    // 其他模式：仅 PNG 做内容自适应分类（线条艺术 Flate，照片 DCT），其余格式走 JPEG。
    let mut processed = if params.force_flate {
        encode_rgb_to_flate_processed(&rgb)
    } else if matches!(guessed_format, ImageFormat::Png) {
        match classify_png_content(&rgb) {
            ImageEncoding::Flate => {
                // 线条艺术 / 索引色内容：FlateDecode 无损，与 JPEG 比体积取小者。
                // 两者都是「编码后」字节数，比较才成立（Flate 侧在编码函数里已 zlib 压缩）。
                let flate_processed = encode_rgb_to_flate_processed(&rgb);
                match encode_rgb_to_jpeg_processed(&rgb, params.quality) {
                    Ok(jpeg_processed) => {
                        if jpeg_processed.data.len() < flate_processed.data.len() {
                            jpeg_processed
                        } else {
                            flate_processed
                        }
                    }
                    Err(_) => flate_processed,
                }
            }
            ImageEncoding::Dct => encode_rgb_to_jpeg_processed(&rgb, params.quality)?,
        }
    } else {
        encode_rgb_to_jpeg_processed(&rgb, params.quality)?
    };
    // 附加 Alpha 数据到 ProcessedImage（SMask 会在 write_pdf 时写入 PDF）
    processed.alpha_data = alpha_data;
    // 兼容模式下像素已是 sRGB，再内嵌原 profile 反而会让查看器重复转换 → 不内嵌
    if converted_to_srgb {
        processed.icc_profile_bytes = None;
    } else {
        processed.icc_profile_bytes = icc_bytes;
    }

    let (page_width, page_height) = get_page_dimensions(&config.page_mode, config.page_size, w, h);
    let layout = calculate_image_layout(w, h, page_width, page_height, effective_margin);

    Ok(vec![PreparedPage {
        processed,
        layout,
        page_width,
        page_height,
    }])
}

// ==================== PDF 写入 ====================

/// 对一组图片应用一组参数，返回 (按图片顺序排好的 prepared pages, sum of processed bytes)
type ProcessAllResult = (Vec<Vec<PreparedPage>>, u64, Vec<String>);

/// 色彩处理方案（由 PdfConfig 派生，贯穿整批处理）
#[derive(Debug)]
enum ColorPlan {
    /// 保真：不改动像素，JPEG 可以直传
    Passthrough,
    /// 兼容：把像素转换到 sRGB；`unconvertible` 统计无法解析的 profile 数（LUT 型）
    ToSrgb {
        unconvertible: std::sync::atomic::AtomicUsize,
    },
}

impl ColorPlan {
    fn from_config(config: &PdfConfig) -> Self {
        match config.color_mode {
            ColorMode::Preserve => ColorPlan::Passthrough,
            ColorMode::ToSrgb => ColorPlan::ToSrgb {
                unconvertible: std::sync::atomic::AtomicUsize::new(0),
            },
        }
    }

    /// 兼容模式下是否允许 JPEG 直传（转换必须重编码，不能直传）
    fn allows_passthrough(&self) -> bool {
        matches!(self, ColorPlan::Passthrough)
    }

    fn unconvertible_count(&self) -> usize {
        match self {
            ColorPlan::ToSrgb { unconvertible } => {
                unconvertible.load(std::sync::atomic::Ordering::Relaxed)
            }
            ColorPlan::Passthrough => 0,
        }
    }

    fn reset(&self) {
        if let ColorPlan::ToSrgb { unconvertible } = self {
            unconvertible.store(0, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

fn process_all(
    images: &[ImageAnalysis],
    config: &PdfConfig,
    params: &ProcessParams,
    window: &tauri::WebviewWindow,
    phase_label: &str,
    color_plan: &ColorPlan,
) -> Result<ProcessAllResult, String> {
    use std::sync::Mutex;
    let total = images.len();
    let counter = std::sync::atomic::AtomicUsize::new(0);
    let results: Mutex<Vec<Option<Vec<PreparedPage>>>> =
        Mutex::new(std::iter::repeat_with(|| None).take(total).collect());
    let warnings: Mutex<Vec<String>> = Mutex::new(Vec::new());

    let outcomes: Vec<Result<(), String>> = images
        .par_iter()
        .enumerate()
        .map(|(idx, img)| {
            // 大图并发限制（阻塞等待，不空转）
            let _slot = if img.width as u64 * img.height as u64 * 4 > 50 * 1024 * 1024 {
                Some(LargeDecodeGate::global().acquire())
            } else {
                None
            };

            let result = process_image(
                Path::new(&img.path),
                config,
                params,
                img.has_icc,
                assume_adobe_rgb(img),
                color_plan,
            );

            let cur = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let _ = window.emit(
                "pdf_progress",
                GenerationProgress {
                    current: cur,
                    total,
                    current_file: img.path.clone(),
                    phase: phase_label.to_string(),
                },
            );

            match result {
                Ok(pages) => {
                    results.lock().unwrap()[idx] = Some(pages);
                    Ok(())
                }
                Err(e) => {
                    // 超大图等非致命错误 → 跳过该图但继续处理其他
                    warnings
                        .lock()
                        .unwrap()
                        .push(format!("{}: {}", img.path, e));
                    Ok(())
                }
            }
        })
        .collect();

    // outcomes 全是 Ok(())，因为错误已转到 warnings
    for r in outcomes {
        r?;
    }

    let collected: Vec<Vec<PreparedPage>> = results
        .into_inner()
        .unwrap()
        .into_iter()
        .flatten()
        .collect();
    let processed_bytes: u64 = collected
        .iter()
        .flat_map(|v| v.iter())
        .map(|p| p.processed.data.len() as u64)
        .sum();
    let warnings = warnings.into_inner().unwrap();
    Ok((collected, processed_bytes, warnings))
}

fn sibling_work_path(output_path: &Path, marker: &str) -> PathBuf {
    let name = output_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("output.pdf");
    output_path.with_file_name(format!(".{name}.{marker}-{}", uuid::Uuid::new_v4()))
}

fn commit_output_file(temp_path: &Path, output_path: &Path) -> Result<(), String> {
    if !output_path.exists() {
        return std::fs::rename(temp_path, output_path).map_err(|error| {
            let _ = std::fs::remove_file(temp_path);
            format!("提交 PDF 失败: {error}")
        });
    }

    let backup_path = sibling_work_path(output_path, "backup");
    if let Err(error) = std::fs::rename(output_path, &backup_path) {
        let _ = std::fs::remove_file(temp_path);
        return Err(format!("备份原 PDF 失败: {error}"));
    }

    match std::fs::rename(temp_path, output_path) {
        Ok(()) => {
            let _ = std::fs::remove_file(backup_path);
            Ok(())
        }
        Err(error) => {
            let restore = std::fs::rename(&backup_path, output_path);
            let _ = std::fs::remove_file(temp_path);
            match restore {
                Ok(()) => Err(format!("提交 PDF 失败，原文件已恢复: {error}")),
                Err(restore_error) => Err(format!(
                    "提交 PDF 失败且无法恢复原文件；备份保留在 {}: {error}; {restore_error}",
                    backup_path.display()
                )),
            }
        }
    }
}

// ==================== 流式 PDF 写出（lopdf） ====================
//
// 背景：printpdf 0.7 的 save() 会先把整份 PDF 序列化进内存 Vec（save_to_bytes），
// 大文件（如 896 张 / 4.32GB）时峰值内存 ≈ 原图数据 ×3，且期间没有任何进度事件，
// 表现为卡在 100%。这里改用 lopdf 的 save_to()（逐对象写入 writer、即时算 xref 偏移），
// 图像字节 move 进流对象（不复制），并用带字节计数器的 writer 在后台线程写出，
// 主线程按已写字节数发射真实进度。

/// 数值 → 紧凑 PDF 数字字符串（去掉多余的小数尾零）
fn fmt_pt(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// 保留 3 位小数（MediaBox 等点位值无需更高精度）
fn rounded_pt(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// zlib 压缩（PDF FlateDecode 需要 zlib 包装的 deflate 流）
fn deflate_zlib(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = encoder.write_all(data);
    encoder.finish().expect("zlib 压缩失败")
}

/// 带字节计数器的写包装：把实际写入字节数累计到 AtomicU64，供进度轮询读取。
/// lopdf 的 save_to 是同步阻塞调用，无法在写的同时回调，因此用计数器 + 后台线程。
struct CountingWriter<'a, W: Write> {
    inner: &'a mut W,
    counter: &'a AtomicU64,
}

impl<'a, W: Write> Write for CountingWriter<'a, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.counter.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// 构建 PDF 文档（纯对象构建，不落盘、不发射进度）。
/// 图像字节 move 进流对象（不复制）。返回 (doc, 总页数, 写盘字节估算)。
fn build_document(
    pages_per_image: Vec<Vec<PreparedPage>>,
    embed_icc: bool,
) -> Result<(Document, usize, u64), String> {
    let total_pages: usize = pages_per_image.iter().map(|v| v.len()).sum();
    if total_pages == 0 {
        return Err("没有可写入的页面".to_string());
    }

    // ---- 构建文档（图像字节 move 进流对象，避免复制大缓冲）----
    let mut doc = Document::with_version("1.4");
    let mut page_ids: Vec<ObjectId> = Vec::with_capacity(total_pages);
    let mut icc_cache: HashMap<Vec<u8>, ObjectId> = HashMap::new();
    // 写盘字节估算：图像流 + alpha + ICC + 内容流的实际长度，另加每页 4KB 结构开销。
    // 估算略大于实际，保证进度不会提前卡在 100%。
    let mut estimated_total_bytes: u64 = 0;

    for prepared_pages in pages_per_image {
        for prepared in prepared_pages {
            let PreparedPage {
                processed,
                layout,
                page_width,
                page_height,
            } = prepared;
            let width_px = processed.width as i64;
            let height_px = processed.height as i64;
            let page_w_pt = rounded_pt(page_width * f64::from(MM_TO_PT));
            let page_h_pt = rounded_pt(page_height * f64::from(MM_TO_PT));

            // ICC 色彩空间：仅当 profile 有效且为 RGB 时以 ICCBased 嵌入，
            // 避免灰度/CMYK profile 与 RGB 像素数据错配。
            let mut color_space = Object::Name(b"DeviceRGB".to_vec());
            if embed_icc {
                if let Some(icc) = processed.icc_profile_bytes.as_ref() {
                    if icc_components(icc) == 3 {
                        // 先按切片查表：命中时不复制 profile，也不重复哈希整段数据
                        let icc_id = match icc_cache.get(icc.as_slice()) {
                            Some(id) => *id,
                            None => {
                                let compressed = deflate_zlib(icc);
                                estimated_total_bytes += compressed.len() as u64;
                                let dict = dictionary! {
                                    "N" => 3,
                                    "Alternate" => "DeviceRGB",
                                    "Filter" => "FlateDecode",
                                };
                                let id = doc.add_object(Object::Stream(
                                    Stream::new(dict, compressed).with_compression(false),
                                ));
                                icc_cache.insert(icc.clone(), id);
                                id
                            }
                        };
                        color_space = Object::Array(vec![
                            Object::Name(b"ICCBased".to_vec()),
                            Object::Reference(icc_id),
                        ]);
                    }
                }
            }

            // 主图像 XObject（两种编码的 data 都已是可直接写入的最终字节流）
            let (filter, data) = match processed.encoding {
                ImageEncoding::Dct => (Some("DCTDecode"), processed.data),
                ImageEncoding::Flate => (Some("FlateDecode"), processed.data),
            };
            estimated_total_bytes += data.len() as u64;

            let mut img_dict = dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => width_px,
                "Height" => height_px,
                "ColorSpace" => color_space,
                "BitsPerComponent" => 8,
                "Interpolate" => true,
            };
            if let Some(f) = filter {
                img_dict.set("Filter", f);
            }
            let mut img_stream = Stream::new(img_dict, data).with_compression(false);

            // 透明通道 → SMask（Flate 压缩的 DeviceGray 软掩码）
            if let Some(alpha) = processed.alpha_data {
                let alpha_compressed = deflate_zlib(&alpha);
                estimated_total_bytes += alpha_compressed.len() as u64;
                let smask_dict = dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => width_px,
                    "Height" => height_px,
                    "ColorSpace" => "DeviceGray",
                    "BitsPerComponent" => 8,
                    "Interpolate" => true,
                    "Filter" => "FlateDecode",
                };
                let smask_id = doc.add_object(Object::Stream(
                    Stream::new(smask_dict, alpha_compressed).with_compression(false),
                ));
                img_stream.dict.set("SMask", smask_id);
            }
            let img_id = doc.add_object(Object::Stream(img_stream));

            // 页面内容流：把 1×1 图像缩放到 scaled 尺寸并平移到 (x, y)
            let sx = layout.scaled_width * f64::from(MM_TO_PT) / processed.width as f64;
            let sy = layout.scaled_height * f64::from(MM_TO_PT) / processed.height as f64;
            let tx = layout.x * f64::from(MM_TO_PT);
            let ty = layout.y * f64::from(MM_TO_PT);
            let content = format!(
                "q {} 0 0 {} {} {} cm /Im0 Do Q",
                fmt_pt(sx),
                fmt_pt(sy),
                fmt_pt(tx),
                fmt_pt(ty)
            );
            estimated_total_bytes += content.len() as u64;
            let content_id = doc.add_object(Object::Stream(
                Stream::new(Dictionary::new(), content.into_bytes()).with_compression(false),
            ));

            let mut xobj = Dictionary::new();
            xobj.set("Im0", img_id);
            let resources = dictionary! { "XObject" => xobj };
            let page_dict = dictionary! {
                "Type" => "Page",
                "Resources" => resources,
                "MediaBox" => vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Real(page_w_pt),
                    Object::Real(page_h_pt),
                ],
                "Contents" => content_id,
            };
            let page_id = doc.add_object(Object::Dictionary(page_dict));
            page_ids.push(page_id);
        }
    }

    // ---- 页树 + 目录 + trailer ----
    let pages_id = doc.new_object_id();
    let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => page_ids.len() as i64,
        }),
    );
    for pid in &page_ids {
        if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(pid) {
            dict.set("Parent", pages_id);
        }
    }
    let catalog_id = doc.new_object_id();
    doc.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        }),
    );
    doc.trailer.set("Root", catalog_id);

    Ok((doc, total_pages, estimated_total_bytes))
}

fn write_pdf(
    pages_per_image: Vec<Vec<PreparedPage>>,
    output_path: &Path,
    window: &tauri::WebviewWindow,
    embed_icc: bool,
) -> Result<u64, String> {
    let (doc, total_pages, estimated_total_bytes) = build_document(pages_per_image, embed_icc)?;

    // ---- 流式写出（后台线程 + 字节进度）----
    let total_est = (estimated_total_bytes + total_pages as u64 * 4096) as usize;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp_path = sibling_work_path(output_path, "tmp");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);

    let counter = Arc::new(AtomicU64::new(0));
    let result_slot: Arc<Mutex<Option<Result<(), String>>>> = Arc::new(Mutex::new(None));
    let counter_thread = Arc::clone(&counter);
    let result_thread = Arc::clone(&result_slot);
    let mut doc_thread = doc;
    let handle = std::thread::spawn(move || {
        let mut counting = CountingWriter {
            inner: &mut writer,
            counter: counter_thread.as_ref(),
        };
        let save_result = doc_thread
            .save_to(&mut counting)
            .map_err(|e| format!("保存PDF失败: {}", e));
        // 写完后在后台线程内完成 flush + fsync，主线程再提交文件
        let flush_result = counting
            .flush()
            .and_then(|_| counting.inner.flush())
            .and_then(|_| counting.inner.get_ref().sync_all())
            .map_err(|e| format!("同步PDF失败: {}", e));
        let combined = match (save_result, flush_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(e), _) => Err(e),
            (Ok(()), Err(e)) => Err(e),
        };
        *result_thread.lock().unwrap() = Some(combined);
    });

    loop {
        if handle.is_finished() {
            break;
        }
        if result_slot.lock().unwrap().is_some() {
            break;
        }
        let written = counter.load(Ordering::Relaxed) as usize;
        let _ = window.emit(
            "pdf_progress",
            GenerationProgress {
                current: written,
                total: total_est,
                current_file: String::new(),
                phase: "正在写入文件".to_string(),
            },
        );
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
    let _ = handle.join();

    // 线程结束后的唯一结果取用点；panic 时 result 为空 → 兜底报错
    let final_res = result_slot.lock().unwrap().take();
    match final_res {
        Some(Ok(())) => {}
        Some(Err(e)) => {
            let _ = std::fs::remove_file(&temp_path);
            return Err(e);
        }
        None => {
            let _ = std::fs::remove_file(&temp_path);
            return Err("PDF 写入线程异常退出".to_string());
        }
    }

    // 保存完成 → 进度置 100%（结果将随后替换进度条）
    let _ = window.emit(
        "pdf_progress",
        GenerationProgress {
            current: total_est,
            total: total_est,
            current_file: String::new(),
            phase: "正在写入文件".to_string(),
        },
    );

    commit_output_file(&temp_path, output_path)?;

    Ok(std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0))
}

// ==================== Tauri Commands ====================

/// 目录分析结果：区分「目录里没有可用图片」与真实的分析结果。
/// 批量模式下前者是正常情况（某个子文件夹只有文档），不该当成错误吞掉或中断。
enum FolderScan {
    /// 目录里没有任何可处理的图片；`unsupported_exts` 是识别到但不支持的扩展名
    NoImages {
        unsupported_exts: Vec<String>,
    },
    Ready(FolderAnalysis),
}

/// 把 NoImages 转成用户可读的错误信息（单文件夹模式使用）
fn no_images_message(unsupported_exts: &[String]) -> String {
    if unsupported_exts.is_empty() {
        "文件夹中没有图片文件".to_string()
    } else {
        format!(
            "文件夹中只有不支持的格式（{}），请先转换为 JPG/PNG/WebP 等格式",
            unsupported_exts.join("、")
        )
    }
}

/// 扫描并分析一个文件夹中的全部图片（单命令与批量命令共用）。
/// 错误语义：目录不可读 → Err；目录可读但没有可用图片 → Ok(NoImages)；
/// 单图分析失败 → 计入 skipped，不影响其他图。
fn analyze_folder_impl(folder: &Path, recursive: bool) -> Result<FolderScan, String> {
    let scan = scan_folder(folder, recursive)?;
    if scan.images.is_empty() {
        let unsupported_exts: Vec<String> =
            scan.skipped.iter().map(|s| format!(".{}", s.ext)).collect();
        return Ok(FolderScan::NoImages { unsupported_exts });
    }

    let results: Vec<Result<ImageAnalysis, String>> =
        scan.images.par_iter().map(|p| analyze_image(p)).collect();

    let mut images = Vec::new();
    let mut skipped = scan.skipped;
    let mut portrait_count = 0usize;
    let mut landscape_count = 0usize;
    let mut square_count = 0usize;
    let mut total_size = 0u64;
    let mut apng_count = 0usize;
    let mut webp_animated_count = 0usize;
    let mut bit16_count = 0usize;
    let mut gamma_count = 0usize;
    let mut icc_count = 0usize;
    let mut icc_v4_count = 0usize;
    let mut bucket_counts: std::collections::BTreeMap<u32, (&'static str, usize)> =
        std::collections::BTreeMap::new();

    for (path, res) in scan.images.iter().zip(results) {
        match res {
            Ok(analysis) => {
                match classify_orientation(analysis.width, analysis.height) {
                    OrientationClass::Portrait => portrait_count += 1,
                    OrientationClass::Landscape => landscape_count += 1,
                    OrientationClass::Square => square_count += 1,
                }
                let (label, min_edge) = classify_resolution(analysis.width, analysis.height);
                let entry = bucket_counts.entry(min_edge).or_insert((label, 0));
                entry.1 += 1;

                // P1+P2: 统计 APNG、动画 WebP、16-bit、gamma、ICC 文件数
                if analysis.is_apng {
                    apng_count += 1;
                }
                if analysis.is_animated_webp {
                    webp_animated_count += 1;
                }
                if analysis.bit_depth > 8 {
                    bit16_count += 1;
                }
                if analysis.has_gamma {
                    gamma_count += 1;
                }
                if analysis.has_icc {
                    icc_count += 1;
                    if analysis.icc_version == Some(4) {
                        icc_v4_count += 1;
                    }
                }

                if let Ok(meta) = std::fs::metadata(path) {
                    total_size += meta.len();
                }
                images.push(analysis);
            }
            // 分析失败（损坏/截断等）不再静默丢弃：列入跳过列表并附原因
            Err(e) => {
                skipped.push(SkippedFile {
                    path: path.to_string_lossy().to_string(),
                    ext: path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("?")
                        .to_string(),
                    reason: format!("分析失败：{}", e),
                });
            }
        }
    }

    let long_image_count = images.iter().filter(|i| i.is_long_image).count();

    let suggested_orientation = if portrait_count > landscape_count {
        "portrait".to_string()
    } else if landscape_count > portrait_count {
        "landscape".to_string()
    } else {
        "auto".to_string()
    };

    let resolution_buckets: Vec<ResolutionBucket> = bucket_counts
        .into_iter()
        .map(|(min_edge, (label, count))| ResolutionBucket {
            label: label.to_string(),
            count,
            min_long_edge: min_edge,
        })
        .collect();

    Ok(FolderScan::Ready(FolderAnalysis {
        images,
        effective_path: folder.to_string_lossy().to_string(),
        recursive,
        auto_switched_from_file: false,
        portrait_count,
        landscape_count,
        square_count,
        suggested_orientation,
        total_size,
        resolution_buckets,
        skipped,
        apng_count,
        webp_animated_count,
        bit16_count,
        gamma_count,
        icc_count,
        icc_v4_count,
        long_image_count,
    }))
}

#[tauri::command]
pub async fn analyze_folder_for_pdf(
    folder_path: String,
    recursive: Option<bool>,
) -> Result<FolderAnalysis, String> {
    tokio::task::spawn_blocking(move || {
        let raw = PathBuf::from(&folder_path);
        let (folder, auto_switched) = resolve_input_path(&raw)?;
        let effective_recursive = if auto_switched {
            false
        } else {
            recursive.unwrap_or(false)
        };

        let mut analysis = match analyze_folder_impl(&folder, effective_recursive)? {
            FolderScan::Ready(analysis) => analysis,
            FolderScan::NoImages { unsupported_exts } => {
                return Err(no_images_message(&unsupported_exts))
            }
        };
        analysis.effective_path = folder.to_string_lossy().to_string();
        analysis.recursive = effective_recursive;
        analysis.auto_switched_from_file = auto_switched;
        Ok(analysis)
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

/// 批量模式：扫描一级子文件夹，每个含图片的子文件夹产生一个分析项。
/// 根目录自身第一层的图片也合并为一项（以根目录名命名）；子文件夹内部递归遍历。
#[tauri::command]
pub async fn analyze_subfolders_for_pdf(
    folder_path: String,
) -> Result<Vec<BatchItemAnalysis>, String> {
    tokio::task::spawn_blocking(move || {
        let raw = PathBuf::from(&folder_path);
        let (root, auto_switched) = resolve_input_path(&raw)?;
        let mut items: Vec<BatchItemAnalysis> = Vec::new();

        // 根目录自身第一层图片 → 合并为一份（以根目录名命名）
        if let FolderScan::Ready(mut analysis) = analyze_folder_impl(&root, false)? {
            analysis.auto_switched_from_file = auto_switched;
            let name = root
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("root")
                .to_string();
            items.push(BatchItemAnalysis {
                name,
                path: root.to_string_lossy().to_string(),
                analysis,
            });
        }

        // 一级子文件夹：内部递归遍历所有图片 → 各合并为一份
        let mut subdirs: Vec<PathBuf> = Vec::new();
        for entry in std::fs::read_dir(&root)
            .map_err(|e| format!("读取目录失败 {}: {}", root.display(), e))?
            .filter_map(Result::ok)
        {
            let p = entry.path();
            if p.is_dir() {
                subdirs.push(p);
            }
        }
        subdirs.sort_by(|a, b| natural_compare(a.as_os_str(), b.as_os_str()));
        for dir in &subdirs {
            // 子目录里没有图片是正常情况（跳过）；真正的 IO 错误照实上报，
            // 不再像以前那样 `if let Ok` 一口吞掉，导致某一份 PDF 无声消失。
            if let FolderScan::Ready(mut analysis) = analyze_folder_impl(dir, true)? {
                analysis.auto_switched_from_file = false;
                let name = dir
                    .file_name()
                    .and_then(OsStr::to_str)
                    .unwrap_or("folder")
                    .to_string();
                items.push(BatchItemAnalysis {
                    name,
                    path: dir.to_string_lossy().to_string(),
                    analysis,
                });
            }
        }

        if items.is_empty() {
            return Err("未找到包含图片的子文件夹".to_string());
        }
        Ok(items)
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
pub fn calculate_preview_layout(
    images: Vec<ImageAnalysis>,
    page_size: PageSize,
    page_mode: PageMode,
    margin: f64,
) -> Result<PreviewData, String> {
    let validated_margin = validate_margin_for_mode(margin, &page_mode, page_size)?;
    // 多页 TIFF 按帧数展开预览页，使预览页数与最终 PDF 页数一致。
    // 注：各帧共用 ImageAnalysis 中首帧的尺寸/缩略图；若多页 TIFF 各帧尺寸不同，
    // 预览的逐帧尺寸以首帧为准（仅页数准确），实际生成时按各帧真实尺寸布局。
    let base_pages: Vec<LayoutResult> = images
        .par_iter()
        .map(|img| calculate_page_layout(img, &page_mode, page_size, validated_margin))
        .collect();
    let pages: Vec<LayoutResult> = images
        .iter()
        .zip(base_pages)
        .flat_map(|(img, layout)| std::iter::repeat(layout).take(img.frame_count.max(1)))
        .collect();

    Ok(PreviewData {
        pages,
        page_size,
        page_mode,
        margin: validated_margin,
    })
}

/// 生成一份 PDF 的核心逻辑（单命令与批量命令共用）。
/// `phase_prefix` 用于批量模式下标识当前是第几份（如 "[2 / 8] "），单份模式传空串。
fn generate_pdf_core(
    images: Vec<ImageAnalysis>,
    config: PdfConfig,
    window: &tauri::WebviewWindow,
    phase_prefix: &str,
) -> Result<GenerationResult, String> {
    use std::time::Instant;
    let start = Instant::now();

    let total = images.len();
    if total == 0 {
        return Err("没有图片可以处理".into());
    }
    if config.output_path.is_empty() {
        return Err("未指定输出路径".into());
    }
    let original_total_size: u64 = images
        .iter()
        .filter_map(|img| std::fs::metadata(&img.path).ok())
        .map(|m| m.len())
        .sum();

    let validated_margin =
        validate_margin_for_mode(config.margin, &config.page_mode, config.page_size)?;
    let config = PdfConfig {
        margin: validated_margin,
        ..config
    };

    // 构建尝试链
    let attempts: Vec<ProcessParams> = match config.merge_mode {
        MergeMode::Original => vec![ProcessParams::original()],
        MergeMode::Lossless { .. } => vec![
            ProcessParams::lossless(95, true),
            ProcessParams::lossless(85, false),
            ProcessParams::lossless(75, false),
        ],
        MergeMode::Portable { target_ratio } => vec![ProcessParams::portable(target_ratio)],
    };

    let max_ratio = match config.merge_mode {
        MergeMode::Original => f32::MAX,
        MergeMode::Lossless { max_size_ratio } => max_size_ratio,
        MergeMode::Portable { .. } => f32::MAX, // portable 模式不强制阈值
    };

    let target_max_bytes = match config.merge_mode {
        MergeMode::Original => u64::MAX,
        _ => (original_total_size as f64 * max_ratio as f64) as u64,
    };

    let output_path = PathBuf::from(&config.output_path);
    let mut last_pages: Option<Vec<Vec<PreparedPage>>> = None;
    let mut last_params: Option<ProcessParams> = None;
    let mut all_warnings: Vec<String> = Vec::new();
    let color_plan = ColorPlan::from_config(&config);

    let _ = window.emit(
        "pdf_progress",
        GenerationProgress {
            current: 0,
            total,
            current_file: String::new(),
            phase: format!("{}处理图片", phase_prefix),
        },
    );

    let mut attempt_idx = 0usize;
    for params in &attempts {
        attempt_idx += 1;
        let phase = if attempts.len() > 1 {
            format!(
                "{}处理图片 ({}: {})",
                phase_prefix,
                attempt_idx,
                params.label()
            )
        } else {
            format!("{}处理图片 ({})", phase_prefix, params.label())
        };
        color_plan.reset();
        let (pages, processed_bytes, attempt_warnings) =
            process_all(&images, &config, params, window, &phase, &color_plan)?;

        // 降级重试时同一文件的失败会重复出现，这里按文件去重
        for w in attempt_warnings {
            if !all_warnings.iter().any(|existing| existing == &w) {
                all_warnings.push(w);
            }
        }

        // 仅 Lossless 模式才需要看阈值；超出则用下一档参数重试。
        // 重试时先丢弃本轮结果，否则两份完整图像数据会同时驻留内存（峰值翻倍）。
        if matches!(config.merge_mode, MergeMode::Lossless { .. })
            && processed_bytes > target_max_bytes
            && attempt_idx < attempts.len()
        {
            drop(pages);
            continue;
        }
        last_pages = Some(pages);
        last_params = Some(*params);
        break;
    }

    // 兼容模式下无法解析的 profile（LUT 型）只能按 sRGB 处理，如实告知
    let unconvertible = color_plan.unconvertible_count();
    if unconvertible > 0 {
        all_warnings.push(format!(
            "{} 张图片的 ICC 无法解析为矩阵型配置，兼容模式下按 sRGB 处理（可能偏色）",
            unconvertible
        ));
    }

    let pages = last_pages.ok_or_else(|| "处理失败：未生成任何页面".to_string())?;
    let params = last_params.expect("params set");
    let total_page_count: usize = pages.iter().map(|v| v.len()).sum();

    // write_pdf 消费 pages：图像字节 move 进 PDF 流对象，随写入逐步释放内存
    let file_size = write_pdf(
        pages,
        &output_path,
        window,
        config.embed_icc.unwrap_or(true),
    )?;

    let elapsed = start.elapsed().as_millis() as u64;
    let size_ratio = if original_total_size > 0 {
        file_size as f32 / original_total_size as f32
    } else {
        0.0
    };
    let exceeded_target =
        matches!(config.merge_mode, MergeMode::Lossless { .. }) && size_ratio > max_ratio;

    Ok(GenerationResult {
        success: true,
        output_path: config.output_path,
        file_size,
        original_total_size,
        page_count: total_page_count,
        elapsed_ms: elapsed,
        mode_used: params.label(),
        size_ratio,
        exceeded_target,
        warnings: all_warnings,
        error: None,
    })
}

#[tauri::command]
pub async fn generate_pdf(
    images: Vec<ImageAnalysis>,
    config: PdfConfig,
    window: tauri::WebviewWindow,
) -> Result<GenerationResult, String> {
    tokio::task::spawn_blocking(move || generate_pdf_core(images, config, &window, ""))
        .await
        .map_err(|e| format!("task join error: {e}"))?
}

/// 去除 Windows 非法文件名字符，保证生成的 PDF 文件名可用。
/// 非法字符：<>:"/\|?* 与控制字符；同时去掉首尾空格与结尾点号，
/// 并规避 Windows 保留设备名（CON/PRN/AUX/NUL/COM1-9/LPT1-9）——
/// 这类名字即使带扩展名也无法创建文件。
fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(
                    c,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0'
                )
        })
        .collect();
    let mut trimmed = cleaned.trim().trim_end_matches('.').trim().to_string();
    // 单段文件名上限 255 字符；留出「 (99).pdf」与路径余量
    if trimmed.chars().count() > 120 {
        trimmed = trimmed.chars().take(120).collect::<String>();
        trimmed = trimmed.trim_end().to_string();
    }
    if trimmed.is_empty() {
        return "output".to_string();
    }
    if is_reserved_device_name(&trimmed) {
        return format!("{trimmed}_");
    }
    trimmed
}

/// 是否为 Windows 保留设备名（大小写不敏感，带扩展名同样冲突）
fn is_reserved_device_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let bytes = upper.as_bytes();
    bytes.len() == 4
        && (bytes[..3] == *b"COM" || bytes[..3] == *b"LPT")
        && (b'1'..=b'9').contains(&bytes[3])
}

/// 为一份 PDF 挑选可用的文件名（不含 .pdf）。
/// - `used` 记录本次批量已占用的名字（小写）；`overwrite` 为 false 时还会避开
///   输出目录里已存在的同名文件，绝不静默覆盖用户上一次的产出
/// - 第一个重名从 `(1)` 开始编号
fn pick_output_name(
    out_dir: &Path,
    base: &str,
    used: &mut HashSet<String>,
    overwrite: bool,
) -> String {
    for suffix in 0..10_000u32 {
        let candidate = if suffix == 0 {
            base.to_string()
        } else {
            format!("{base} ({suffix})")
        };
        let key = candidate.to_lowercase();
        if used.contains(&key) {
            continue;
        }
        if !overwrite && out_dir.join(format!("{candidate}.pdf")).exists() {
            continue;
        }
        used.insert(key);
        return candidate;
    }
    let fallback = format!("{base} {}", uuid::Uuid::new_v4());
    used.insert(fallback.to_lowercase());
    fallback
}

/// 批量生成：每个子文件夹生成一份 PDF，输出到指定目录，
/// 以子文件夹名命名（净化非法字符），重名自动追加序号。
/// 单项失败不影响其他项；进度事件按份附带 "[i / N]" 前缀，
/// 每完成一份额外发送 `pdf_batch_item` 事件供列表逐行更新。
#[tauri::command]
pub async fn generate_pdf_batch(
    items: Vec<BatchItemAnalysis>,
    config: PdfConfig,
    output_dir: String,
    overwrite_existing: Option<bool>,
    window: tauri::WebviewWindow,
) -> Result<Vec<BatchGenerationResult>, String> {
    tokio::task::spawn_blocking(move || {
        let out_dir = PathBuf::from(&output_dir);
        std::fs::create_dir_all(&out_dir).map_err(|e| format!("创建输出目录失败: {}", e))?;

        let overwrite = overwrite_existing.unwrap_or(false);
        let total = items.len();
        let mut used_names: HashSet<String> = HashSet::new();
        let mut results = Vec::with_capacity(total);

        for (idx, item) in items.into_iter().enumerate() {
            let base = sanitize_file_name(&item.name);
            // 默认不覆盖输出目录里已存在的同名 PDF（重名自动加序号），
            // 避免批量重跑时静默替换掉用户上一次的产出。
            let filename = pick_output_name(&out_dir, &base, &mut used_names, overwrite);
            let output_path = out_dir.join(format!("{filename}.pdf"));

            // 不在这里发 pdf_progress：generate_pdf_core 自己会按图片逐条上报，
            // 再叠一层「第几份」的进度会让百分比在 60% 和 37% 之间来回跳。
            // 当前是第几份已经体现在它发出的 phase 前缀 "[i / N]" 里。
            let config = PdfConfig {
                output_path: output_path.to_string_lossy().to_string(),
                ..config.clone()
            };
            let phase_prefix = format!("[{} / {}] ", idx + 1, total);
            let outcome = generate_pdf_core(item.analysis.images, config, &window, &phase_prefix);
            let entry = match outcome {
                Ok(result) => BatchGenerationResult {
                    name: filename,
                    output_path: output_path.to_string_lossy().to_string(),
                    success: true,
                    result: Some(result),
                    error: None,
                },
                Err(e) => BatchGenerationResult {
                    name: filename,
                    output_path: output_path.to_string_lossy().to_string(),
                    success: false,
                    result: None,
                    error: Some(e),
                },
            };
            // 逐条推送，列表不必等整批跑完才更新
            let _ = window.emit(
                "pdf_batch_item",
                BatchItemEvent {
                    index: idx,
                    item: entry.clone(),
                },
            );
            results.push(entry);
        }
        Ok(results)
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

/// JPEG 缩略图专用：利用 DCT 缩放直接解码出接近目标尺寸的图像
///
/// jpeg-decoder 的 `scale()` 支持 1/8、1/4、1/2 缩放因子，会选取
/// 「解码结果在两个轴上都不小于请求尺寸的最小缩放档」，随后只需一次
/// 小幅重采样即可到目标尺寸。对 50MP 照片可避免全尺寸解码，
/// 内存从 ~200MB 降到个位数 MB，耗时下降一个数量级。
///
/// 返回 None 表示不适用（非 RGB/灰度 JPEG 或解码失败），调用方回退通用路径。
fn decode_jpeg_scaled(path: &Path, limit: u32) -> Option<DynamicImage> {
    let file = File::open(path).ok()?;
    let mut decoder = RawJpegDecoder::new(BufReader::new(file));
    let requested = limit.min(u16::MAX as u32) as u16;
    decoder.scale(requested, requested).ok()?;
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (w, h) = (info.width as u32, info.height as u32);
    match info.pixel_format {
        PixelFormat::RGB24 => ImageBuffer::from_raw(w, h, pixels).map(DynamicImage::ImageRgb8),
        PixelFormat::L8 => ImageBuffer::from_raw(w, h, pixels).map(DynamicImage::ImageLuma8),
        _ => None,
    }
}

/// 缩略图缓存：内存 LRU（最近 256 张）+ 磁盘缓存（跨会话复用）。
/// 键包含路径、文件大小、修改时间和目标尺寸，原图变化后会自动失效。
struct ThumbnailCache {
    dir: PathBuf,
    mem: Mutex<lru::LruCache<String, Arc<Vec<u8>>>>,
}

static THUMBNAIL_CACHE: OnceLock<ThumbnailCache> = OnceLock::new();

/// 在应用启动时调用，把缩略图落到应用缓存目录。
pub fn init_thumbnail_cache(dir: PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    let cap = std::num::NonZeroUsize::new(256).expect("256 is non-zero");
    let _ = THUMBNAIL_CACHE.set(ThumbnailCache {
        dir,
        mem: Mutex::new(lru::LruCache::new(cap)),
    });
}

fn thumbnail_cache() -> &'static ThumbnailCache {
    THUMBNAIL_CACHE.get_or_init(|| {
        let dir = std::env::temp_dir().join("toolsbag").join("thumbnails");
        let _ = std::fs::create_dir_all(&dir);
        let cap = std::num::NonZeroUsize::new(256).expect("256 is non-zero");
        ThumbnailCache {
            dir,
            mem: Mutex::new(lru::LruCache::new(cap)),
        }
    })
}

fn thumbnail_key(path: &Path, limit: u32) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    if let Ok(meta) = std::fs::metadata(path) {
        hasher.update(meta.len().to_le_bytes());
        if let Ok(modified) = meta.modified() {
            if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                hasher.update(duration.as_nanos().to_le_bytes());
            }
        }
    }
    hasher.update(limit.to_le_bytes());
    format!("{:x}", hasher.finalize())
}

/// 生成缩略图 JPEG 原始字节（不再 base64，直接走 Tauri IPC 二进制通道）。
fn render_thumbnail_jpeg(path: &Path, limit: u32) -> Result<Vec<u8>, String> {
    let guessed_format = ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .map(|r| r.format().unwrap_or(ImageFormat::Png))
        .unwrap_or(ImageFormat::Png);

    // JPEG 优先走 DCT 缩放解码；TIFF 走 image_crate::open（多页只取第一页）；
    // 其余格式走 fast 路径
    let img = if matches!(guessed_format, ImageFormat::Jpeg) {
        match decode_jpeg_scaled(path, limit) {
            Some(img) => img,
            None => decode_image_fast(path, guessed_format)?,
        }
    } else if matches!(guessed_format, ImageFormat::Tiff) {
        image_crate::open(path).map_err(|e| e.to_string())?
    } else {
        decode_image_fast(path, guessed_format)?
    };

    // 应用 EXIF 旋转，保证缩略图方向与最终 PDF 一致
    let img = apply_exif_orientation(img, path);
    let (w, h) = img.dimensions();
    let thumbnail = if w <= limit && h <= limit {
        img
    } else {
        let scale = if w > h {
            limit as f64 / w as f64
        } else {
            limit as f64 / h as f64
        };
        let new_w = (w as f64 * scale) as u32;
        let new_h = (h as f64 * scale) as u32;
        img.resize(new_w, new_h, FilterType::Triangle)
    };

    // 输出 JPEG q=80 — 比 PNG base64 体积小 5–10 倍
    let rgb = if has_alpha_channel(&thumbnail) {
        flatten_rgba_to_white(&thumbnail.to_rgba8())
    } else {
        thumbnail.to_rgb8()
    };
    let (tw, th) = rgb.dimensions();
    let mut buffer = Cursor::new(Vec::new());
    let mut encoder = image_crate::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, 80);
    encoder
        .encode(rgb.as_raw(), tw, th, image_crate::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())?;
    Ok(buffer.into_inner())
}

fn thumbnail_bytes(path: &Path, limit: u32) -> Result<Arc<Vec<u8>>, String> {
    let cache = thumbnail_cache();
    let key = thumbnail_key(path, limit);

    {
        let mut mem = cache.mem.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = mem.get(&key).cloned() {
            return Ok(hit);
        }
    }

    let disk_path = cache.dir.join(format!("{key}.jpg"));
    if let Ok(bytes) = std::fs::read(&disk_path) {
        if !bytes.is_empty() {
            let arc = Arc::new(bytes);
            let mut mem = cache.mem.lock().unwrap_or_else(|e| e.into_inner());
            mem.put(key, arc.clone());
            return Ok(arc);
        }
    }

    let bytes = render_thumbnail_jpeg(path, limit)?;
    // 原子写：先写临时文件再改名，避免并发请求读到半张图。
    let tmp = disk_path.with_extension("jpg.tmp");
    if std::fs::write(&tmp, &bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &disk_path);
    }
    let arc = Arc::new(bytes);
    {
        let mut mem = cache.mem.lock().unwrap_or_else(|e| e.into_inner());
        mem.put(key, arc.clone());
    }
    Ok(arc)
}

/// 返回缩略图 JPEG 原始字节。
///
/// 旧实现返回 `data:image/jpeg;base64,...`：每张图多 33% 体积，还要在 IPC 上做
/// 字符串编码/解码。现在直接返回二进制，前端用 Blob URL 挂到 `<img>`，大图列表
/// 的内存和耗时都明显下降。
#[tauri::command]
pub async fn get_image_thumbnail(
    image_path: String,
    max_size: u32,
) -> Result<tauri::ipc::Response, String> {
    let path = PathBuf::from(&image_path);
    let limit = max_size.clamp(256, 1024);
    let bytes = tokio::task::spawn_blocking(move || thumbnail_bytes(&path, limit))
        .await
        .map_err(|e| format!("task join error: {e}"))??;
    Ok(tauri::ipc::Response::new((*bytes).clone()))
}

/// 后台预生成缩略图并写入磁盘缓存，供后续滚动/翻页秒开。
/// 只处理前 120 个去重路径，避免一次性把 CPU 占满。
#[tauri::command]
pub async fn prewarm_thumbnails(paths: Vec<String>, max_size: u32) -> Result<usize, String> {
    let limit = max_size.clamp(256, 1024);
    tokio::task::spawn_blocking(move || {
        let unique: HashSet<String> = paths.into_iter().take(120).collect();
        let paths: Vec<String> = unique.into_iter().collect();
        Ok::<usize, String>(
            paths
                .par_iter()
                .filter(|p| thumbnail_bytes(Path::new(p.as_str()), limit).is_ok())
                .count(),
        )
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {expected}, got {actual}"
        );
    }

    fn sample_image(width: u32, height: u32) -> ImageAnalysis {
        ImageAnalysis {
            path: "sample.jpg".to_string(),
            width,
            height,
            is_portrait: height > width,
            format: "jpeg".to_string(),
            has_alpha: false,
            is_apng: false,
            is_animated_webp: false,
            is_grayscale: false,
            bit_depth: 8,
            has_gamma: false,
            gamma: None,
            file_size: 0,
            frame_count: 1,
            has_icc: false,
            icc_profile: None,
            icc_version: None,
            exif_color_space: None,
            is_long_image: width > 0
                && height > 0
                && (width as f32 / height as f32 > 5.0 || height as f32 / width as f32 > 5.0),
        }
    }

    #[test]
    fn validate_margin_for_mode_rejects_negative_and_oversized() {
        let fixed = PageMode::Fixed {
            orientation: FixedOrientation::Auto,
        };
        assert!(validate_margin_for_mode(-1.0, &fixed, PageSize::A4).is_err());
        assert!(validate_margin_for_mode(85.0, &fixed, PageSize::A4).is_err());
        assert_eq!(
            validate_margin_for_mode(20.0, &fixed, PageSize::A4).expect("valid margin"),
            20.0
        );
        // 原图模式无幅面限制
        let orig = PageMode::Original;
        assert_eq!(
            validate_margin_for_mode(50.0, &orig, PageSize::A4).expect("original ok"),
            50.0
        );
    }

    #[test]
    fn fixed_orientation_resolves_based_on_image_shape() {
        let auto_fixed = PageMode::Fixed {
            orientation: FixedOrientation::Auto,
        };
        // 竖图 → 竖幅面
        assert_eq!(
            get_page_dimensions(&auto_fixed, PageSize::A4, 400, 800),
            (210.0, 297.0)
        );
        // 横图 → 横幅面
        assert_eq!(
            get_page_dimensions(&auto_fixed, PageSize::A4, 800, 400),
            (297.0, 210.0)
        );
        // 强制竖向
        let force_p = PageMode::Fixed {
            orientation: FixedOrientation::Portrait,
        };
        assert_eq!(
            get_page_dimensions(&force_p, PageSize::A4, 800, 400),
            (210.0, 297.0)
        );
        // 强制横向
        let force_l = PageMode::Fixed {
            orientation: FixedOrientation::Landscape,
        };
        assert_eq!(
            get_page_dimensions(&force_l, PageSize::A4, 400, 800),
            (297.0, 210.0)
        );
    }

    #[test]
    fn original_mode_uses_pixel_dimensions_at_72dpi() {
        let orig = PageMode::Original;
        let (w, h) = get_page_dimensions(&orig, PageSize::A4, 1920, 1080);
        // 1920 px @ 72 DPI = 1920 * 25.4 / 72 ≈ 677.33 mm
        assert!((w - 677.333).abs() < 0.01);
        assert!((h - 380.999).abs() < 0.01);
    }

    /// 用户场景：合并三张不同比例的图片
    /// 800×600（4:3 横图）/ 1000×2560（≈ 0.39 长竖图）/ 781×781（1:1 方图）
    /// 期望：Original 模式下每张图获得独立的页面尺寸 + 独立的页面比例，
    /// 图像完全填充页面（无白边、无居中偏移）。
    #[test]
    fn original_mode_per_image_pages_keep_aspect_ratio() {
        let orig = PageMode::Original;
        let img1 = sample_image(800, 600); // 4:3 横
        let img2 = sample_image(1000, 2560); // 25:64 竖（极长）
        let img3 = sample_image(781, 781); // 1:1 方

        let l1 = calculate_page_layout(&img1, &orig, PageSize::A4, 0.0);
        let l2 = calculate_page_layout(&img2, &orig, PageSize::A4, 0.0);
        let l3 = calculate_page_layout(&img3, &orig, PageSize::A4, 0.0);

        // 1) 三张页的尺寸两两不同
        assert!(
            (l1.page_width - l2.page_width).abs() > 1.0,
            "page1 width should differ from page2"
        );
        assert!(
            (l1.page_height - l2.page_height).abs() > 1.0,
            "page1 height should differ from page2"
        );
        assert!((l1.page_width - l3.page_width).abs() > 1.0);
        assert!((l2.page_width - l3.page_width).abs() > 1.0);

        // 2) 每页比例严格等于源图比例
        let ratio1 = l1.page_width / l1.page_height;
        let ratio2 = l2.page_width / l2.page_height;
        let ratio3 = l3.page_width / l3.page_height;
        assert!(
            (ratio1 - 800.0 / 600.0).abs() < 1e-6,
            "page1 ratio should match 800:600"
        );
        assert!(
            (ratio2 - 1000.0 / 2560.0).abs() < 1e-6,
            "page2 ratio should match 1000:2560"
        );
        assert!((ratio3 - 1.0).abs() < 1e-6, "page3 ratio should match 1:1");

        // 3) 具体 mm 尺寸（72 DPI）
        //    800 × 25.4 / 72 ≈ 282.22 mm
        //    600 × 25.4 / 72 ≈ 211.67 mm
        assert!((l1.page_width - 282.222).abs() < 0.01);
        assert!((l1.page_height - 211.667).abs() < 0.01);
        //    1000 × 25.4 / 72 ≈ 352.78 mm
        //    2560 × 25.4 / 72 ≈ 903.11 mm
        assert!((l2.page_width - 352.778).abs() < 0.01);
        assert!((l2.page_height - 903.111).abs() < 0.01);
        //    781 × 25.4 / 72 ≈ 275.51 mm
        assert!((l3.page_width - 275.511).abs() < 0.01);
        assert!((l3.page_height - 275.511).abs() < 0.01);

        // 4) 图像完全填满页面：无偏移、无白边
        for layout in [&l1, &l2, &l3] {
            assert!(
                layout.image.x.abs() < 0.01,
                "image should start at x=0 (got {})",
                layout.image.x
            );
            assert!(
                layout.image.y.abs() < 0.01,
                "image should start at y=0 (got {})",
                layout.image.y
            );
            assert!(
                (layout.image.scaled_width - layout.page_width).abs() < 0.5,
                "image width should equal page width"
            );
            assert!(
                (layout.image.scaled_height - layout.page_height).abs() < 0.5,
                "image height should equal page height"
            );
            assert_close(layout.margin, 0.0);
        }
    }

    #[test]
    fn layout_preserves_aspect_ratio_and_centers_image() {
        let image = sample_image(400, 200);
        let fixed = PageMode::Fixed {
            orientation: FixedOrientation::Landscape,
        };
        let layout = calculate_page_layout(&image, &fixed, PageSize::A4, 10.0);

        assert_close(layout.page_width, 297.0);
        assert_close(layout.page_height, 210.0);
        assert_close(layout.image.scaled_width, 277.0);
        assert_close(layout.image.scaled_height, 138.5);
        assert_close(layout.image.x, 10.0);
        assert_close(layout.image.y, 35.75);
    }

    #[test]
    fn natural_sort_orders_numeric_runs_naturally() {
        use std::cmp::Ordering;
        let cmp = |a: &str, b: &str| natural_compare(OsStr::new(a), OsStr::new(b));
        assert_eq!(cmp("IMG_2.jpg", "IMG_10.jpg"), Ordering::Less);
        assert_eq!(cmp("a10", "a9"), Ordering::Greater);
        assert_eq!(cmp("a1", "A1"), Ordering::Equal);
    }

    #[test]
    fn natural_sort_handles_zero_padding() {
        use std::cmp::Ordering;
        let cmp = |a: &str, b: &str| natural_compare(OsStr::new(a), OsStr::new(b));
        // 数值相等 → 比较原字符串长度（"01" 排在 "1" 之后）
        assert_eq!(cmp("1.jpg", "01.jpg"), Ordering::Less);
    }

    #[test]
    fn portable_params_map_endpoints() {
        let p100 = ProcessParams::portable(1.0);
        assert_eq!(p100.quality, 95);
        assert!((p100.scale - 1.0).abs() < 1e-3);

        let p10 = ProcessParams::portable(0.10);
        assert_eq!(p10.quality, 40);
        assert!((p10.scale - 0.35).abs() < 1e-3);

        // out-of-range clamped
        let p0 = ProcessParams::portable(0.0);
        assert_eq!(p0.quality, 40);
    }

    #[test]
    fn original_params_are_lossless() {
        let p = ProcessParams::original();
        assert!(p.passthrough_jpeg);
        assert!(p.force_flate);
        assert!((p.scale - 1.0).abs() < 1e-3);
    }

    #[test]
    fn original_mode_forces_lossless_flate_for_photo_png() {
        let dir =
            std::env::temp_dir().join(format!("toolsbag-pdf-original-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("photo.png");
        let mut img = RgbImage::new(256, 256);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = image_crate::Rgb([
                (x % 256) as u8,
                (y % 256) as u8,
                ((x * 7 + y * 11) % 251) as u8,
            ]);
        }
        img.save(&path).unwrap();

        let config = PdfConfig {
            page_size: PageSize::A4,
            page_mode: PageMode::Original,
            margin: 0.0,
            output_path: String::new(),
            merge_mode: MergeMode::Original,
            embed_icc: Some(true),
            color_mode: ColorMode::Preserve,
        };
        let original = process_image(
            &path,
            &config,
            &ProcessParams::original(),
            false,
            false,
            &ColorPlan::Passthrough,
        )
        .unwrap();
        assert_eq!(original[0].processed.encoding, ImageEncoding::Flate);

        // 普通无损模式对照片型 PNG 会走 JPEG；原样模式必须保持 Flate。
        let smart = process_image(
            &path,
            &config,
            &ProcessParams::lossless(95, true),
            false,
            false,
            &ColorPlan::Passthrough,
        )
        .unwrap();
        assert_eq!(smart[0].processed.encoding, ImageEncoding::Dct);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn merge_mode_default_is_lossless_110pct() {
        match MergeMode::default() {
            MergeMode::Lossless { max_size_ratio } => assert!((max_size_ratio - 1.10).abs() < 1e-6),
            _ => panic!("expected default Lossless"),
        }
    }

    #[test]
    fn page_layout_includes_image_path() {
        let image = sample_image(400, 200);
        let fixed = PageMode::Fixed {
            orientation: FixedOrientation::Landscape,
        };
        let layout = calculate_page_layout(&image, &fixed, PageSize::A4, 10.0);
        assert_eq!(layout.image_path, "sample.jpg");
    }

    #[test]
    fn original_mode_has_zero_margin() {
        let image = sample_image(400, 200);
        let orig = PageMode::Original;
        let layout = calculate_page_layout(&image, &orig, PageSize::A4, 10.0);
        assert_close(layout.margin, 0.0);
    }

    /// 构造一个最小可识别的"假 JPEG"用于测试 marker 扫描：
    /// SOI + APP0(JFIF, 16 字节) + DQT(短桩, 5 字节) + EOI
    fn fake_jpeg_with_jfif() -> Vec<u8> {
        let mut v = Vec::new();
        // SOI
        v.extend_from_slice(&[0xFF, 0xD8]);
        // APP0 (JFIF), length = 16
        v.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
        v.extend_from_slice(b"JFIF\0");
        v.extend_from_slice(&[
            0x01, 0x02, // version 1.02
            0x00, // density units
            0x00, 0x48, 0x00, 0x48, // X/Y density
            0x00, 0x00, // thumbnail w/h
        ]);
        // DQT 桩 (length=3)
        v.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x03, 0x00]);
        // EOI
        v.extend_from_slice(&[0xFF, 0xD9]);
        v
    }

    #[test]
    fn app14_inserted_after_jfif_app0() {
        let mut data = fake_jpeg_with_jfif();
        let original_len = data.len();
        let injected = inject_app14_ycbcr(&mut data);
        assert!(injected);
        assert_eq!(data.len(), original_len + 16);
        // SOI + APP0(18 bytes total: FFE0 + len 0x0010=16 incl length itself) → APP14 应插在 i=20
        assert_eq!(&data[2..4], &[0xFF, 0xE0]); // APP0 仍在原位
        assert_eq!(&data[20..22], &[0xFF, 0xEE]); // APP14 紧随其后
        assert_eq!(&data[24..29], b"Adobe");
        assert_eq!(data[35], 0x01); // ColorTransform = 1 (YCbCr)
    }

    #[test]
    fn app14_skipped_when_already_present() {
        // SOI + APP14 + EOI
        let mut data = vec![0xFF, 0xD8];
        data.extend_from_slice(&APP14_ADOBE_YCBCR);
        data.extend_from_slice(&[0xFF, 0xD9]);
        let before = data.clone();
        let injected = inject_app14_ycbcr(&mut data);
        assert!(!injected);
        assert_eq!(data, before, "已有 APP14 时不应重复注入");
    }

    #[test]
    fn app14_noop_on_non_jpeg() {
        let mut data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let before = data.clone();
        let injected = inject_app14_ycbcr(&mut data);
        assert!(!injected);
        assert_eq!(data, before);
    }

    #[test]
    fn app14_inserted_when_no_app_segments() {
        // SOI 直接接 DQT（无 APP0/APP14） — APP14 应插在 SOI 之后、DQT 之前
        let mut data = vec![0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x03, 0x00];
        let injected = inject_app14_ycbcr(&mut data);
        assert!(injected);
        assert_eq!(&data[0..2], &[0xFF, 0xD8]);
        assert_eq!(&data[2..4], &[0xFF, 0xEE]); // APP14
        assert_eq!(&data[18..20], &[0xFF, 0xDB]); // DQT 被推到后面
    }

    #[test]
    fn classify_orientation_handles_three_cases() {
        assert_eq!(
            classify_orientation(1920, 1080),
            OrientationClass::Landscape
        );
        assert_eq!(classify_orientation(1080, 1920), OrientationClass::Portrait);
        assert_eq!(classify_orientation(1024, 1024), OrientationClass::Square);
        // [0.95, 1.05] 边界附近：1080/1024 ≈ 1.055，已超阈值 → 不算方图
        assert_eq!(
            classify_orientation(1080, 1024),
            OrientationClass::Landscape
        );
        // 4:5 (Instagram portrait) ratio = 0.8 → 竖图
        assert_eq!(classify_orientation(1080, 1350), OrientationClass::Portrait);
        // 边界保护：宽或高为 0 不应 panic
        assert_eq!(classify_orientation(0, 100), OrientationClass::Landscape);
    }

    #[test]
    fn classify_resolution_buckets_by_long_edge() {
        assert_eq!(classify_resolution(640, 480).0, "SD (< 720p)");
        assert_eq!(classify_resolution(1280, 720).0, "HD (720p)");
        assert_eq!(classify_resolution(1920, 1080).0, "FHD (1080p)");
        assert_eq!(classify_resolution(2560, 1440).0, "QHD / 2K");
        assert_eq!(classify_resolution(3840, 2160).0, "4K UHD");
        // 长边为短边的方向无关：取 max
        assert_eq!(classify_resolution(1080, 1920).0, "FHD (1080p)");
        // 8K 及以上
        assert_eq!(classify_resolution(7680, 4320).0, "8K+");
        // 5K
        assert_eq!(classify_resolution(5120, 2880).0, "5K");
    }

    // ==================== P0: PNG 内容分类测试 ====================

    /// 构造纯色图（唯一颜色 = 1，应判定为 Flate）
    #[test]
    fn classify_png_solid_color_is_flate() {
        let rgb = RgbImage::from_pixel(64, 64, image_crate::Rgb([128, 128, 128]));
        assert_eq!(classify_png_content(&rgb), ImageEncoding::Flate);
    }

    /// 构造少量颜色图（< 4096 唯一颜色，应判定为 Flate）
    #[test]
    fn classify_png_few_colors_is_flate() {
        let mut rgb = RgbImage::new(64, 64);
        // 只用 256 种颜色（0..255 的灰度）
        for y in 0..64 {
            for x in 0..64 {
                let v = ((y * 64 + x) % 256) as u8;
                rgb.put_pixel(x, y, image_crate::Rgb([v, v, v]));
            }
        }
        assert_eq!(classify_png_content(&rgb), ImageEncoding::Flate);
    }

    /// 构造照片类图（大量唯一颜色，应判定为 Dct）
    #[test]
    fn classify_png_many_colors_is_dct() {
        let mut rgb = RgbImage::new(256, 256);
        // 每个像素颜色不同 → 65536 唯一颜色 > 4096
        for y in 0..256 {
            for x in 0..256 {
                let r = ((x * 3 + y * 7) % 256) as u8;
                let g = ((x * 11 + y * 13) % 256) as u8;
                let b = ((x * 17 + y * 19) % 256) as u8;
                rgb.put_pixel(x, y, image_crate::Rgb([r, g, b]));
            }
        }
        assert_eq!(classify_png_content(&rgb), ImageEncoding::Dct);
    }

    /// 构造二值图（黑白两色，应判定为 Flate）
    #[test]
    fn classify_png_binary_is_flate() {
        let mut rgb = RgbImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                rgb.put_pixel(x, y, image_crate::Rgb([v, v, v]));
            }
        }
        assert_eq!(classify_png_content(&rgb), ImageEncoding::Flate);
    }

    // ==================== P0: Flate 编码测试 ====================

    #[test]
    fn encode_rgb_to_flate_produces_correct_dimensions() {
        let rgb = RgbImage::from_pixel(32, 16, image_crate::Rgb([100, 150, 200]));
        let processed = encode_rgb_to_flate_processed(&rgb);
        assert_eq!(processed.width, 32);
        assert_eq!(processed.height, 16);
        assert_eq!(processed.encoding, ImageEncoding::Flate);
        // data 已是 zlib 压缩后的流：解压后应还原为 32 * 16 * 3 字节原始像素行
        let mut raw = Vec::new();
        flate2::read::ZlibDecoder::new(processed.data.as_slice())
            .read_to_end(&mut raw)
            .expect("zlib 解压失败");
        assert_eq!(raw, rgb.as_raw().clone());
    }

    /// 纯色图走 Flate 分支时，必须真的选 Flate（无损），而不是被 JPEG 抢走。
    /// 回归点：早期版本拿「Flate 的原始像素字节数」和「JPEG 压缩后字节数」比较，
    /// JPEG 几乎必然更小，Flate 分支形同虚设，线稿/截图被有损压缩。
    #[test]
    fn flate_branch_wins_for_line_art() {
        let rgb = RgbImage::from_pixel(512, 512, image_crate::Rgb([255, 255, 255]));
        let flate = encode_rgb_to_flate_processed(&rgb);
        let jpeg = encode_rgb_to_jpeg_processed(&rgb, 95).unwrap();
        assert!(
            flate.data.len() < jpeg.data.len(),
            "纯色图 Flate({}) 应小于 JPEG({})",
            flate.data.len(),
            jpeg.data.len()
        );
    }

    #[test]
    fn encode_rgb_to_jpeg_has_dct_encoding() {
        let rgb = RgbImage::from_pixel(32, 16, image_crate::Rgb([100, 150, 200]));
        let processed = encode_rgb_to_jpeg_processed(&rgb, 95).unwrap();
        assert_eq!(processed.encoding, ImageEncoding::Dct);
        // JPEG 数据应以 SOI marker 开头
        assert!(processed.data.len() >= 2);
        assert_eq!(processed.data[0], 0xFF);
        assert_eq!(processed.data[1], 0xD8);
    }

    // ==================== P1+P2: ImageAnalysis 新字段测试 ====================

    #[test]
    fn sample_image_has_default_metadata_fields() {
        let img = sample_image(400, 300);
        assert!(!img.is_apng);
        assert!(!img.is_grayscale);
        assert_eq!(img.bit_depth, 8);
        assert!(!img.has_gamma);
        assert!(img.gamma.is_none());
        assert!(!img.has_icc);
        assert!(img.icc_profile.is_none());
        assert!(img.exif_color_space.is_none());
    }

    // ==================== P1-3: magic number 检测测试 ====================

    #[test]
    fn detect_format_by_magic_jpeg() {
        let dir = std::env::temp_dir();
        let p = dir.join("magic_test.jpg");
        std::fs::write(&p, b"\xFF\xD8\xFF\xE0JFIF").unwrap();
        assert_eq!(detect_format_by_magic(&p), DetectedFormat::Jpeg);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn detect_format_by_magic_png() {
        let dir = std::env::temp_dir();
        let p = dir.join("magic_test.png");
        std::fs::write(&p, b"\x89PNG\r\n\x1A\n").unwrap();
        assert_eq!(detect_format_by_magic(&p), DetectedFormat::Png);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn detect_format_by_magic_pdf_disguised_as_jpg() {
        // .jpg 扩展名但实际是 PDF
        let dir = std::env::temp_dir();
        let p = dir.join("fake.jpg");
        std::fs::write(&p, b"%PDF-1.4 ...").unwrap();
        assert_eq!(detect_format_by_magic(&p), DetectedFormat::Pdf);
        assert!(!detect_format_by_magic(&p).is_processable());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn detect_format_by_magic_psd_disguised() {
        let dir = std::env::temp_dir();
        let p = dir.join("fake.png");
        std::fs::write(&p, b"8BPS\x00\x01").unwrap();
        assert_eq!(detect_format_by_magic(&p), DetectedFormat::Psd);
        let _ = std::fs::remove_file(&p);
    }

    // ==================== P1-2: 内存保护测试 ====================

    #[test]
    fn check_image_memory_ok_for_normal_image() {
        // 1920x1080 x4 = ~8MB，远低于 400MB
        assert_eq!(check_image_memory(1920, 1080), MemoryCheck::Ok);
    }

    #[test]
    fn check_image_memory_oversized_for_huge_image() {
        // 20000x20000 x4 = 1.6GB，超过 400MB
        match check_image_memory(20000, 20000) {
            MemoryCheck::Oversized { estimated_mb, .. } => assert!(estimated_mb > 400),
            _ => panic!("expected Oversized"),
        }
    }

    // ==================== P1-1: SMask 分离测试 ====================

    #[test]
    fn split_rgba_separates_rgb_and_alpha() {
        let mut rgba = RgbaImage::new(2, 2);
        rgba.put_pixel(0, 0, image_crate::Rgba([10, 20, 30, 40]));
        rgba.put_pixel(1, 0, image_crate::Rgba([50, 60, 70, 80]));
        rgba.put_pixel(0, 1, image_crate::Rgba([90, 100, 110, 120]));
        rgba.put_pixel(1, 1, image_crate::Rgba([130, 140, 150, 160]));
        let (rgb, alpha) = split_rgba_to_rgb_alpha(&rgba);
        assert_eq!(rgb.get_pixel(0, 0).0, [10, 20, 30]);
        assert_eq!(alpha, vec![40, 80, 120, 160]);
    }

    // ==================== 长图判定测试 ====================

    #[test]
    fn long_image_detection() {
        // 1080x20000 长截图 → 长图
        let long = sample_image(1080, 20000);
        assert!(long.is_long_image);
        // 1920x1080 普通横图 → 非长图
        let normal = sample_image(1920, 1080);
        assert!(!normal.is_long_image);
        // 1080x1350 竖图 → 非长图
        let portrait = sample_image(1080, 1350);
        assert!(!portrait.is_long_image);
    }

    #[test]
    fn completed_temp_pdf_replaces_existing_output() {
        let dir =
            std::env::temp_dir().join(format!("toolsbag-pdf-commit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("output.pdf");
        let temp = dir.join("output.tmp");
        std::fs::write(&output, b"old pdf").unwrap();
        std::fs::write(&temp, b"new pdf").unwrap();

        commit_output_file(&temp, &output).unwrap();

        assert_eq!(std::fs::read(&output).unwrap(), b"new pdf");
        assert!(!temp.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    // ==================== ICC 提取测试 ====================

    #[test]
    fn icc_components_parses_color_space() {
        let mut rgb = vec![0u8; 128];
        rgb[16..20].copy_from_slice(b"RGB ");
        rgb[36..40].copy_from_slice(b"acsp");
        assert_eq!(icc_components(&rgb), 3);

        let mut gray = vec![0u8; 128];
        gray[16..20].copy_from_slice(b"GRAY");
        gray[36..40].copy_from_slice(b"acsp");
        assert_eq!(icc_components(&gray), 1);

        let junk = vec![0u8; 128];
        assert_eq!(icc_components(&junk), 0);
    }

    /// 构造带 ICC 的假 JPEG：SOI + APP2(ICC_PROFILE 拆成 2 段) + EOI
    fn fake_jpeg_with_icc(profile: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&[0xFF, 0xD8]); // SOI
        let mid = profile.len() / 2;
        for (idx, slice) in [&profile[..mid], &profile[mid..]].iter().enumerate() {
            let seg_len = 14 + slice.len() + 2; // 段长含自身 2 字节长度字段
            v.push(0xFF);
            v.push(0xE2); // APP2
            v.extend_from_slice(&[((seg_len >> 8) & 0xFF) as u8, (seg_len & 0xFF) as u8]);
            v.extend_from_slice(b"ICC_PROFILE\x00");
            v.push((idx + 1) as u8); // seq
            v.push(2); // total
            v.extend_from_slice(slice);
        }
        v.extend_from_slice(&[0xFF, 0xD9]); // EOI
        v
    }

    #[test]
    fn extract_jpeg_icc_reassembles_split_chunks() {
        let mut profile = vec![0u8; 200];
        profile[36..40].copy_from_slice(b"acsp");
        let data = fake_jpeg_with_icc(&profile);
        let extracted = extract_jpeg_icc_from_bytes(&data).expect("icc 提取成功");
        assert_eq!(extracted, profile);
    }

    #[test]
    fn extract_jpeg_icc_none_without_profile() {
        let data = fake_jpeg_with_jfif();
        assert!(extract_jpeg_icc_from_bytes(&data).is_none());
    }

    // ==================== 分析失败上报 / alpha 检测 / SMask 测试 ====================

    #[test]
    fn analyze_reports_failed_files_in_skipped() {
        let dir =
            std::env::temp_dir().join(format!("toolsbag-pdf-analyze-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // 1×1 红色 JPEG（有效）
        let mut buf = Cursor::new(Vec::new());
        let rgb = RgbImage::from_pixel(1, 1, image_crate::Rgb([255, 0, 0]));
        let mut enc = image_crate::codecs::jpeg::JpegEncoder::new(&mut buf);
        enc.encode(rgb.as_raw(), 1, 1, image_crate::ExtendedColorType::Rgb8)
            .unwrap();
        std::fs::write(dir.join("ok.jpg"), buf.into_inner()).unwrap();
        // 损坏的 .jpg（垃圾字节，无 magic 也无法解码）→ 应出现在 skipped 并附原因
        std::fs::write(dir.join("broken.jpg"), b"\x00\x01\x02\x03 not an image").unwrap();

        let analysis = match analyze_folder_impl(&dir, false).expect("analyze should succeed") {
            FolderScan::Ready(analysis) => analysis,
            FolderScan::NoImages { .. } => panic!("应至少分析出一张图"),
        };
        assert_eq!(analysis.images.len(), 1);
        assert_eq!(analysis.skipped.len(), 1);
        assert!(
            analysis.skipped[0].reason.contains("分析失败"),
            "reason: {}",
            analysis.skipped[0].reason
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 目录里没有任何可用图片 → NoImages（而不是 Err），批量模式据此静默跳过该子目录
    #[test]
    fn analyze_empty_folder_reports_no_images() {
        let dir = std::env::temp_dir().join(format!("toolsbag-pdf-empty-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("note.txt"), b"hello").unwrap();
        match analyze_folder_impl(&dir, false).expect("读取目录本身不应失败") {
            FolderScan::NoImages { unsupported_exts } => assert!(unsupported_exts.is_empty()),
            FolderScan::Ready(_) => panic!("空目录不应有分析结果"),
        }
        // 只有不支持格式的目录也一样
        std::fs::write(dir.join("clip.heic"), b"not really heic").unwrap();
        match analyze_folder_impl(&dir, false).expect("读取目录本身不应失败") {
            FolderScan::NoImages { unsupported_exts } => {
                assert_eq!(unsupported_exts, vec![".heic".to_string()])
            }
            FolderScan::Ready(_) => panic!("无可用图片时不应有分析结果"),
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn inspect_png_metadata_detects_real_alpha() {
        use image_crate::ImageEncoder as _;
        let dir = std::env::temp_dir();

        // RGBA PNG → has_alpha true
        let rgba = RgbaImage::from_pixel(2, 2, image_crate::Rgba([255, 0, 0, 128]));
        let path = dir.join(format!("alpha_{}.png", uuid::Uuid::new_v4()));
        let mut buf = Cursor::new(Vec::new());
        let enc = image_crate::codecs::png::PngEncoder::new(&mut buf);
        enc.write_image(rgba.as_raw(), 2, 2, image_crate::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&path, buf.into_inner()).unwrap();
        let meta = inspect_png_metadata(&path);
        assert!(meta.has_alpha, "RGBA PNG 应检测到 alpha");
        let _ = std::fs::remove_file(&path);

        // RGB PNG → has_alpha false（修复按格式一刀切误报）
        let rgb = RgbImage::from_pixel(2, 2, image_crate::Rgb([10, 20, 30]));
        let path2 = dir.join(format!("nonalpha_{}.png", uuid::Uuid::new_v4()));
        let mut buf2 = Cursor::new(Vec::new());
        let enc2 = image_crate::codecs::png::PngEncoder::new(&mut buf2);
        enc2.write_image(rgb.as_raw(), 2, 2, image_crate::ExtendedColorType::Rgb8)
            .unwrap();
        std::fs::write(&path2, buf2.into_inner()).unwrap();
        let meta2 = inspect_png_metadata(&path2);
        assert!(!meta2.has_alpha, "RGB PNG 不应误报 alpha");
        let _ = std::fs::remove_file(&path2);
    }

    /// 非方形透明图（30×40）的 SMask 必须按实际宽高写入，
    /// 这是修复 printpdf 时代「方形才保留透明」限制的回归测试。
    #[test]
    fn build_document_smask_uses_actual_dimensions() {
        let rgb = RgbImage::from_pixel(30, 40, image_crate::Rgb([10, 20, 30]));
        let mut processed = encode_rgb_to_flate_processed(&rgb);
        processed.alpha_data = Some(vec![255u8; 30 * 40]);
        let pages = vec![PreparedPage {
            processed,
            layout: ImageLayout {
                original_width: 30,
                original_height: 40,
                scaled_width: 60.0,
                scaled_height: 80.0,
                x: 0.0,
                y: 0.0,
            },
            page_width: 60.0,
            page_height: 80.0,
        }];
        let (doc, total, _) = build_document(vec![pages], false).unwrap();
        assert_eq!(total, 1);

        let smask_id = doc
            .objects
            .values()
            .find_map(|obj| match obj {
                Object::Stream(s) => s
                    .dict
                    .get(b"SMask")
                    .ok()
                    .and_then(|o| o.as_reference().ok()),
                _ => None,
            })
            .expect("主图流应引用 SMask");
        match doc.objects.get(&smask_id) {
            Some(Object::Stream(s)) => {
                let width = s.dict.get(b"Width").ok().and_then(|o| o.as_i64().ok());
                let height = s.dict.get(b"Height").ok().and_then(|o| o.as_i64().ok());
                assert_eq!(width, Some(30));
                assert_eq!(height, Some(40));
            }
            other => panic!("预期 SMask 流，实际 {:?}", other),
        }
    }

    #[test]
    fn extract_png_icc_inflates_iccp_chunk() {
        // 构造一个带 iCCP chunk 的最小 PNG：签名 + iCCP + IEND
        let mut profile = vec![0u8; 64];
        profile[36..40].copy_from_slice(b"acsp");
        profile[16..20].copy_from_slice(b"RGB ");

        use flate2::write::ZlibEncoder;
        let mut encoder = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &profile).unwrap();
        let compressed = encoder.finish().unwrap();

        // iCCP data = 名称 + NUL + 压缩方法(0) + zlib 压缩的 profile
        let mut iccp_data = b"test profile".to_vec();
        iccp_data.push(0);
        iccp_data.push(0);
        iccp_data.extend_from_slice(&compressed);

        let mut png = Vec::new();
        png.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        png.extend_from_slice(&(iccp_data.len() as u32).to_be_bytes());
        png.extend_from_slice(b"iCCP");
        png.extend_from_slice(&iccp_data);
        png.extend_from_slice(&[0u8, 0, 0, 0]); // CRC 占位（提取时不校验）
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0u8, 0, 0, 0]);

        let dir = std::env::temp_dir();
        let path = dir.join(format!("icc_test_{}.png", uuid::Uuid::new_v4()));
        std::fs::write(&path, &png).unwrap();
        let extracted = extract_png_icc_bytes(&path).expect("png icc 提取成功");
        assert_eq!(extracted, profile);
        let _ = std::fs::remove_file(&path);
    }

    // ==================== PDF 结构往返测试 ====================

    /// 构造一个 PreparedPage（2×2 纯色图，Flate 编码），可选携带 ICC
    fn prepared_page(icc: Option<Vec<u8>>) -> PreparedPage {
        let rgb = RgbImage::from_pixel(2, 2, image_crate::Rgb([200, 50, 50]));
        let mut processed = encode_rgb_to_flate_processed(&rgb);
        processed.icc_profile_bytes = icc;
        PreparedPage {
            processed,
            layout: ImageLayout {
                original_width: 2,
                original_height: 2,
                scaled_width: 20.0,
                scaled_height: 20.0,
                x: 0.0,
                y: 0.0,
            },
            page_width: 20.0,
            page_height: 20.0,
        }
    }

    /// 扫描文档中是否存在 ICCBased 色彩空间流（dict 带 Alternate /DeviceRGB）
    fn has_icc_stream(doc: &Document) -> bool {
        doc.objects.values().any(|obj| {
            if let Object::Stream(stream) = obj {
                stream
                    .dict
                    .get(b"Alternate")
                    .map(|o| o.as_name_str().is_ok_and(|s| s == "DeviceRGB"))
                    .unwrap_or(false)
            } else {
                false
            }
        })
    }

    #[test]
    fn build_document_roundtrip_is_valid_with_icc() {
        let mut profile = vec![0u8; 128];
        profile[16..20].copy_from_slice(b"RGB ");
        profile[36..40].copy_from_slice(b"acsp");

        // 2 页：第一页带 RGB ICC，第二页不带
        let (mut doc, total_pages, est) = build_document(
            vec![
                vec![prepared_page(Some(profile.clone()))],
                vec![prepared_page(None)],
            ],
            true,
        )
        .expect("build document");

        assert_eq!(total_pages, 2);
        assert!(est > 0);
        assert!(has_icc_stream(&doc), "开启 ICC 时应有 ICCBased 流");

        // 落盘并重新解析：验证 xref/页树/对象结构合法
        let mut buf = Cursor::new(Vec::new());
        doc.save_to(&mut buf).expect("save to bytes");
        let bytes = buf.into_inner();
        assert!(bytes.starts_with(b"%PDF"), "应以 %PDF 开头");
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"), "应以 %%EOF 结尾");

        let parsed = lopdf::Document::load_mem(&bytes).expect("re-parse pdf");
        assert_eq!(parsed.get_pages().len(), 2, "重新解析后应有 2 页");
        assert!(has_icc_stream(&parsed), "重解析后仍应包含 ICCBased 流");

        // 关闭 ICC 嵌入 → 不应出现 ICCBased 流
        let (mut doc2, _, _) = build_document(vec![vec![prepared_page(Some(profile))]], false)
            .expect("build without icc");
        assert!(!has_icc_stream(&doc2), "关闭 ICC 后不应有 ICCBased 流");
        let mut buf2 = Cursor::new(Vec::new());
        doc2.save_to(&mut buf2).expect("save no-icc");
        let parsed2 = lopdf::Document::load_mem(&buf2.into_inner()).expect("re-parse no-icc");
        assert!(!has_icc_stream(&parsed2));
        assert_eq!(parsed2.get_pages().len(), 1);
    }

    // ==================== Adobe RGB profile 测试 ====================

    /// 从 ICC profile 中读取某个 tag 的数据段
    fn icc_tag_data<'a>(profile: &'a [u8], sig: &[u8; 4]) -> Option<&'a [u8]> {
        let count = u32::from_be_bytes(profile[128..132].try_into().ok()?) as usize;
        for i in 0..count {
            let base = 132 + i * 12;
            if base + 12 > profile.len() {
                return None;
            }
            if &profile[base..base + 4] == sig {
                let off = u32::from_be_bytes(profile[base + 4..base + 8].try_into().ok()?) as usize;
                let sz = u32::from_be_bytes(profile[base + 8..base + 12].try_into().ok()?) as usize;
                return profile.get(off..off + sz);
            }
        }
        None
    }

    /// 读取 XYZ-type 标签的三个分量（16.16 定点）
    fn xyz_tag(profile: &[u8], sig: &[u8; 4]) -> Option<[f64; 3]> {
        let data = icc_tag_data(profile, sig)?;
        if data.len() < 16 {
            return None;
        }
        let r = |i: usize| {
            u32::from_be_bytes(data[4 + i * 4..8 + i * 4].try_into().unwrap()) as f64 / 65536.0
        };
        Some([r(0), r(1), r(2)])
    }

    #[test]
    fn adobe_rgb_matrix_matches_reference() {
        // 对照 Bruce Lindbloom 公开的 Adobe RGB (1998) → XYZ(D65) 矩阵
        let m = rgb_to_xyz_matrix(
            0.6400, 0.3300, 0.2100, 0.7100, 0.1500, 0.0600, 0.3127, 0.3290,
        );
        let ref_m = [
            [0.5766690, 0.1855582, 0.1882286],
            [0.2973450, 0.6273636, 0.0752915],
            [0.0270314, 0.0706889, 0.9913375],
        ];
        for i in 0..3 {
            for j in 0..3 {
                assert!(
                    (m[i][j] - ref_m[i][j]).abs() < 5e-4,
                    "m[{i}][{j}] = {}，参考 {}",
                    m[i][j],
                    ref_m[i][j]
                );
            }
        }
    }

    #[test]
    fn adobe_rgb_profile_is_structurally_valid() {
        let p = build_adobe_rgb_profile();
        // 头部
        assert_eq!(&p[0..4], &(p.len() as u32).to_be_bytes(), "size 字段");
        assert_eq!(&p[36..40], b"acsp", "acsp 签名");
        assert_eq!(&p[16..20], b"RGB ", "色彩空间");
        assert_eq!(&p[20..24], b"XYZ ", "PCS");
        assert!(p.len() < 2000, "profile 应小巧，实际 {}", p.len());
        // 关键标签齐全且偏移合法
        for sig in [
            b"desc", b"cprt", b"wtpt", b"chad", b"rXYZ", b"gXYZ", b"bXYZ", b"rTRC", b"gTRC",
            b"bTRC",
        ] {
            assert!(
                icc_tag_data(&p, sig).is_some(),
                "缺少标签 {:?}",
                String::from_utf8_lossy(sig)
            );
        }
        // 白点 = D50
        let wtpt = xyz_tag(&p, b"wtpt").expect("wtpt");
        for (i, expect) in [0.9642, 1.0, 0.8249].iter().enumerate() {
            assert!((wtpt[i] - expect).abs() < 1e-4);
        }
        // 基色应使 R=G=B=1 映射到 D50 白点（内部一致性）
        let rx = xyz_tag(&p, b"rXYZ").expect("rXYZ");
        let gx = xyz_tag(&p, b"gXYZ").expect("gXYZ");
        let bx = xyz_tag(&p, b"bXYZ").expect("bXYZ");
        for i in 0..3 {
            let sum = rx[i] + gx[i] + bx[i];
            assert!(
                (sum - [0.9642, 1.0, 0.8249][i]).abs() < 2e-3,
                "D50 白点分量 {i} 不闭合: {sum}"
            );
        }
        // 与官方 Adobe RGB (1998) 的 D50 基色参考值对照（容差放宽，防记忆偏差误报）
        let expect_r = [0.6097, 0.3111, 0.0195];
        let expect_g = [0.2053, 0.6257, 0.0609];
        let expect_b = [0.1492, 0.0632, 0.7446];
        for i in 0..3 {
            assert!((rx[i] - expect_r[i]).abs() < 3e-3, "rXYZ[{i}] = {}", rx[i]);
            assert!((gx[i] - expect_g[i]).abs() < 3e-3, "gXYZ[{i}] = {}", gx[i]);
            assert!((bx[i] - expect_b[i]).abs() < 3e-3, "bXYZ[{i}] = {}", bx[i]);
        }
    }

    // ==================== 页面尺寸上限测试 ====================

    #[test]
    fn original_mode_page_capped_at_200_inches() {
        let orig = PageMode::Original;
        // 20000×5000 px → 20000pt > 14400pt 上限 → 等比缩到长边 14400pt
        let (w, h) = get_page_dimensions(&orig, PageSize::A4, 20000, 5000);
        let (w_pt, h_pt) = (w * f64::from(MM_TO_PT), h * f64::from(MM_TO_PT));
        assert!(w_pt <= 14400.0 + 1e-3, "宽 {}pt 应 ≤ 14400", w_pt);
        assert!((w_pt / h_pt - 4.0).abs() < 1e-3, "宽高比应保持 4:1");
        // 长图（竖版）同样等比缩放
        let (w2, h2) = get_page_dimensions(&orig, PageSize::A4, 5000, 20000);
        assert!((w2 * f64::from(MM_TO_PT) - 3600.0).abs() < 1e-3);
        assert!((h2 * f64::from(MM_TO_PT) - 14400.0).abs() < 1e-3);
        // 未超限的图不受影响
        let (w3, h3) = get_page_dimensions(&orig, PageSize::A4, 1000, 500);
        assert!((w3 * f64::from(MM_TO_PT) - 1000.0).abs() < 1e-3);
        assert!((h3 * f64::from(MM_TO_PT) - 500.0).abs() < 1e-3);
    }

    // ==================== EXIF ColorSpace 读取测试 ====================

    /// 构造一个内嵌 EXIF（含 ColorSpace + 可选 InteroperabilityIndex）的最小 JPEG。
    /// 小端 TIFF：IFD0 → Exif IFD → Interop IFD。
    fn jpeg_with_exif_color_space(color_space: u16, interop: Option<&[u8]>) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II*\0");
        tiff.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset
                                                     // IFD0: 1 entry ExifIFDPointer(0x8769) → Exif IFD
        let exif_ifd_off = 8 + 2 + 12 + 4; // 26
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x8769u16.to_le_bytes());
        tiff.extend_from_slice(&4u16.to_le_bytes()); // LONG
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&(exif_ifd_off as u32).to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0
                                                     // Exif IFD: 2 entries (ColorSpace + InteropIFDPointer)
        let interop_ifd_off = exif_ifd_off + 2 + 2 * 12 + 4; // 56
        tiff.extend_from_slice(&2u16.to_le_bytes());
        tiff.extend_from_slice(&0xA001u16.to_le_bytes()); // ColorSpace
        tiff.extend_from_slice(&3u16.to_le_bytes()); // SHORT
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&(color_space as u32).to_le_bytes());
        tiff.extend_from_slice(&0xA005u16.to_le_bytes()); // InteropIFDPointer
        tiff.extend_from_slice(&4u16.to_le_bytes()); // LONG
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&(interop_ifd_off as u32).to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes()); // next = 0
                                                     // Interop IFD: 1 entry InteroperabilityIndex(0x0001) ASCII inline
        let interop_val = interop.unwrap_or(b"R03");
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x0001u16.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes()); // ASCII
        tiff.extend_from_slice(&(interop_val.len() as u32 + 1).to_le_bytes()); // +NUL
        let mut val = [0u8; 4];
        val[..interop_val.len().min(4)].copy_from_slice(&interop_val[..interop_val.len().min(4)]);
        tiff.extend_from_slice(&val);
        tiff.extend_from_slice(&0u32.to_le_bytes()); // next = 0

        // JPEG: SOI + APP1(Exif) + EOI
        let seg_len = 6 + tiff.len() + 2;
        let mut jpg = Vec::new();
        jpg.extend_from_slice(&[0xFF, 0xD8]);
        jpg.extend_from_slice(&[0xFF, 0xE1]);
        jpg.extend_from_slice(&[((seg_len >> 8) & 0xFF) as u8, (seg_len & 0xFF) as u8]);
        jpg.extend_from_slice(b"Exif\0\0");
        jpg.extend_from_slice(&tiff);
        jpg.extend_from_slice(&[0xFF, 0xD9]);
        jpg
    }

    #[test]
    fn read_exif_color_space_detects_adobe_rgb() {
        let dir = std::env::temp_dir();
        let p = dir.join(format!("exif_cs_{}.jpg", uuid::Uuid::new_v4()));

        // ColorSpace=2 + InteropIndex R03 → adobe_rgb
        std::fs::write(&p, jpeg_with_exif_color_space(2, Some(b"R03"))).unwrap();
        assert_eq!(read_exif_color_space(&p).as_deref(), Some("adobe_rgb"));
        // ColorSpace=1 → srgb
        std::fs::write(&p, jpeg_with_exif_color_space(1, Some(b"R98"))).unwrap();
        assert_eq!(read_exif_color_space(&p).as_deref(), Some("srgb"));
        // ColorSpace=2 + 其它 InteropIndex → uncalibrated
        std::fs::write(&p, jpeg_with_exif_color_space(2, Some(b"JPN"))).unwrap();
        assert_eq!(read_exif_color_space(&p).as_deref(), Some("uncalibrated"));
        // 无 EXIF → None
        std::fs::write(
            &p,
            b"\xFF\xD8\xFF\xE0\x00\x10JFIF\0\x01\x02\0\0\x48\0\x48\0\0\xFF\xD9",
        )
        .unwrap();
        assert_eq!(read_exif_color_space(&p), None);

        let _ = std::fs::remove_file(&p);
    }

    // ==================== ICC 头部扫描（不整文件读盘） ====================

    fn temp_path(ext: &str) -> PathBuf {
        std::env::temp_dir().join(format!("toolsbag-test-{}.{}", uuid::Uuid::new_v4(), ext))
    }

    /// SOI + APP2(ICC 分片) + SOS + 大量"扫描数据" + EOI。
    /// 扫描器必须在 SOS 处停下，不能读入后面的扫描数据。
    fn fake_jpeg_icc_then_scan_data(profile: &[u8], scan_data: usize) -> Vec<u8> {
        let mut v = fake_jpeg_with_icc(profile);
        v.truncate(v.len() - 2); // 去掉尾部的 EOI
        v.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x0C]); // SOS 段头
        v.extend_from_slice(&[0u8; 8]); // SOS 载荷
        v.extend(std::iter::repeat_n(0xABu8, scan_data));
        v.extend_from_slice(&[0xFF, 0xD9]);
        v
    }

    #[test]
    fn jpeg_icc_scan_stops_at_sos_and_reads_full_profile() {
        let mut profile = vec![7u8; 300];
        profile[36..40].copy_from_slice(b"acsp");
        let path = temp_path("jpg");
        std::fs::write(&path, fake_jpeg_icc_then_scan_data(&profile, 4096)).unwrap();

        let (found, truncated) = scan_jpeg_icc(&path, ICC_MAX_BYTES).expect("应扫到 ICC");
        assert_eq!(found, profile);
        assert!(!truncated);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn jpeg_icc_scan_honours_header_budget() {
        let mut profile = vec![9u8; 300];
        profile[36..40].copy_from_slice(b"acsp");
        let path = temp_path("jpg");
        std::fs::write(&path, fake_jpeg_icc_then_scan_data(&profile, 4096)).unwrap();

        let (found, truncated) = scan_jpeg_icc(&path, 64).expect("应扫到 ICC");
        assert_eq!(found.len(), 64, "预算内只读 64 字节");
        assert!(truncated, "预算截断要如实上报");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn jpeg_icc_scan_none_without_profile() {
        let path = temp_path("jpg");
        std::fs::write(&path, fake_jpeg_with_jfif()).unwrap();
        assert!(scan_jpeg_icc(&path, ICC_MAX_BYTES).is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn icc_profile_name_parses_builtin_desc() {
        let profile = build_adobe_rgb_profile();
        assert_eq!(
            icc_profile_name(&profile).as_deref(),
            Some("Adobe RGB (1998)")
        );
    }

    #[test]
    fn icc_profile_name_parses_v4_mluc_desc() {
        let mut mluc = Vec::new();
        mluc.extend_from_slice(b"mluc");
        mluc.extend_from_slice(&0u32.to_be_bytes());
        mluc.extend_from_slice(&1u32.to_be_bytes()); // 1 条记录
        mluc.extend_from_slice(&12u32.to_be_bytes());
        mluc.extend_from_slice(b"en");
        mluc.extend_from_slice(b"US");
        let text: Vec<u16> = "P3 display".encode_utf16().collect();
        mluc.extend_from_slice(&((text.len() * 2) as u32).to_be_bytes());
        mluc.extend_from_slice(&28u32.to_be_bytes());
        for c in text {
            mluc.extend_from_slice(&c.to_be_bytes());
        }
        let mut p = vec![0u8; 128];
        p[16..20].copy_from_slice(b"RGB ");
        p[36..40].copy_from_slice(b"acsp");
        let off = 128 + 4 + 12;
        p.extend_from_slice(&1u32.to_be_bytes());
        p.extend_from_slice(b"desc");
        p.extend_from_slice(&(off as u32).to_be_bytes());
        p.extend_from_slice(&(mluc.len() as u32).to_be_bytes());
        p.extend_from_slice(&mluc);
        assert_eq!(icc_profile_name(&p).as_deref(), Some("P3 display"));
    }

    #[test]
    fn webp_icc_chunk_is_located() {
        let mut profile = vec![3u8; 180];
        profile[36..40].copy_from_slice(b"acsp");
        profile[16..20].copy_from_slice(b"RGB ");
        let mut body = Vec::new();
        body.extend_from_slice(b"WEBP");
        body.extend_from_slice(b"VP8X");
        body.extend_from_slice(&10u32.to_le_bytes());
        body.extend_from_slice(&[0u8; 10]);
        body.extend_from_slice(b"ICCP");
        body.extend_from_slice(&(profile.len() as u32).to_le_bytes());
        body.extend_from_slice(&profile);
        if profile.len() % 2 == 1 {
            body.push(0); // 奇数长度补一个字节
        }
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(body.len() as u32).to_le_bytes());
        file.extend_from_slice(&body);

        let path = temp_path("webp");
        std::fs::write(&path, &file).unwrap();
        let (off, len) = webp_icc_chunk(&path).expect("应定位到 ICCP");
        assert_eq!(len, profile.len());
        assert_eq!(
            read_range_budgeted(&path, off, len, ICC_MAX_BYTES).unwrap(),
            profile
        );
        let (has, name, _) = scan_icc_metadata(&path, "webp");
        assert!(has);
        assert_eq!(name, None, "这个假 profile 没有 desc 标签");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tiff_icc_tag_is_located() {
        let mut profile = vec![5u8; 120];
        profile[36..40].copy_from_slice(b"acsp");
        let mut file = Vec::new();
        file.extend_from_slice(b"II*\0");
        file.extend_from_slice(&8u32.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes()); // 1 个 entry
        file.extend_from_slice(&34675u16.to_le_bytes()); // ICC profile 标签
        file.extend_from_slice(&7u16.to_le_bytes()); // UNDEFINED
        file.extend_from_slice(&(profile.len() as u32).to_le_bytes());
        file.extend_from_slice(&26u32.to_le_bytes()); // 数据偏移
        file.extend_from_slice(&0u32.to_le_bytes()); // next IFD
        file.extend_from_slice(&profile);

        let path = temp_path("tif");
        std::fs::write(&path, &file).unwrap();
        let (off, len) = tiff_icc_tag(&path).expect("应定位到 ICC 标签");
        assert_eq!(len, profile.len());
        assert_eq!(
            read_range_budgeted(&path, off, len, ICC_MAX_BYTES).unwrap(),
            profile
        );
        let (has, _, _) = scan_icc_metadata(&path, "tiff");
        assert!(has);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn png_color_scan_distinguishes_iccp_from_srgb_chunk() {
        let build_png = |extra: Vec<u8>| {
            let mut png = Vec::new();
            png.extend_from_slice(&PNG_SIGNATURE);
            png.extend_from_slice(&extra);
            png.extend_from_slice(&0u32.to_be_bytes());
            png.extend_from_slice(b"IEND");
            png.extend_from_slice(&[0u8, 0, 0, 0]);
            png
        };
        // 仅 sRGB chunk：只说明「已是 sRGB」，没有可内嵌的 profile
        let mut srgb_chunk = Vec::new();
        srgb_chunk.extend_from_slice(&1u32.to_be_bytes());
        srgb_chunk.extend_from_slice(b"sRGB");
        srgb_chunk.push(0);
        srgb_chunk.extend_from_slice(&[0u8, 0, 0, 0]);
        let path = temp_path("png");
        std::fs::write(&path, build_png(srgb_chunk)).unwrap();
        let scan = scan_png_color_chunks(&path);
        assert!(scan.srgb_tagged);
        assert!(!scan.has_icc, "sRGB chunk 不等于内嵌 ICC profile");
        assert!(!scan_icc_metadata(&path, "png").0);
        let _ = std::fs::remove_file(&path);

        // 带 iCCP：应识别出内嵌 ICC 与名称
        let mut iccp_data = b"Adobe RGB (1998)".to_vec();
        iccp_data.push(0);
        iccp_data.push(0);
        iccp_data.extend_from_slice(&[0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01]);
        let mut iccp_chunk = Vec::new();
        iccp_chunk.extend_from_slice(&(iccp_data.len() as u32).to_be_bytes());
        iccp_chunk.extend_from_slice(b"iCCP");
        iccp_chunk.extend_from_slice(&iccp_data);
        iccp_chunk.extend_from_slice(&[0u8, 0, 0, 0]);
        std::fs::write(&path, build_png(iccp_chunk)).unwrap();
        let (has, name, _) = scan_icc_metadata(&path, "png");
        assert!(has);
        assert_eq!(name.as_deref(), Some("Adobe RGB (1998)"));
        let _ = std::fs::remove_file(&path);
    }

    // ==================== 批量输出命名 ====================

    #[test]
    fn pick_output_name_starts_suffix_at_one() {
        let dir = std::env::temp_dir().join(format!("toolsbag-name-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut used = HashSet::new();
        assert_eq!(pick_output_name(&dir, "照片", &mut used, false), "照片");
        assert_eq!(pick_output_name(&dir, "照片", &mut used, false), "照片 (1)");
        assert_eq!(pick_output_name(&dir, "照片", &mut used, false), "照片 (2)");
        // 大小写不敏感（Windows）
        assert_eq!(pick_output_name(&dir, "Photo", &mut used, false), "Photo");
        assert_eq!(
            pick_output_name(&dir, "photo", &mut used, false),
            "photo (1)"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pick_output_name_avoids_existing_files_unless_overwriting() {
        let dir = std::env::temp_dir().join(format!("toolsbag-name-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("报告.pdf"), "上次的产出").unwrap();

        let mut used = HashSet::new();
        assert_eq!(pick_output_name(&dir, "报告", &mut used, false), "报告 (1)");

        // 显式要求覆盖时沿用原名
        let mut used2 = HashSet::new();
        assert_eq!(pick_output_name(&dir, "报告", &mut used2, true), "报告");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sanitize_file_name_handles_reserved_and_long_names() {
        assert_eq!(sanitize_file_name("CON"), "CON_");
        assert_eq!(sanitize_file_name("com1"), "com1_");
        assert_eq!(sanitize_file_name("LPT9"), "LPT9_");
        assert_eq!(sanitize_file_name("COM0"), "COM0"); // 0 不是保留名
        assert_eq!(sanitize_file_name("a/b:c*d?"), "abcd");
        assert_eq!(sanitize_file_name("  trailing.  "), "trailing");
        assert_eq!(sanitize_file_name(""), "output");
        assert_eq!(sanitize_file_name("..."), "output");
        let long = "长".repeat(300);
        assert_eq!(sanitize_file_name(&long).chars().count(), 120);
    }

    // ==================== EXIF 方向 / CMYK / SMask ====================

    /// 真正可解码的 JPEG（w×h 纯色），SOI 后插入 EXIF APP1 设定 Orientation
    fn jpeg_file_with_orientation(w: u32, h: u32, orientation: u16) -> Vec<u8> {
        let rgb = RgbImage::from_pixel(w, h, image_crate::Rgb([120, 200, 60]));
        let mut buf = Cursor::new(Vec::new());
        let mut enc = image_crate::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 92);
        enc.encode(rgb.as_raw(), w, h, image_crate::ExtendedColorType::Rgb8)
            .unwrap();
        let jpeg = buf.into_inner();
        let exif = exif_app1_with_orientation(orientation);
        let mut out = Vec::new();
        out.extend_from_slice(&jpeg[0..2]); // SOI
        out.extend_from_slice(&exif);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    /// 构造 EXIF APP1 段（小端 TIFF，IFD0 仅一个 Orientation）
    fn exif_app1_with_orientation(orientation: u16) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II*\0");
        tiff.extend_from_slice(&8u32.to_le_bytes()); // IFD0 偏移
        tiff.extend_from_slice(&1u16.to_le_bytes()); // 1 个 entry
        tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation
        tiff.extend_from_slice(&3u16.to_le_bytes()); // SHORT
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&(orientation as u32).to_le_bytes()); // 内联值
        tiff.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0

        let seg_len = 2 + 6 + tiff.len();
        let mut out = Vec::new();
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&[(seg_len >> 8) as u8, (seg_len & 0xFF) as u8]);
        out.extend_from_slice(b"Exif\0\0");
        out.extend_from_slice(&tiff);
        out
    }

    fn original_mode_config() -> PdfConfig {
        PdfConfig {
            page_size: PageSize::A4,
            page_mode: PageMode::Original,
            margin: 0.0,
            output_path: "unused.pdf".to_string(),
            merge_mode: MergeMode::default(),
            embed_icc: None,
            color_mode: ColorMode::Preserve,
        }
    }

    #[test]
    fn exif_rotated_jpeg_does_not_get_redundant_smask() {
        let path = temp_path("jpg");
        // 4×8 竖图 + orientation=6（顺时针 90°）→ 应变成 8×4 横图
        std::fs::write(&path, jpeg_file_with_orientation(4, 8, 6)).unwrap();
        assert_eq!(read_exif_orientation(&path), Some(6));

        let pages = process_image(
            &path,
            &original_mode_config(),
            &ProcessParams::lossless(95, true),
            false,
            false,
            &ColorPlan::Passthrough,
        )
        .expect("处理应成功");

        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].processed.width, 8, "EXIF 旋转后宽高应互换");
        assert_eq!(pages[0].processed.height, 4);
        assert!(
            pages[0].processed.alpha_data.is_none(),
            "JPEG 本来没有 alpha，旋转不应凭空造出一个全不透明的 SMask"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cmyk_jpeg_with_exif_orientation_keeps_geometry() {
        // 3×5 的 CMYK JPEG + orientation=6 → 期望输出 5×3 且不被撕裂
        let raw: Vec<u8> = (0..3 * 5 * 4).map(|i| (i * 37 % 256) as u8).collect();
        let mut buf = Cursor::new(Vec::new());
        let mut enc = image_crate::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 92);
        if enc
            .encode(&raw, 3, 5, image_crate::ExtendedColorType::Cmyk8)
            .is_err()
        {
            return; // 编码器不支持 CMYK 时跳过（不同版本行为有差异）
        }
        let jpeg = buf.into_inner();
        let mut out = Vec::new();
        out.extend_from_slice(&jpeg[0..2]);
        out.extend_from_slice(&exif_app1_with_orientation(6));
        out.extend_from_slice(&jpeg[2..]);
        let path = temp_path("jpg");
        std::fs::write(&path, &out).unwrap();

        if !jpeg_is_cmyk(&path) {
            let _ = std::fs::remove_file(&path);
            return; // 编码结果不是 CMYK32，测不到这条分支
        }

        let pages = process_image(
            &path,
            &original_mode_config(),
            &ProcessParams::lossless(95, true),
            false,
            false,
            &ColorPlan::Passthrough,
        )
        .expect("CMYK + EXIF 应能处理");
        assert_eq!(pages[0].processed.width, 5, "旋转后的宽应是原高");
        assert_eq!(pages[0].processed.height, 3, "旋转后的高应是原宽");
        let _ = std::fs::remove_file(&path);
    }

    // ==================== 兼容模式：转换到 sRGB ====================

    #[test]
    fn srgb_conversion_maps_shared_primaries_and_white() {
        let profile = build_adobe_rgb_profile();

        let convert = |rgb: [u8; 3]| -> [u8; 3] {
            let mut img = RgbImage::from_pixel(1, 1, image_crate::Rgb(rgb));
            assert!(convert_rgb_to_srgb(&mut img, Some(&profile)));
            img.get_pixel(0, 0).0
        };

        // 白点必须闭合
        assert_eq!(convert([255, 255, 255]), [255, 255, 255]);
        // Adobe RGB 与 sRGB 的红/蓝基色完全相同 → 原样保留
        assert_eq!(convert([255, 0, 0]), [255, 0, 0]);
        assert_eq!(convert([0, 0, 255]), [0, 0, 255]);
        // 绿基色在 sRGB 色域外 → 红蓝被裁剪到 0，绿保持饱和
        let green = convert([0, 255, 0]);
        assert_eq!(green[0], 0);
        assert_eq!(green[2], 0);
        assert!(green[1] >= 250, "绿色应保持饱和，实际 {:?}", green);
    }

    #[test]
    fn srgb_conversion_of_an_srgb_profile_is_near_identity() {
        // 真·sRGB profile（sRGB 基色 + para type 4 曲线）：转换应几乎恒等
        let profile = build_matrix_shaper_profile(
            "Test sRGB",
            "test",
            (0.6400, 0.3300, 0.3000, 0.6000, 0.1500, 0.0600),
            0.3127,
            0.3290,
            2.2,
        );
        let profile = replace_trc_with_parametric_srgb(&profile);
        assert!(
            matches!(
                parse_rgb_matrix_profile(&profile).expect("应解析成功").trc[0],
                ToneCurve::Parametric { func: 4, .. }
            ),
            "替换后应是 para 曲线"
        );

        for color in [
            [128u8, 128, 128],
            [64, 128, 192],
            [200, 150, 100],
            [10, 20, 30],
            [255, 0, 128],
        ] {
            let mut img = RgbImage::from_pixel(1, 1, image_crate::Rgb(color));
            assert!(convert_rgb_to_srgb(&mut img, Some(&profile)));
            let out = img.get_pixel(0, 0).0;
            for c in 0..3 {
                let diff = (out[c] as i32 - color[c] as i32).abs();
                assert!(diff <= 2, "{:?} → {:?} 偏差过大（通道 {}）", color, out, c);
            }
        }
    }

    /// γ2.2 曲线与 sRGB 曲线在暗部本就不同（一个是指数、一个带线性趾），
    /// 所以用 γ2.2 profile 转 sRGB 时暗部必然有可见位移——固化这个预期。
    #[test]
    fn gamma22_profile_shifts_deep_shadows_but_keeps_midtones() {
        let profile = build_matrix_shaper_profile(
            "Test γ2.2",
            "test",
            (0.6400, 0.3300, 0.3000, 0.6000, 0.1500, 0.0600),
            0.3127,
            0.3290,
            2.2,
        );
        let convert = |rgb: [u8; 3]| -> [u8; 3] {
            let mut img = RgbImage::from_pixel(1, 1, image_crate::Rgb(rgb));
            convert_rgb_to_srgb(&mut img, Some(&profile));
            img.get_pixel(0, 0).0
        };
        assert_eq!(convert([255, 255, 255]), [255, 255, 255]);
        for v in [64u8, 128, 200] {
            let out = convert([v, v, v])[0];
            assert!(
                (out as i32 - v as i32).abs() <= 3,
                "中高调 {v} → {out} 位移过大"
            );
        }
        // 10/255 附近：γ2.2 比 sRGB 曲线更"暗"，转换后应明显变暗
        let dark = convert([10, 10, 10])[0];
        assert!(dark < 10, "暗部应被压暗，实际 {dark}");
    }

    /// 用 para type 4（sRGB 传递函数）替换 profile 里的三条 TRC
    fn replace_trc_with_parametric_srgb(profile: &[u8]) -> Vec<u8> {
        let sigs: [&[u8; 4]; 10] = [
            b"desc", b"cprt", b"wtpt", b"chad", b"rXYZ", b"gXYZ", b"bXYZ", b"rTRC", b"gTRC",
            b"bTRC",
        ];
        let para = {
            let mut d = Vec::new();
            d.extend_from_slice(b"para");
            d.extend_from_slice(&4u16.to_be_bytes()); // funcType = 4
            d.extend_from_slice(&0u16.to_be_bytes()); // reserved
            for v in [
                2.4f64,
                1.0 / 1.055,
                0.055 / 1.055,
                1.0 / 12.92,
                0.04045,
                0.0,
                0.0,
            ] {
                d.extend_from_slice(&((v * 65536.0).round() as i32).to_be_bytes());
            }
            d
        };
        let tags: Vec<([u8; 4], Vec<u8>)> = sigs
            .iter()
            .map(|sig| {
                let data = icc_tag_bytes(profile, sig).expect("tag 应存在").to_vec();
                let data = if sig[1..4] == *b"TRC" {
                    para.clone()
                } else {
                    data
                };
                (**sig, data)
            })
            .collect();
        assemble_icc_profile(tags)
    }

    #[test]
    fn srgb_conversion_is_skipped_without_profile() {
        let mut img = RgbImage::from_pixel(2, 2, image_crate::Rgb([10, 20, 30]));
        assert!(!convert_rgb_to_srgb(&mut img, None));
        assert_eq!(img.get_pixel(0, 0).0, [10, 20, 30]);
        // 非矩阵型/损坏 profile 同样不动像素
        let junk = vec![0u8; 200];
        assert!(!convert_rgb_to_srgb(&mut img, Some(&junk)));
        assert_eq!(img.get_pixel(0, 0).0, [10, 20, 30]);
    }

    #[test]
    fn parse_matrix_profile_reads_gamma_and_curve_types() {
        let profile = build_adobe_rgb_profile();
        let parsed = parse_rgb_matrix_profile(&profile).expect("应解析成功");
        match parsed.trc[0] {
            ToneCurve::Gamma(g) => assert!((g - 2.2).abs() < 0.01, "γ = {}", g),
            ref other => panic!("应为 Gamma 曲线，实际 {:?}", other),
        }
        // curv 表形式
        let table = ToneCurve::Table(vec![0, 32768, 65535]);
        assert!((table.eval(0.0) - 0.0).abs() < 1e-6);
        assert!((table.eval(1.0) - 1.0).abs() < 1e-6);
        assert!((table.eval(0.5) - 0.5).abs() < 0.01);
        // para type 4（sRGB 曲线）：输入 0.5（即 8-bit 128）→ 线性 ≈0.2140
        let srgbb = ToneCurve::Parametric {
            func: 4,
            g: 2.4,
            a: 1.0 / 1.055,
            b: 0.055 / 1.055,
            c: 1.0 / 12.92,
            d: 0.04045,
            e: 0.0,
            f: 0.0,
        };
        let v = srgbb.eval(0.5);
        assert!((v - 0.2140).abs() < 0.002, "sRGB 曲线 0.5 → {}", v);
        // 线性 1.0 → 1.0（白点闭合）
        assert!((srgbb.eval(1.0) - 1.0).abs() < 1e-6);
    }
}
