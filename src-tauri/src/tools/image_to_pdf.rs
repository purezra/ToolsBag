//! 图片合并PDF工具 - Rust后端核心模块
//!
//! 功能：
//! - 支持 JPG/JPEG、PNG、WebP 格式
//! - 透明通道正确处理（flatten_to_white / preserve）
//! - 页面尺寸：A4、A3、B5、iPad Pro
//! - 页面方向：横版、竖版、自由
//! - 边距系统
//! - 预览与最终输出一致性
//!
//! 重要：图像嵌入PDF时必须重新编码为JPEG/PNG压缩流，禁止原始RGBA位图

extern crate image as image_crate;

use exif::{In, Reader as ExifReader, Tag};
use image_crate::codecs::webp::WebPDecoder;
use image_crate::imageops::FilterType;
use image_crate::{
    ColorType, DynamicImage, GenericImage, GenericImageView, ImageBuffer, ImageDecoder,
    ImageFormat, ImageReader, RgbImage, RgbaImage,
};
use jpeg_decoder::{Decoder as RawJpegDecoder, PixelFormat};
use printpdf::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor};
use std::path::{Path, PathBuf};
use tauri::Emitter;
use tiff::decoder::{Decoder as TiffDecoder, DecodingResult as TiffDecodingResult};

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
    /// 返回页面尺寸（宽度, 高度）单位 mm，默认竖版
    pub fn dimensions(&self) -> (f64, f64) {
        match self {
            PageSize::A4 => (210.0, 297.0),
            PageSize::A3 => (297.0, 420.0),
            PageSize::B5 => (176.0, 250.0),
            PageSize::IPadPro => (160.4, 232.8),
        }
    }

    /// 短边长度
    pub fn short_edge(&self) -> f64 {
        let (w, h) = self.dimensions();
        w.min(h)
    }
}

/// 页面方向
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PageOrientation {
    #[serde(rename = "portrait")]
    Portrait,
    #[serde(rename = "landscape")]
    Landscape,
    #[serde(rename = "auto")]
    Auto,
}

/// 透明通道处理模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum TransparentMode {
    #[serde(rename = "flatten_to_white")]
    #[default]
    FlattenToWhite, // 预合成到白底（推荐，最兼容）
    #[serde(rename = "preserve")]
    Preserve, // 保留透明通道（PNG）
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
}

/// 文件夹分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderAnalysis {
    pub images: Vec<ImageAnalysis>,
    pub portrait_count: usize,
    pub landscape_count: usize,
    pub suggested_orientation: String,
    pub total_size: u64,
}

/// 布局计算结果（单页）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutResult {
    pub page_width: f64,
    pub page_height: f64,
    pub margin: f64,
    pub image: ImageLayout,
}

/// 图片布局信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageLayout {
    pub original_width: u32,
    pub original_height: u32,
    pub scaled_width: f64,
    pub scaled_height: f64,
    pub x: f64,
    pub y: f64,
}

/// 预览数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewData {
    pub pages: Vec<LayoutResult>,
    pub page_size: PageSize,
    pub orientation: PageOrientation,
    pub margin: f64,
}

/// PDF生成配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfConfig {
    pub page_size: PageSize,
    pub orientation: PageOrientation,
    pub margin: f64,
    pub compression: u8, // 0-100, 100=不压缩
    pub output_path: String,
    #[serde(default)]
    pub transparent_mode: TransparentMode,
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8, // JPEG质量 1-100，默认90
}

fn default_jpeg_quality() -> u8 {
    90
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
    pub error: Option<String>,
}

// ==================== 核心算法 ====================

fn is_portrait_image(width: u32, height: u32) -> bool {
    height > width
}

fn get_page_dimensions(
    page_size: PageSize,
    orientation: PageOrientation,
    img_width: u32,
    img_height: u32,
) -> (f64, f64) {
    let (base_w, base_h) = page_size.dimensions();
    match orientation {
        PageOrientation::Portrait => (base_w, base_h),
        PageOrientation::Landscape => (base_h, base_w),
        PageOrientation::Auto => {
            if is_portrait_image(img_width, img_height) {
                (base_w, base_h)
            } else {
                (base_h, base_w)
            }
        }
    }
}

