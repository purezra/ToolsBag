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

use exif::{In, Reader as ExifReader, Tag};
use image_crate::codecs::png::PngDecoder;
use image_crate::codecs::webp::WebPDecoder;
use image_crate::imageops::FilterType;
use image_crate::{
    ColorType, DynamicImage, GenericImageView, ImageBuffer, ImageDecoder, ImageFormat, ImageReader,
    RgbImage, RgbaImage,
};
use jpeg_decoder::{Decoder as RawJpegDecoder, PixelFormat};
use printpdf::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor, Read};
use std::path::{Path, PathBuf};
use tauri::Emitter;
use tiff::decoder::{Decoder as TiffDecoder, DecodingResult as TiffDecodingResult};
use walkdir::WalkDir;

const MM_TO_PT: f32 = 2.834_645_7;

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
    /// P2: 是否含 ICC 色彩配置（iCCP chunk 或 JPEG APP2 ICC）
    #[serde(default)]
    pub has_icc: bool,
    /// P2: ICC 配置描述（如 "Adobe RGB", "Display P3", "sRGB" 等）
    #[serde(default)]
    pub icc_profile: Option<String>,
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
    /// P2: 16-bit PNG 文件数（降采样到 8-bit，精度有损）
    #[serde(default)]
    pub bit16_count: usize,
    /// P2: 含 gamma 信息的文件数（PDF 不内嵌 ICC/gamma，可能有轻微色偏）
    #[serde(default)]
    pub gamma_count: usize,
    /// P2: 含 ICC 色彩配置的文件数（广色域图片可能有偏色）
    #[serde(default)]
    pub icc_count: usize,
    /// P3: 长图文件数（长宽比 > 5:1，可能生成超长页面）
    #[serde(default)]
    pub long_image_count: usize,
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

// ==================== 布局计算 ====================