fn get_draw_area(page_width: f64, page_height: f64, margin: f64) -> (f64, f64) {
    (page_width - 2.0 * margin, page_height - 2.0 * margin)
}

fn validate_margin(margin: f64, page_size: PageSize) -> Result<f64, String> {
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
        // 画布更瘦长：按宽度缩放，垂直居中
        let scale = w_draw / img_width as f64;
        let scaled_width = img_width as f64 * scale;
        let scaled_height = img_height as f64 * scale;
        let offset_y = (h_draw - scaled_height) / 2.0;
        (scaled_width, scaled_height, margin, margin + offset_y)
    } else {
        // 图片更瘦长：按高度缩放，水平居中
        let scale = h_draw / img_height as f64;
        let scaled_width = img_width as f64 * scale;
        let scaled_height = img_height as f64 * scale;
        let offset_x = (w_draw - scaled_width) / 2.0;
        (scaled_width, scaled_height, margin + offset_x, margin)
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
    page_size: PageSize,
    orientation: PageOrientation,
    margin: f64,
) -> LayoutResult {
    let (page_width, page_height) =
        get_page_dimensions(page_size, orientation, image.width, image.height);
    let image_layout =
        calculate_image_layout(image.width, image.height, page_width, page_height, margin);
    LayoutResult {
        page_width,
        page_height,
        margin,
        image: image_layout,
    }
}

// ==================== 图片处理 ====================

fn detect_image_format(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => Some("jpeg".to_string()),
        "png" => Some("png".to_string()),
        "webp" => Some("webp".to_string()),
        "tif" | "tiff" => Some("tiff".to_string()),
        _ => None,
    }
}

/// 检查DynamicImage是否有alpha通道
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

/// 将RGBA图像预合成到白底RGB
/// 公式: output_rgb = src_rgb * alpha + 255 * (1 - alpha)
fn flatten_rgba_to_white(rgba: &RgbaImage) -> RgbImage {
    let (w, h) = rgba.dimensions();
    let mut rgb = RgbImage::new(w, h);

    // 使用并行处理加速透明通道合成（利用多核CPU）
    use rayon::prelude::*;
    rgb.par_chunks_mut(w as usize * 3)
        .enumerate()
        .for_each(|(y, chunk)| {
            for x in 0..w as usize {
                let pixel = rgba.get_pixel(x as u32, y as u32);
                let [r, g, b, a] = pixel.0;
                let alpha = a as f32 / 255.0;
                let inv_alpha = 1.0 - alpha;

                let out_r = (r as f32 * alpha + 255.0 * inv_alpha) as u8;
                let out_g = (g as f32 * alpha + 255.0 * inv_alpha) as u8;
                let out_b = (b as f32 * alpha + 255.0 * inv_alpha) as u8;

                let idx = x * 3;
                chunk[idx] = out_r;
                chunk[idx + 1] = out_g;
                chunk[idx + 2] = out_b;
            }
        });

    rgb
}

fn analyze_image(path: &Path) -> Result<ImageAnalysis, String> {
    let format = detect_image_format(path).ok_or("不支持的图片格式")?;
    let (width, height) =
        image_crate::image_dimensions(path).map_err(|e| format!("无法读取图片尺寸: {}", e))?;
    let has_alpha = matches!(format.as_str(), "png" | "webp" | "tiff");

    Ok(ImageAnalysis {
        path: path.to_string_lossy().to_string(),
        width,
        height,
        is_portrait: is_portrait_image(width, height),
        format,
        has_alpha,
    })
}

fn decode_image_fast(path: &Path, guessed_format: ImageFormat) -> Result<DynamicImage, String> {
    if matches!(guessed_format, ImageFormat::WebP) {
        // WebP 解码加速路径
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

    ImageReader::open(path)
        .map_err(|e| format!("无法打开图片 {}: {}", path.display(), e))?
        .with_guessed_format()
        .map_err(|e| format!("格式检测失败 {}", e))?
        .decode()
        .map_err(|e| format!("图片解码失败: {}", e))
}

fn scan_folder(folder_path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut images = Vec::new();
    for entry in std::fs::read_dir(folder_path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext = ext.to_str().unwrap_or("").to_lowercase();
                if matches!(
                    ext.as_str(),
                    "jpg" | "jpeg" | "png" | "webp" | "tif" | "tiff"
                ) {
                    images.push(path);
                }
            }
        }
    }
    images.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    Ok(images)
}