fn is_portrait_image(width: u32, height: u32) -> bool {
    height > width
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
            let w = px_to_mm(img_width);
            let h = px_to_mm(img_height);
            // 横图横摆、竖图竖摆（像素尺寸本身已蕴含方向）
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
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("?").to_string();
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
    Oversized { estimated_mb: u64, width: u32, height: u32 },
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

/// 全局信号量：限制同时解码的大图数量。
/// 用 AtomicUsize 简单实现（ponytail: 足够本场景，不用 crossbeam）
static LARGE_DECODE_SEMAPHORE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn acquire_large_decode_slot() -> bool {
    use std::sync::atomic::Ordering;
    loop {
        let cur = LARGE_DECODE_SEMAPHORE.load(Ordering::Acquire);
        if cur >= MAX_CONCURRENT_LARGE_DECODES {
            return false;
        }
        if LARGE_DECODE_SEMAPHORE
            .compare_exchange_weak(cur, cur + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return true;
        }
    }
}

fn release_large_decode_slot() {
    use std::sync::atomic::Ordering;
    LARGE_DECODE_SEMAPHORE.fetch_sub(1, Ordering::AcqRel);
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
fn dimensions_after_exif_orientation(path: &Path, width: u32, height: u32) -> (u32, u32) {
    match read_exif_orientation(path) {
        Some(5..=8) => (height, width),
        _ => (width, height),
    }
}

fn analyze_image(path: &Path) -> Result<ImageAnalysis, String> {
    let format = detect_image_format(path).ok_or("不支持的图片格式")?;
    let (raw_width, raw_height) =
        image_crate::image_dimensions(path).map_err(|e| format!("无法读取图片尺寸: {}", e))?;
    let (width, height) = dimensions_after_exif_orientation(path, raw_width, raw_height);
    // 可能含 alpha 的格式；BMP/TGA/PNM/Farbfeld/DDS/ICO 大部分不含但安全起见标记
    let has_alpha = matches!(
        format.as_str(),
        "png" | "webp" | "tiff" | "gif" | "ico" | "tga" | "dds" | "farbfeld"
    );

    // P1+P2: 对 PNG 格式检测 APNG、灰度、16-bit、gamma、ICC 元数据
    let (is_apng, is_grayscale, bit_depth, has_gamma, gamma, has_icc, icc_profile) = if format == "png" {
        let meta = inspect_png_metadata(path);
        (
            meta.is_apng,
            meta.is_grayscale,
            meta.bit_depth,
            meta.has_gamma,
            meta.gamma,
            meta.has_icc,
            meta.icc_profile,
        )
    } else if format == "jpeg" {
        // JPEG: 检测 APP2 ICC_PROFILE
        let has_icc = detect_jpeg_icc(path);
        let icc_profile = detect_jpeg_icc_name(path);
        (false, false, 8, false, None, has_icc, icc_profile)
    } else {
        (false, false, 8, false, None, false, None)
    };

    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    // 多页 TIFF 需统计帧数，让预览/页数/进度与实际 PDF 页数一致
    let frame_count = if format == "tiff" {
        count_tiff_frames(path)
    } else {
        1
    };

    Ok(ImageAnalysis {
        path: path.to_string_lossy().to_string(),
        width,
        height,
        is_portrait: is_portrait_image(width, height),
        format,
        has_alpha,
        is_apng,
        is_grayscale,
        bit_depth,
        has_gamma,
        gamma,
        file_size,
        frame_count,
        has_icc,
        icc_profile,
        is_long_image: width > 0 && height > 0 && (width as f32 / height as f32 > 5.0 || height as f32 / width as f32 > 5.0),
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

fn convert_cmyk_to_rgb(path: &Path) -> Result<Vec<u8>, String> {
    let mut decoder = RawJpegDecoder::new(std::fs::File::open(path).map_err(|e| e.to_string())?);
    let pixels = decoder
        .decode()
        .map_err(|e| format!("CMYK 解码失败: {}", e))?;
    let info = decoder.info().ok_or("无法获取 JPEG 信息")?;
    if !matches!(info.pixel_format, PixelFormat::CMYK32) {
        return Err("不是 CMYK JPEG".into());
    }
    let mut rgb = Vec::with_capacity(info.width as usize * info.height as usize * 3);
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
    Ok(rgb)
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
    })
}

/// 将原始 RGB 像素数据包装为 Flate 路径的 ProcessedImage。
/// data 存放原始 RGB 行 (w*h*3 字节)，lopdf 会在 release 模式下
/// 自动做 FlateDecode 压缩（XObject::compress_stream → lopdf Stream::compress）。
fn encode_rgb_to_flate_processed(rgb: &RgbImage) -> ProcessedImage {
    let (w, h) = rgb.dimensions();
    ProcessedImage {
        data: rgb.as_raw().clone(),
        width: w,
        height: h,
        encoding: ImageEncoding::Flate,
        alpha_data: None,
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
    /// 是否有 gAMA / sRGB chunk（gamma != 1.0 / sRGB 标记）
    has_gamma: bool,
    /// gamma 值（如有）
    gamma: Option<f64>,
    /// 是否含 iCCP chunk（嵌入式 ICC 配置）
    has_icc: bool,
    /// ICC profile 名称（如 "Adobe RGB", "Display P3" 等）
    icc_profile: Option<String>,
}

/// 读取 PNG 文件的元数据（不解码像素），用于 APNG 检测、16-bit 警告、灰度优化等。
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
        has_gamma,
        gamma,
        has_icc: detect_png_icc(path),
        icc_profile: detect_png_icc_name(path),
    }
}

/// 快速扫描 JPEG APP2 marker 中是否有 ICC_PROFILE。
fn detect_jpeg_icc(path: &Path) -> bool {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return false,
    };
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return false;
    }
    let mut i = 2usize;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            break;
        }
        let marker = data[i + 1];
        // Skip padding FF bytes
        if marker == 0xFF {
            i += 1;
            continue;
        }
        // Standalone markers (no length)
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        if i + 3 >= data.len() {
            break;
        }
        let seg_len = ((data[i + 2] as usize) << 8) | (data[i + 3] as usize);
        // APP2 (0xE2) with "ICC_PROFILE\0" identifier
        if marker == 0xE2 && i + 4 + 12 < data.len() {
            let ident = &data[i + 4..i + 4 + 12];
            if ident == b"ICC_PROFILE\x00" {
                return true;
            }
        }
        // SOS marker → stop scanning (image data follows)
        if marker == 0xDA {
            break;
        }
        i += 2 + seg_len;
    }
    false
}

/// 从 JPEG APP2 ICC_PROFILE 中提取 profile 描述名称。
/// JPEG ICC 没有直接存名称，需要解析 ICC profile header 中的 'desc' tag。
/// 为简化实现，仅返回 "ICC (embedded)" 标记，让前端知道有 ICC。
fn detect_jpeg_icc_name(path: &Path) -> Option<String> {
    if detect_jpeg_icc(path) {
        // 读取 ICC profile 的前 128 字节（header），尝试提取描述
        // ICC header 偏移 80-127 可能包含 profile description
        let data = std::fs::read(path).ok()?;
        let mut i = 2usize;
        while i + 1 < data.len() {
            if data[i] != 0xFF { break; }
            let marker = data[i + 1];
            if marker == 0xFF { i += 1; continue; }
            if (0xD0..=0xD9).contains(&marker) || marker == 0x01 { i += 2; continue; }
            if i + 3 >= data.len() { break; }
            let seg_len = ((data[i + 2] as usize) << 8) | (data[i + 3] as usize);
            if marker == 0xE2 && i + 4 + 12 < data.len() {
                let ident = &data[i + 4..i + 4 + 12];
                if ident == b"ICC_PROFILE\x00" {
                    // ICC data 从 i+4+14 开始（12 bytes ident + 1 byte chunk_idx + 1 byte chunk_count）
                    let icc_start = i + 4 + 14;
                    // 读取 ICC header 的前 128 字节找 'desc' tag
                    if icc_start + 128 < data.len() {
                        let icc_header = &data[icc_start..icc_start + 128];
                        // ICC header: offset 80-127 是 date/time，描述在 tag table 里
                        // 简化：检查前 4 字节获取 profile 大小，再检查常见 profile 名称
                        let profile_size = u32::from_be_bytes([icc_header[0], icc_header[1], icc_header[2], icc_header[3]]);

                        // 常见 ICC profile 签名检测
                        // 偏移 36-39: signature "acsp" (Adobe)
                        if icc_header.len() > 43 && &icc_header[36..40] == b"acsp" {
                            // 检查偏移 41-43 的 RGB 条件
                            return Some("ICC (embedded)".to_string());
                        }
                        if profile_size > 0 {
                            return Some("ICC (embedded)".to_string());
                        }
                    }
                    return Some("ICC (embedded)".to_string());
                }
            }
            if marker == 0xDA { break; }
            i += 2 + seg_len;
        }
    }
    None
}
/// 只读 chunk header，不解码像素，开销极小。
fn detect_png_icc(path: &Path) -> bool {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    // PNG 签名 8 字节
    let mut sig = [0u8; 8];
    if std::io::Read::read(&mut file, &mut sig).is_err() || sig != [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return false;
    }
    loop {
        let mut len_buf = [0u8; 4];
        let mut type_buf = [0u8; 4];
        if std::io::Read::read(&mut file, &mut len_buf).is_err() || std::io::Read::read(&mut file, &mut type_buf).is_err() {
            break;
        }
        let chunk_len = u32::from_be_bytes(len_buf) as usize;
        let chunk_type = &type_buf;
        if chunk_type == b"iCCP" {
            return true;
        }
        if chunk_type == b"sRGB" {
            // sRGB chunk 也算色彩配置
            return true;
        }
        if chunk_type == b"IEND" {
            break;
        }
        // 跳过 chunk data + CRC (4 bytes)
        let skip = chunk_len + 4;
        if std::io::Seek::seek(&mut file, std::io::SeekFrom::Current(skip as i64)).is_err() {
            break;
        }
    }
    false
}