fn create_thumbnail(img: &DynamicImage, max_size: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    if w <= max_size && h <= max_size {
        return img.clone();
    }
    let scale = if w > h {
        max_size as f64 / w as f64
    } else {
        max_size as f64 / h as f64
    };
    let new_w = (w as f64 * scale) as u32;
    let new_h = (h as f64 * scale) as u32;

    // 缩略图使用Triangle滤镜，速度更快
    img.resize(new_w, new_h, FilterType::Triangle)
}

fn apply_compression(img: &DynamicImage, compression: u8) -> DynamicImage {
    if compression >= 100 {
        return img.clone();
    }
    let scale = (compression as f64 / 100.0).sqrt();
    let (w, h) = img.dimensions();
    let new_w = ((w as f64 * scale) as u32).max(1);
    let new_h = ((h as f64 * scale) as u32).max(1);

    // 使用Triangle滤镜（双三次插值），比Lanczos3快2-3倍，质量损失极小
    img.resize(new_w, new_h, FilterType::Triangle)
}

fn is_heic(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext = ext.to_ascii_lowercase();
        return ext == "heic" || ext == "heif";
    }
    false
}

fn apply_exif_orientation(img: DynamicImage, path: &Path) -> DynamicImage {
    if let Ok(file) = std::fs::File::open(path) {
        let mut reader = BufReader::new(file);
        if let Ok(exif) = ExifReader::new().read_from_container(&mut reader) {
            if let Some(orientation) = exif
                .get_field(Tag::Orientation, In::PRIMARY)
                .and_then(|f| f.value.get_uint(0))
            {
                let base = img.to_rgba8();
                let rotated = match orientation {
                    2 => image_crate::imageops::flip_horizontal(&base),
                    3 => image_crate::imageops::rotate180(&base),
                    4 => image_crate::imageops::flip_vertical(&base),
                    5 => image_crate::imageops::flip_horizontal(&image_crate::imageops::rotate90(
                        &base,
                    )),
                    6 => image_crate::imageops::rotate90(&base),
                    7 => image_crate::imageops::flip_horizontal(&image_crate::imageops::rotate270(
                        &base,
                    )),
                    8 => image_crate::imageops::rotate270(&base),
                    _ => base,
                };
                return DynamicImage::ImageRgba8(rotated);
            }
        }
    }
    img
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

fn encode_rgb_to_processed(rgb: &RgbImage, quality: u8) -> Result<ProcessedImage, String> {
    let (w, h) = rgb.dimensions();
    let mut buffer = Cursor::new(Vec::new());
    let mut encoder =
        image_crate::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, quality);
    encoder
        .encode(rgb.as_raw(), w, h, image_crate::ExtendedColorType::Rgb8)
        .map_err(|e| format!("JPEG编码失败: {}", e))?;

    Ok(ProcessedImage {
        data: buffer.into_inner(),
        width: w,
        height: h,
        filter: ImageFilter::DCT,
        color_space: ColorSpace::Rgb,
        bits: ColorBits::Bit8,
    })
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

fn handle_multipage_tiff(path: &Path, config: &PdfConfig) -> Result<Vec<PreparedPage>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut decoder = TiffDecoder::new(file).map_err(|e| e.to_string())?;
    let mut pages = Vec::new();

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

        let mut r#dyn = DynamicImage::ImageRgb8(rgb_image);
        if config.compression < 100 {
            r#dyn = apply_compression(&r#dyn, config.compression);
        }
        let (dw, dh) = r#dyn.dimensions();
        let rgb = if let Some(alpha_data) = alpha {
            let mut rgba = RgbaImage::new(dw, dh);
            rgba.copy_from(&r#dyn.to_rgba8(), 0, 0).ok();
            for (p, a) in rgba.pixels_mut().zip(alpha_data.iter()) {
                p.0[3] = *a;
            }
            flatten_rgba_to_white(&rgba)
        } else {
            r#dyn.to_rgb8()
        };

        let processed = encode_rgb_to_processed(&rgb, config.jpeg_quality)?;
        let (page_width, page_height) = get_page_dimensions(
            config.page_size,
            config.orientation,
            processed.width,
            processed.height,
        );
        let layout = calculate_image_layout(
            processed.width,
            processed.height,
            page_width,
            page_height,
            validate_margin(config.margin, config.page_size)?,
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

// ==================== PDF图像嵌入（核心修复） ====================

/// 处理后的图像数据
struct ProcessedImage {
    /// 编码后的图像字节流（已压缩）
    data: Vec<u8>,
    /// 图像宽度
    width: u32,
    /// 图像高度
    height: u32,
    /// PDF流过滤器
    filter: ImageFilter,
    /// 色彩空间
    color_space: ColorSpace,
    /// 每组位数
    bits: ColorBits,
}

struct PreparedPage {
    processed: ProcessedImage,
    layout: ImageLayout,
    page_width: f64,
    page_height: f64,
}

/// 处理单张图像：解码 -> 可选resize -> 透明处理 -> 编码为JPEG/PNG
fn process_image_for_pdf(path: &Path, config: &PdfConfig) -> Result<Vec<PreparedPage>, String> {
    if is_heic(path) {
        return Err("HEIC/HEIF 格式暂不支持，请先转换为 JPEG/PNG/WebP".to_string());
    }

    let guessed_format = ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .map(|r| r.format().unwrap_or(ImageFormat::Png))
        .unwrap_or(ImageFormat::Png);

    // 多页 TIFF 特殊处理
    if matches!(guessed_format, ImageFormat::Tiff) {
        return handle_multipage_tiff(path, config);
    }

    // 使用快速解码路径（WebP）+ 通用解码
    let mut img = decode_image_fast(path, guessed_format)?;

    // 应用 EXIF 旋转
    img = apply_exif_orientation(img, path);

    let (orig_w, orig_h) = img.dimensions();
    let megapixels = (orig_w as u64 * orig_h as u64) / 1_000_000;
    if megapixels > 100 {
        eprintln!(
            "警告: 图片 {} 超过100MP ({}MP)，建议降低压缩级别",
            path.display(),
            megapixels
        );
    }

    // JPEG CMYK 转 RGB（避免色偏）
    if matches!(guessed_format, ImageFormat::Jpeg) && jpeg_is_cmyk(path) {
        let rgb = convert_cmyk_to_rgb(path)?;
        let rgb_img =
            image_crate::RgbImage::from_raw(orig_w, orig_h, rgb).ok_or("CMYK 转换失败")?;
        img = DynamicImage::ImageRgb8(rgb_img);
    }

    // 压缩=等比缩放
    if config.compression < 100 {
        img = apply_compression(&img, config.compression);
    }

    // 统一处理透明：flatten to white。preserve 模式暂时视为 flatten 并提示
    if config.transparent_mode == TransparentMode::Preserve {
        eprintln!("提示: 当前版本不提供透明保留，已自动改为白底合成。");
    }

    let (w, h) = img.dimensions();
    let rgb = if has_alpha_channel(&img) {
        flatten_rgba_to_white(&img.to_rgba8())
    } else {
        img.to_rgb8()
    };

    let processed = encode_rgb_to_processed(&rgb, config.jpeg_quality)?;

    let (page_width, page_height) = get_page_dimensions(config.page_size, config.orientation, w, h);
    let layout = calculate_image_layout(
        w,
        h,
        page_width,
        page_height,
        validate_margin(config.margin, config.page_size)?,
    );

    Ok(vec![PreparedPage {
        processed,
        layout,
        page_width,
        page_height,
    }])
}

/// 将编码后的图像嵌入PDF页面
fn embed_encoded_image_to_page(
    doc: &PdfDocumentReference,
    page: PdfPageIndex,
    layer: PdfLayerIndex,
    prepared: &PreparedPage,
) -> Result<(), String> {
    let current_layer = doc.get_page(page).get_layer(layer);
    let processed = &prepared.processed;

    let image_xobject = ImageXObject {
        width: Px(processed.width as usize),
        height: Px(processed.height as usize),
        color_space: processed.color_space,
        bits_per_component: processed.bits,
        interpolate: true,
        image_data: processed.data.clone(),
        image_filter: Some(processed.filter),
        clipping_bbox: None,
        smask: None,
    };

    let pdf_image = Image::from(image_xobject);

    // 计算变换（以 mm 为基准 -> pt；dpi 固定 72 确保比例准确）
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

// ==================== Tauri Commands ====================

#[tauri::command]
pub async fn analyze_folder_for_pdf(folder_path: String) -> Result<FolderAnalysis, String> {
    tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&folder_path);
        if !path.exists() || !path.is_dir() {
            return Err("文件夹不存在".to_string());
        }

        let image_paths = scan_folder(&path)?;
        if image_paths.is_empty() {
            return Err("文件夹中没有支持的图片文件".to_string());
        }

        let results: Vec<Result<ImageAnalysis, String>> =
            image_paths.par_iter().map(|p| analyze_image(p)).collect();

        let mut images = Vec::new();
        let mut portrait_count = 0;
        let mut landscape_count = 0;
        let mut total_size = 0u64;

        for analysis in results.into_iter().flatten() {
            if analysis.is_portrait {
                portrait_count += 1;
            } else {
                landscape_count += 1;
            }
            if let Ok(meta) = std::fs::metadata(&analysis.path) {
                total_size += meta.len();
            }
            images.push(analysis);
        }

        let suggested_orientation = if portrait_count > landscape_count {
            "portrait".to_string()
        } else if landscape_count > portrait_count {
            "landscape".to_string()
        } else {
            "auto".to_string()
        };

        Ok(FolderAnalysis {
            images,
            portrait_count,
            landscape_count,
            suggested_orientation,
            total_size,
        })
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
pub fn calculate_preview_layout(
    images: Vec<ImageAnalysis>,
    page_size: PageSize,
    orientation: PageOrientation,
    margin: f64,
) -> Result<PreviewData, String> {
    let validated_margin = validate_margin(margin, page_size)?;
    let pages: Vec<LayoutResult> = images
        .par_iter()
        .map(|img| calculate_page_layout(img, page_size, orientation, validated_margin))
        .collect();

    Ok(PreviewData {
        pages,
        page_size,
        orientation,
        margin: validated_margin,
    })
}

#[tauri::command]
pub async fn generate_pdf(
    images: Vec<ImageAnalysis>,
    config: PdfConfig,
    window: tauri::WebviewWindow,
) -> Result<GenerationResult, String> {
    use std::time::Instant;
    let start = Instant::now();

    let total = images.len();
    let original_total_size: u64 = images
        .iter()
        .filter_map(|img| std::fs::metadata(&img.path).ok())
        .map(|m| m.len())
        .sum();

    let validated_margin = validate_margin(config.margin, config.page_size)?;
    let config = PdfConfig {
        margin: validated_margin,
        ..config
    };

    // 第一阶段：并行处理图片（解码 -> resize -> 编码为JPEG/PNG）
    let _ = window.emit(
        "pdf_progress",
        GenerationProgress {
            current: 0,
            total,
            current_file: String::new(),
            phase: "处理图片".to_string(),
        },
    );

    let mut doc: Option<PdfDocumentReference> = None;
    let mut page_counter = 0usize;
    let (tx, rx) =
        std::sync::mpsc::sync_channel::<Result<(usize, String, Vec<PreparedPage>), String>>(4);
    let images_clone = images.clone();
    let config_clone = config.clone();
    let tx_clone = tx.clone();
    rayon::spawn(move || {
        images_clone
            .par_iter()
            .enumerate()
            .for_each_with(tx_clone, |sender, (idx, img)| {
                let res = process_image_for_pdf(Path::new(&img.path), &config_clone)
                    .map(|pages| (idx, img.path.clone(), pages));
                let _ = sender.send(res);
            });
    });
    drop(tx);

    let mut pending: BTreeMap<usize, (String, Vec<PreparedPage>)> = BTreeMap::new();
    let mut next_index = 0usize;

    // 边接收边按序写入PDF，避免同时持有所有页面数据
    for received in rx {
        let (idx, path, pages) = received?;
        pending.insert(idx, (path, pages));

        while let Some((_, prepared_pages)) = pending.remove(&next_index) {
            for prepared in &prepared_pages {
                if doc.is_none() {
                    let (d, page, layer) = PdfDocument::new(
                        "Image to PDF",
                        Mm(prepared.page_width as f32),
                        Mm(prepared.page_height as f32),
                        "Layer 1",
                    );
                    embed_encoded_image_to_page(&d, page, layer, prepared)?;
                    doc = Some(d);
                } else if let Some(ref d) = doc {
                    let (page, layer) = d.add_page(
                        Mm(prepared.page_width as f32),
                        Mm(prepared.page_height as f32),
                        "Layer 1",
                    );
                    embed_encoded_image_to_page(d, page, layer, prepared)?;
                }

                page_counter += 1;
                let _ = window.emit(
                    "pdf_progress",
                    GenerationProgress {
                        current: page_counter,
                        total,
                        current_file: String::new(),
                        phase: "写入页面".to_string(),
                    },
                );
            }
            // prepared_pages 在此 drop，释放已写入页面的内存
            next_index += 1;
        }
    }

    let doc = doc.ok_or_else(|| "没有有效的图片可以处理".to_string())?;

    // 保存PDF
    let output_path = PathBuf::from(&config.output_path);
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let file = File::create(&output_path).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);
    doc.save(&mut writer)
        .map_err(|e| format!("保存PDF失败: {}", e))?;

    let file_size = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let elapsed = start.elapsed().as_millis() as u64;

    // 验证输出大小
    if file_size > original_total_size * 5 {
        eprintln!(
            "警告: PDF体积({:.1}MB)远大于原始图片总大小({:.1}MB)",
            file_size as f64 / 1024.0 / 1024.0,
            original_total_size as f64 / 1024.0 / 1024.0
        );
    }

    Ok(GenerationResult {
        success: true,
        output_path: config.output_path,
        file_size,
        original_total_size,
        page_count: total,
        elapsed_ms: elapsed,
        error: None,
    })
}

#[tauri::command]
pub async fn get_image_thumbnail(image_path: String, max_size: u32) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let path = Path::new(&image_path);
        let img = image_crate::open(path).map_err(|e| e.to_string())?;

        let thumbnail = create_thumbnail(&img, max_size.clamp(256, 1024));

        let mut buffer = Cursor::new(Vec::new());
        thumbnail
            .write_to(&mut buffer, ImageFormat::Png)
            .map_err(|e| e.to_string())?;

        let base64_data = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            buffer.into_inner(),
        );

        Ok(format!("data:image/png;base64,{}", base64_data))
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
        }
    }

    #[test]
    fn validate_margin_rejects_negative_and_oversized_values() {
        assert!(validate_margin(-1.0, PageSize::A4).is_err());
        assert!(validate_margin(85.0, PageSize::A4).is_err());
        assert_eq!(
            validate_margin(20.0, PageSize::A4).expect("valid margin"),
            20.0
        );
    }

    #[test]
    fn auto_orientation_tracks_image_shape() {
        assert_eq!(
            get_page_dimensions(PageSize::A4, PageOrientation::Auto, 400, 800),
            (210.0, 297.0)
        );
        assert_eq!(
            get_page_dimensions(PageSize::A4, PageOrientation::Auto, 800, 400),
            (297.0, 210.0)
        );
    }

    #[test]
    fn layout_preserves_aspect_ratio_and_centers_image() {
        let image = sample_image(400, 200);
        let layout = calculate_page_layout(&image, PageSize::A4, PageOrientation::Landscape, 10.0);

        assert_close(layout.page_width, 297.0);
        assert_close(layout.page_height, 210.0);
        assert_close(layout.image.scaled_width, 277.0);
        assert_close(layout.image.scaled_height, 138.5);
        assert_close(layout.image.x, 10.0);
        assert_close(layout.image.y, 35.75);
    }
}