/// 从 iCCP chunk 中提取 profile 名称（如 "Adobe RGB", "Display P3"）。
/// 名称是 iCCP chunk data 的前段（null-terminated compressed string）。
fn detect_png_icc_name(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut sig = [0u8; 8];
    std::io::Read::read(&mut file, &mut sig).ok()?;
    if sig != [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return None;
    }
    loop {
        let mut len_buf = [0u8; 4];
        let mut type_buf = [0u8; 4];
        if std::io::Read::read(&mut file, &mut len_buf).is_err() || std::io::Read::read(&mut file, &mut type_buf).is_err() {
            break;
        }
        let chunk_len = u32::from_be_bytes(len_buf) as usize;
        let chunk_type = &type_buf;
        if chunk_type == b"sRGB" {
            return Some("sRGB".to_string());
        }
        if chunk_type == b"iCCP" {
            // iCCP data: profile name (null-terminated) + compression method (1 byte) + compressed profile
            let mut data = vec![0u8; chunk_len.min(256)]; // 只读前 256 字节拿名称
            let read_len = std::io::Read::read(&mut file, &mut data).ok()?;
            let name_end = data[..read_len].iter().position(|&b| b == 0);
            if let Some(end) = name_end {
                let name = String::from_utf8_lossy(&data[..end]).to_string();
                return Some(name);
            }
            // 读完剩余 data + CRC
            let remaining = chunk_len - read_len + 4;
            std::io::Seek::seek(&mut file, std::io::SeekFrom::Current(remaining as i64)).ok()?;
            continue;
        }
        if chunk_type == b"IEND" {
            break;
        }
        std::io::Seek::seek(&mut file, std::io::SeekFrom::Current((chunk_len + 4) as i64)).ok()?;
    }
    None
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
    /// JPEG 编码质量（仅在需要重编码时使用）
    quality: u8,
    /// 像素缩放比例（1.0 = 原尺寸；< 1.0 = 等比缩小）
    scale: f32,
}

impl ProcessParams {
    fn lossless(quality: u8, passthrough: bool) -> Self {
        Self {
            passthrough_jpeg: passthrough,
            quality,
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
            quality,
            scale,
        }
    }

    fn label(&self) -> String {
        if self.passthrough_jpeg && (self.scale - 1.0).abs() < 1e-3 {
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
    /// - Dct: JPEG 字节
    /// - Flate: 原始 RGB 像素行（lopdf 会在 release 模式下自动 FlateDecode 压缩）
    data: Vec<u8>,
    width: u32,
    height: u32,
    encoding: ImageEncoding,
    /// Alpha 通道数据（DeviceGray，8-bit），用于 PDF SMask。
    /// Some(alpha_bytes) → PDF 中 RGB 作为主图像，Alpha 作为 SMask 子图像。
    /// None → 图像无透明通道，直接嵌入 RGB。
    alpha_data: Option<Vec<u8>>,
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
fn jpeg_passthrough(path: &Path) -> Result<ProcessedImage, String> {
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
    Ok(ProcessedImage {
        data,
        width: w,
        height: h,
        encoding: ImageEncoding::Dct,
        alpha_data: None,
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

        let mut rgba_image = if let Some(alpha_data) = alpha {
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
        rgba_image = apply_orientation_value(rgba_image, exif_orientation);
        if (params.scale - 1.0).abs() > 1e-3 {
            rgba_image = resize_dynamic(&rgba_image, params.scale);
        }
        let rgb = if has_alpha_channel(&rgba_image) {
            flatten_rgba_to_white(&rgba_image.to_rgba8())
        } else {
            rgba_image.to_rgb8()
        };

        let processed = encode_rgb_to_jpeg_processed(&rgb, params.quality)?;
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
) -> Result<Vec<PreparedPage>, String> {
    // ---- 内存预检：读取尺寸但不解码，超大图直接拒绝 ----
    let (raw_w, raw_h) = match image_crate::image_dimensions(path) {
        Ok((w, h)) => (w, h),
        Err(e) => return Err(format!("无法读取图片尺寸 {}: {}", path.display(), e)),
    };
    let (w, h) = dimensions_after_exif_orientation(path, raw_w, raw_h);
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

    // ---- JPEG passthrough 路径 ----
    if matches!(guessed_format, ImageFormat::Jpeg)
        && params.passthrough_jpeg
        && (params.scale - 1.0).abs() < 1e-3
        && !jpeg_is_cmyk(path)
        && read_exif_orientation(path).is_none()
    {
        let processed = jpeg_passthrough(path)?;
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

    // ---- 通用路径：解码 → EXIF → CMYK 转换 → 缩放 → 透明合成 → 编码 ----
    let mut img = decode_image_fast(path, guessed_format)?;
    img = apply_exif_orientation(img, path);

    let (orig_w, orig_h) = img.dimensions();
    if matches!(guessed_format, ImageFormat::Jpeg) && jpeg_is_cmyk(path) {
        let rgb = convert_cmyk_to_rgb(path)?;
        let rgb_img =
            image_crate::RgbImage::from_raw(orig_w, orig_h, rgb).ok_or("CMYK 转换失败")?;
        img = DynamicImage::ImageRgb8(rgb_img);
    }

    if (params.scale - 1.0).abs() > 1e-3 {
        img = resize_dynamic(&img, params.scale);
    }

    let (w, h) = img.dimensions();
    // 透明通道处理：
    // - 方形/接近方形（|w-h| < 4px）透明图 → RGB + Alpha 分离，Alpha 走 PDF SMask（真正保留透明）
    // - 非方形透明图 → 退回白底合成（printpdf 0.7 的 SMask height 用 img.width，非方形会尺寸错位）
    //   ponytail: 上游 bug，方形时 width==height 无影响；非方形用白底兜底避免输出损坏 PDF
    let (rgb, alpha_data) = if has_alpha_channel(&img) {
        let is_square = (w as i32 - h as i32).abs() < 4;
        if is_square {
            let (r, a) = split_rgba_to_rgb_alpha(&img.to_rgba8());
            (r, Some(a))
        } else {
            // 非方形透明图：白底合成（fallback），透明丢失但 PDF 不损坏
            (flatten_rgba_to_white(&img.to_rgba8()), None)
        }
    } else {
        (img.to_rgb8(), None)
    };

    // ---- PNG 内容自适应编码分流 ----
    // 仅对 PNG 格式做分类；其他格式一律走 JPEG
    let mut processed = if matches!(guessed_format, ImageFormat::Png) {
        let encoding = classify_png_content(&rgb);
        match encoding {
            ImageEncoding::Flate => {
                // 线条艺术 / 索引色内容：走 FlateDecode 无损路径
                // 双编码比较：同时生成 JPEG 和 Flate，取体积更小者
                let flate_processed = encode_rgb_to_flate_processed(&rgb);
                let jpeg_result = encode_rgb_to_jpeg_processed(&rgb, params.quality);
                match jpeg_result {
                    Ok(jpeg_processed) => {
                        // 取体积更小者；若 JPEG 更小则用 JPEG（照片误判为线条艺术时的安全网）
                        if jpeg_processed.data.len() < flate_processed.data.len() {
                            jpeg_processed
                        } else {
                            flate_processed
                        }
                    }
                    Err(_) => flate_processed,
                }
            }
            ImageEncoding::Dct => {
                // 照片类内容：走 JPEG DCTDecode 路径
                encode_rgb_to_jpeg_processed(&rgb, params.quality)?
            }
        }
    } else {
        encode_rgb_to_jpeg_processed(&rgb, params.quality)?
    };
    // 附加 Alpha 数据到 ProcessedImage（SMask 会在 embed_page 时写入 PDF）
    processed.alpha_data = alpha_data;

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

fn embed_page(
    doc: &PdfDocumentReference,
    page: PdfPageIndex,
    layer: PdfLayerIndex,
    prepared: &PreparedPage,
) -> Result<(), String> {
    let current_layer = doc.get_page(page).get_layer(layer);
    let processed = &prepared.processed;

    // 构造 SMask（Alpha 通道）：printpdf 的 SMask.matte 存储灰度 alpha 值
    let smask = processed.alpha_data.as_ref().map(|alpha| SMask {
        width: processed.width as i64,
        height: processed.height as i64,
        bits_per_component: 8,
        interpolate: true,
        matte: alpha.iter().map(|&b| b as i64).collect(),
    });

    let image_xobject = match processed.encoding {
        ImageEncoding::Dct => ImageXObject {
            width: Px(processed.width as usize),
            height: Px(processed.height as usize),
            color_space: ColorSpace::Rgb,
            bits_per_component: ColorBits::Bit8,
            interpolate: true,
            image_data: processed.data.clone(),
            image_filter: Some(ImageFilter::DCT),
            clipping_bbox: None,
            smask,
        },
        ImageEncoding::Flate => {
            ImageXObject {
                width: Px(processed.width as usize),
                height: Px(processed.height as usize),
                color_space: ColorSpace::Rgb,
                bits_per_component: ColorBits::Bit8,
                interpolate: true,
                image_data: processed.data.clone(),
                image_filter: None,
                clipping_bbox: None,
                smask,
            }
        }
    };

    let pdf_image = Image::from(image_xobject);

    let transform = ImageTransform {
        translate_x: Some(Mm(prepared.layout.x as f32)),
        translate_y: Some(Mm(prepared.layout.y as f32)),
        scale_x: Some((prepared.layout.scaled_width as f32 * MM_TO_PT) / processed.width as f32),
        scale_y: Some((prepared.layout.scaled_height as f32 * MM_TO_PT) / processed.height as f32),
        rotate: None,
        dpi: Some(72.0),
    };

    pdf_image.add_to_layer(current_layer, transform);
    Ok(())
}

/// 对一组图片应用一组参数，返回 (按图片顺序排好的 prepared pages, sum of processed bytes)
type ProcessAllResult = (Vec<Vec<PreparedPage>>, u64, Vec<String>);

fn process_all(
    images: &[ImageAnalysis],
    config: &PdfConfig,
    params: &ProcessParams,
    window: &tauri::WebviewWindow,
    phase_label: &str,
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
            // 大图并发限制
            let is_large = img.width as u64 * img.height as u64 * 4 > 50 * 1024 * 1024;
            let slot = if is_large {
                while !acquire_large_decode_slot() {
                    std::thread::yield_now();
                }
                true
            } else {
                false
            };

            let result = process_image(Path::new(&img.path), config, params);

            if slot {
                release_large_decode_slot();
            }

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
                    warnings.lock().unwrap().push(format!("{}: {}", img.path, e));
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

fn write_pdf(
    pages_per_image: &[Vec<PreparedPage>],
    output_path: &Path,
    window: &tauri::WebviewWindow,
) -> Result<u64, String> {
    let total_pages: usize = pages_per_image.iter().map(|v| v.len()).sum();
    let mut doc: Option<PdfDocumentReference> = None;
    let mut page_counter = 0usize;

    for prepared_pages in pages_per_image {
        for prepared in prepared_pages {
            if doc.is_none() {
                let (d, page, layer) = PdfDocument::new(
                    "Image to PDF",
                    Mm(prepared.page_width as f32),
                    Mm(prepared.page_height as f32),
                    "Layer 1",
                );
                embed_page(&d, page, layer, prepared)?;
                doc = Some(d);
            } else if let Some(ref d) = doc {
                let (page, layer) = d.add_page(
                    Mm(prepared.page_width as f32),
                    Mm(prepared.page_height as f32),
                    "Layer 1",
                );
                embed_page(d, page, layer, prepared)?;
            }
            page_counter += 1;
            let _ = window.emit(
                "pdf_progress",
                GenerationProgress {
                    current: page_counter,
                    total: total_pages,
                    current_file: String::new(),
                    phase: "写入页面".to_string(),
                },
            );
        }
    }

    let doc = doc.ok_or_else(|| "没有有效的图片可以处理".to_string())?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = File::create(output_path).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);
    doc.save(&mut writer)
        .map_err(|e| format!("保存PDF失败: {}", e))?;

    Ok(std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0))
}

// ==================== Tauri Commands ====================

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

        let scan = scan_folder(&folder, effective_recursive)?;
        if scan.images.is_empty() && scan.skipped.is_empty() {
            return Err("文件夹中没有图片文件".to_string());
        }
        if scan.images.is_empty() {
            let exts: Vec<String> = scan.skipped.iter().map(|s| format!(".{}", s.ext)).collect();
            return Err(format!(
                "文件夹中只有不支持的格式（{}），请先转换为 JPG/PNG/WebP 等格式",
                exts.join("、")
            ));
        }

        let results: Vec<Result<ImageAnalysis, String>> =
            scan.images.par_iter().map(|p| analyze_image(p)).collect();

        let mut images = Vec::new();
        let mut portrait_count = 0usize;
        let mut landscape_count = 0usize;
        let mut square_count = 0usize;
        let mut total_size = 0u64;
        let mut apng_count = 0usize;
        let mut bit16_count = 0usize;
        let mut gamma_count = 0usize;
        let mut icc_count = 0usize;
        let mut bucket_counts: std::collections::BTreeMap<u32, (&'static str, usize)> =
            std::collections::BTreeMap::new();

        for (path, res) in scan.images.iter().zip(results.into_iter()) {
            if let Ok(analysis) = res {
                match classify_orientation(analysis.width, analysis.height) {
                    OrientationClass::Portrait => portrait_count += 1,
                    OrientationClass::Landscape => landscape_count += 1,
                    OrientationClass::Square => square_count += 1,
                }
                let (label, min_edge) = classify_resolution(analysis.width, analysis.height);
                let entry = bucket_counts.entry(min_edge).or_insert((label, 0));
                entry.1 += 1;

                // P1+P2: 统计 APNG、16-bit、gamma、ICC 文件数
                if analysis.is_apng {
                    apng_count += 1;
                }
                if analysis.bit_depth > 8 {
                    bit16_count += 1;
                }
                if analysis.has_gamma {
                    gamma_count += 1;
                }
                if analysis.has_icc {
                    icc_count += 1;
                }

                if let Ok(meta) = std::fs::metadata(path) {
                    total_size += meta.len();
                }
                images.push(analysis);
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

        Ok(FolderAnalysis {
            images,
            effective_path: folder.to_string_lossy().to_string(),
            recursive: effective_recursive,
            auto_switched_from_file: auto_switched,
            portrait_count,
            landscape_count,
            square_count,
            suggested_orientation,
            total_size,
            resolution_buckets,
            skipped: scan.skipped,
            apng_count,
            bit16_count,
            gamma_count,
            icc_count,
            long_image_count,
        })
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

#[tauri::command]
pub async fn generate_pdf(
    images: Vec<ImageAnalysis>,
    config: PdfConfig,
    window: tauri::WebviewWindow,
) -> Result<GenerationResult, String> {
    tokio::task::spawn_blocking(move || {
        use std::time::Instant;
        let start = Instant::now();

        let total = images.len();
        if total == 0 {
            return Err("没有图片可以处理".into());
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
            MergeMode::Lossless { .. } => vec![
                ProcessParams::lossless(95, true),
                ProcessParams::lossless(85, false),
                ProcessParams::lossless(75, false),
            ],
            MergeMode::Portable { target_ratio } => vec![ProcessParams::portable(target_ratio)],
        };

        let max_ratio = match config.merge_mode {
            MergeMode::Lossless { max_size_ratio } => max_size_ratio,
            MergeMode::Portable { .. } => f32::MAX, // portable 模式不强制阈值
        };

        let target_max_bytes = (original_total_size as f64 * max_ratio as f64) as u64;

        let output_path = PathBuf::from(&config.output_path);
        let mut last_pages: Option<Vec<Vec<PreparedPage>>> = None;
        let mut last_params: Option<ProcessParams> = None;
        let mut all_warnings: Vec<String> = Vec::new();

        let _ = window.emit(
            "pdf_progress",
            GenerationProgress {
                current: 0,
                total,
                current_file: String::new(),
                phase: "处理图片".to_string(),
            },
        );

        let mut attempt_idx = 0usize;
        for params in &attempts {
            attempt_idx += 1;
            let phase = if attempts.len() > 1 {
                format!("处理图片 ({}: {})", attempt_idx, params.label())
            } else {
                format!("处理图片 ({})", params.label())
            };
            let (pages, processed_bytes, attempt_warnings) = process_all(&images, &config, params, &window, &phase)?;
            last_pages = Some(pages);
            last_params = Some(*params);
            all_warnings.extend(attempt_warnings);

            // 仅 Lossless 模式才需要看阈值；超出则用下一档参数重试
            if matches!(config.merge_mode, MergeMode::Lossless { .. })
                && processed_bytes > target_max_bytes
                && attempt_idx < attempts.len()
            {
                continue;
            }
            break;
        }

        let pages = last_pages.ok_or_else(|| "处理失败：未生成任何页面".to_string())?;
        let params = last_params.expect("params set");
        let total_page_count: usize = pages.iter().map(|v| v.len()).sum();

        let file_size = write_pdf(&pages, &output_path, &window)?;
        drop(pages); // 释放图像缓冲区

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
        PixelFormat::RGB24 => {
            ImageBuffer::from_raw(w, h, pixels).map(DynamicImage::ImageRgb8)
        }
        PixelFormat::L8 => ImageBuffer::from_raw(w, h, pixels).map(DynamicImage::ImageLuma8),
        _ => None,
    }
}

#[tauri::command]
pub async fn get_image_thumbnail(image_path: String, max_size: u32) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&image_path);
        let limit = max_size.clamp(256, 1024);
        let guessed_format = ImageReader::open(&path)
            .and_then(|r| r.with_guessed_format())
            .map(|r| r.format().unwrap_or(ImageFormat::Png))
            .unwrap_or(ImageFormat::Png);

        // JPEG 优先走 DCT 缩放解码；TIFF 走 image_crate::open（多页只取第一页）；
        // 其余格式走 fast 路径
        let img = if matches!(guessed_format, ImageFormat::Jpeg) {
            match decode_jpeg_scaled(&path, limit) {
                Some(img) => img,
                None => decode_image_fast(&path, guessed_format)?,
            }
        } else if matches!(guessed_format, ImageFormat::Tiff) {
            image_crate::open(&path).map_err(|e| e.to_string())?
        } else {
            decode_image_fast(&path, guessed_format)?
        };

        // 应用 EXIF 旋转，保证缩略图方向与最终 PDF 一致
        let img = apply_exif_orientation(img, &path);
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

        let base64_data = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            buffer.into_inner(),
        );
        Ok(format!("data:image/jpeg;base64,{}", base64_data))
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
            is_grayscale: false,
            bit_depth: 8,
            has_gamma: false,
            gamma: None,
            file_size: 0,
            frame_count: 1,
            has_icc: false,
            icc_profile: None,
            is_long_image: width > 0 && height > 0 && (width as f32 / height as f32 > 5.0 || height as f32 / width as f32 > 5.0),
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
        // data 应为原始 RGB 像素行：32 * 16 * 3 = 1536 字节
        assert_eq!(processed.data.len(), 32 * 16 * 3);
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
}
