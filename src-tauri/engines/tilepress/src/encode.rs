//! 图片准备层：把源文件变成可直接写进 PDF 的流。
//!
//! 三条路线：
//! 1. **原图无损**：JPEG → DCTDecode 原始字节直塞；PNG → IDAT + predictor 直塞。
//! 2. **WebP**：PDF 不认 WebP。有损 WebP 转 JPEG q92；无损 WebP 转 Flate；
//!    带 alpha 的拆出 SMask 保留透明度。
//! 3. **重编码档 / DPI 上限**：zune-jpeg 解码 → fast_image_resize 缩放 →
//!    jpeg-encoder 编码，全程纯 Rust，不依赖任何 C/C++ 库。

use anyhow::{bail, Context, Result};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

/// 输出质量档
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    /// 原图无损：JPEG/PNG 原始字节直塞，零重编码
    Lossless,
    /// 高质量 q92
    High,
    /// 标准 q85
    Standard,
    /// 小体积 q75
    Small,
}

impl Default for Quality {
    fn default() -> Self {
        Quality::Lossless
    }
}

impl Quality {
    pub fn jpeg_quality(self) -> Option<u8> {
        match self {
            Quality::Lossless => None,
            Quality::High => Some(92),
            Quality::Standard => Some(85),
            Quality::Small => Some(75),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Quality::Lossless => "原图无损",
            Quality::High => "高质量 q92",
            Quality::Standard => "标准 q85",
            Quality::Small => "小体积 q75",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "lossless" | "original" => Some(Quality::Lossless),
            "high" => Some(Quality::High),
            "standard" => Some(Quality::Standard),
            "small" => Some(Quality::Small),
            _ => None,
        }
    }

    /// 估算体积时使用的经验压缩比（相对原图字节）
    pub fn size_ratio(self) -> f32 {
        match self {
            Quality::Lossless => 1.0,
            Quality::High => 0.34,
            Quality::Standard => 0.20,
            Quality::Small => 0.13,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfColor {
    Gray,
    Rgb,
    Cmyk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfFilter {
    /// JPEG 原始字节
    Dct,
    /// 裸 Flate 流
    Flate,
    /// PNG IDAT：Flate + predictor 15
    PngPredictor { colors: u8, columns: u32 },
}

pub struct Smask {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub struct PreparedImage {
    pub width: u32,
    pub height: u32,
    pub color: PdfColor,
    pub filter: PdfFilter,
    pub data: Vec<u8>,
    pub smask: Option<Smask>,
    /// CMYK JPEG 直塞时需要 /Decode 反相
    pub decode_cmyk: bool,
    /// 源文件字节数（用于进度 / 估算）
    pub source_bytes: u64,
}

impl PreparedImage {
    /// PDF 图片字典（不含 /Length 与 /SMask，由写入器补）
    pub fn dict(&self) -> String {
        let cs = match self.color {
            PdfColor::Gray => "/DeviceGray",
            PdfColor::Rgb => "/DeviceRGB",
            PdfColor::Cmyk => "/DeviceCMYK",
        };
        let filter = match self.filter {
            PdfFilter::Dct => "/DCTDecode".to_string(),
            PdfFilter::Flate => "/FlateDecode".to_string(),
            PdfFilter::PngPredictor { colors, columns } => format!(
                "/FlateDecode /DecodeParms << /Predictor 15 /Colors {} /BitsPerComponent 8 /Columns {} >>",
                colors, columns
            ),
        };
        let decode = if self.decode_cmyk {
            " /Decode [1 0 1 0 1 0 1 0]"
        } else {
            ""
        };
        format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent 8 /Filter {}{}",
            self.width, self.height, cs, filter, decode
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum Format {
    Jpeg,
    Png,
    WebP,
    Gif,
    Bmp,
    Tiff,
    Avif,
    Unknown,
}

/// AVIF / HEIF：ISOBMFF 容器，ftyp 的 major brand 为 avif / avis
#[allow(dead_code)]
fn is_avif(b: &[u8]) -> bool {
    b.len() >= 12 && &b[4..8] == b"ftyp" && matches!(&b[8..12], b"avif" | b"avis" | b"mif1")
}

fn detect_format(b: &[u8]) -> Format {
    if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xD8 {
        Format::Jpeg
    } else if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        Format::Png
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Format::WebP
    } else if b.starts_with(b"GIF8") {
        Format::Gif
    } else if b.starts_with(b"BM") {
        Format::Bmp
    } else if b.starts_with(b"II\x2A\x00") || b.starts_with(b"MM\x00\x2A") {
        Format::Tiff
    } else {
        Format::Unknown
    }
}

/// 根据 DPI 上限计算缩放后的目标像素；不需要缩放时返回 None。
pub fn dpi_target(iw: u32, ih: u32, need_w: f32, need_h: f32) -> Option<(u32, u32)> {
    if iw == 0 || ih == 0 || need_w <= 0.0 || need_h <= 0.0 {
        return None;
    }
    let s = (need_w / iw as f32).min(need_h / ih as f32);
    // 原图只超出 15% 以内时不折腾
    if s >= 1.0 / 1.15 {
        return None;
    }
    let nw = ((iw as f32 * s).round() as u32).max(1);
    let nh = ((ih as f32 * s).round() as u32).max(1);
    if nw >= iw && nh >= ih {
        None
    } else {
        Some((nw, nh))
    }
}

/// 按**文件内容**识别格式打开图片（不看扩展名）。
/// 现实里常有 .png 实际是 JPEG 的文件，image::open 会按扩展名猜格式然后解码失败。
pub fn open_any(path: &std::path::Path) -> anyhow::Result<image::DynamicImage> {
    let f = std::fs::File::open(path)
        .with_context(|| format!("打开图片失败：{}", path.display()))?;
    let reader = std::io::BufReader::new(f);
    let mut r = image::ImageReader::new(reader);
    r = r.with_guessed_format().context("识别图片格式失败")?;
    r.decode().context("图片解码失败")
}

/// 准备一张图片。`target` 为 DPI 上限算出的目标像素（只缩不放）。
pub fn prepare(path: &Path, target: Option<(u32, u32)>, quality: Quality) -> Result<PreparedImage> {
    let raw = std::fs::read(path).with_context(|| format!("读取图片失败：{}", path.display()))?;
    let source_bytes = raw.len() as u64;
    let fmt = detect_format(&raw);

    let mut img = match quality {
        Quality::Lossless => match fmt {
            Format::Jpeg if target.is_none() => {
                if let Some(info) = jpeg_info(&raw) {
                    // 直接把读进来的原始字节交给写入器，避免再复制一份
                    return Ok(jpeg_passthrough(raw, info, source_bytes));
                }
                // 解析失败就走解码兜底
                decode(&raw, fmt)?
            }
            Format::Png if target.is_none() => {
                match png_passthrough(raw, source_bytes) {
                    Ok(p) => return Ok(p),
                    Err(raw) => decode(&raw, fmt)?,
                }
            }
            Format::Avif => {
                let dec = decode(&raw, fmt)?;
                let dec = resize_decoded(dec, target)?;
                // AVIF 无法直塞 PDF：无损档按解码后的像素走 Flate（等价 PNG），不丢画质
                return Ok(flate_prepared(dec, source_bytes));
            }
            Format::WebP => {
                let info = webp_info(&raw);
                let dec = decode(&raw, fmt)?;
                let dec = resize_decoded(dec, target)?;
                if info.map(|i| i.lossless).unwrap_or(false) {
                    // 无损 WebP → Flate（等价于 PNG Flate）
                    return Ok(flate_prepared(dec, source_bytes));
                }
                // 有损 WebP → JPEG q92
                return Ok(jpeg_prepared(dec, 92, source_bytes)?);
            }
            _ => decode(&raw, fmt)?,
        },
        _ => decode(&raw, fmt)?,
    };

    img = resize_decoded(img, target)?;

    match quality.jpeg_quality() {
        Some(q) => jpeg_prepared(img, q, source_bytes),
        None => Ok(flate_prepared(img, source_bytes)),
    }
}

struct Decoded {
    w: u32,
    h: u32,
    rgb: Vec<u8>,
    alpha: Option<Vec<u8>>,
}

fn resize_decoded(mut d: Decoded, target: Option<(u32, u32)>) -> Result<Decoded> {
    let Some((nw, nh)) = target else { return Ok(d) };
    if nw == d.w && nh == d.h {
        return Ok(d);
    }
    let rgb = resize_rgb(d.w, d.h, &d.rgb, nw, nh)?;
    let alpha = match d.alpha.take() {
        Some(a) => Some(resize_gray(d.w, d.h, &a, nw, nh)?),
        None => None,
    };
    d.rgb = rgb;
    d.alpha = alpha;
    d.w = nw;
    d.h = nh;
    Ok(d)
}

pub fn resize_rgb(w: u32, h: u32, src: &[u8], nw: u32, nh: u32) -> Result<Vec<u8>> {
    resize_rgb_owned(w, h, src.to_vec(), nw, nh)
}

/// 与 [`resize_rgb`] 相同，但直接接管源缓冲区，避免整图再复制一份。
pub fn resize_rgb_owned(w: u32, h: u32, src: Vec<u8>, nw: u32, nh: u32) -> Result<Vec<u8>> {
    if nw == 0 || nh == 0 {
        bail!("缩放目标尺寸为 0");
    }
    if nw == w && nh == h {
        return Ok(src);
    }
    let src_img = FirImage::from_vec_u8(w, h, src, PixelType::U8x3)
        .map_err(|e| anyhow::anyhow!("创建缩放源失败：{:?}", e))?;
    let mut dst = FirImage::new(nw, nh, PixelType::U8x3);
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));
    Resizer::new()
        .resize(&src_img, &mut dst, &opts)
        .map_err(|e| anyhow::anyhow!("缩放失败：{:?}", e))?;
    Ok(dst.into_vec())
}

pub fn resize_rgba(w: u32, h: u32, src: &[u8], nw: u32, nh: u32) -> Result<Vec<u8>> {
    resize_rgba_owned(w, h, src.to_vec(), nw, nh)
}

/// 与 [`resize_rgba`] 相同，但直接接管源缓冲区，避免整图再复制一份。
pub fn resize_rgba_owned(w: u32, h: u32, src: Vec<u8>, nw: u32, nh: u32) -> Result<Vec<u8>> {
    if nw == 0 || nh == 0 {
        bail!("缩放目标尺寸为 0");
    }
    if nw == w && nh == h {
        return Ok(src);
    }
    let src_img = FirImage::from_vec_u8(w, h, src, PixelType::U8x4)
        .map_err(|e| anyhow::anyhow!("创建缩放源失败：{:?}", e))?;
    let mut dst = FirImage::new(nw, nh, PixelType::U8x4);
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));
    Resizer::new()
        .resize(&src_img, &mut dst, &opts)
        .map_err(|e| anyhow::anyhow!("缩放失败：{:?}", e))?;
    Ok(dst.into_vec())
}

fn resize_gray(w: u32, h: u32, src: &[u8], nw: u32, nh: u32) -> Result<Vec<u8>> {
    if nw == 0 || nh == 0 {
        bail!("缩放目标尺寸为 0");
    }
    if nw == w && nh == h {
        return Ok(src.to_vec());
    }
    let src_img = FirImage::from_vec_u8(w, h, src.to_vec(), PixelType::U8)
        .map_err(|e| anyhow::anyhow!("创建缩放源失败：{:?}", e))?;
    let mut dst = FirImage::new(nw, nh, PixelType::U8);
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));
    Resizer::new()
        .resize(&src_img, &mut dst, &opts)
        .map_err(|e| anyhow::anyhow!("缩放失败：{:?}", e))?;
    Ok(dst.into_vec())
}

fn decode(raw: &[u8], fmt: Format) -> Result<Decoded> {
    if fmt == Format::Jpeg {
        if let Ok(d) = decode_jpeg_zune(raw) {
            return Ok(d);
        }
    }
    decode_with_image_crate(raw)
}

fn decode_jpeg_zune(raw: &[u8]) -> Result<Decoded> {
    use std::io::Cursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::JpegDecoder;

    let mut dec = JpegDecoder::new(Cursor::new(raw));
    let px = dec.decode().map_err(|e| anyhow::anyhow!("JPEG 解码失败：{:?}", e))?;
    let info = dec.info().context("JPEG 头信息缺失")?;
    let w = info.width as u32;
    let h = info.height as u32;
    if w == 0 || h == 0 {
        bail!("JPEG 尺寸为 0");
    }
    let cs = dec.output_colorspace().unwrap_or(ColorSpace::RGB);
    match cs {
        ColorSpace::RGB => {
            if px.len() < (w * h * 3) as usize {
                bail!("JPEG 解码数据长度不足");
            }
            Ok(Decoded { w, h, rgb: px, alpha: None })
        }
        ColorSpace::RGBA => {
            if px.len() < (w * h * 4) as usize {
                bail!("JPEG 解码数据长度不足");
            }
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            let mut alpha = Vec::with_capacity((w * h) as usize);
            for p in px.chunks_exact(4) {
                rgb.extend_from_slice(&p[..3]);
                alpha.push(p[3]);
            }
            Ok(Decoded { w, h, rgb, alpha: Some(alpha) })
        }
        ColorSpace::Luma => {
            if px.len() < (w * h) as usize {
                bail!("JPEG 解码数据长度不足");
            }
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            for &v in px.iter().take((w * h) as usize) {
                rgb.extend_from_slice(&[v, v, v]);
            }
            Ok(Decoded { w, h, rgb, alpha: None })
        }
        ColorSpace::CMYK | ColorSpace::YCCK => {
            if px.len() < (w * h * 4) as usize {
                bail!("JPEG 解码数据长度不足");
            }
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            for p in px.chunks_exact(4) {
                let (c, m, y, k) = (p[0] as u32, p[1] as u32, p[2] as u32, p[3] as u32);
                rgb.push(255u32.saturating_sub(c + k) as u8);
                rgb.push(255u32.saturating_sub(m + k) as u8);
                rgb.push(255u32.saturating_sub(y + k) as u8);
            }
            Ok(Decoded { w, h, rgb, alpha: None })
        }
        _ => bail!("不支持的 JPEG 输出色彩空间：{:?}", cs),
    }
}

fn decode_with_image_crate(raw: &[u8]) -> Result<Decoded> {
    let img = image::load_from_memory(raw).context("图片解码失败（格式损坏或不受支持）")?;
    let w = img.width();
    let h = img.height();
    if w == 0 || h == 0 {
        bail!("图片尺寸为 0");
    }
    let has_alpha = img.color().has_alpha();
    let rgba = img.to_rgba8();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut alpha = if has_alpha {
        Some(Vec::with_capacity((w * h) as usize))
    } else {
        None
    };
    for p in rgba.pixels() {
        rgb.extend_from_slice(&p.0[..3]);
        if let Some(a) = alpha.as_mut() {
            a.push(p.0[3]);
        }
    }
    // 全不透明的 alpha 通道没有保留价值
    if let Some(a) = alpha.as_ref() {
        if a.iter().all(|&v| v == 255) {
            alpha = None;
        }
    }
    Ok(Decoded { w, h, rgb, alpha })
}

fn jpeg_prepared(d: Decoded, quality: u8, source_bytes: u64) -> Result<PreparedImage> {
    let data = encode_jpeg(&d.rgb, d.w, d.h, quality)?;
    Ok(PreparedImage {
        width: d.w,
        height: d.h,
        color: PdfColor::Rgb,
        filter: PdfFilter::Dct,
        data,
        smask: d.alpha.map(|a| Smask {
            width: d.w,
            height: d.h,
            data: flate(&a),
        }),
        decode_cmyk: false,
        source_bytes,
    })
}

fn flate_prepared(d: Decoded, source_bytes: u64) -> PreparedImage {
    let smask = d.alpha.map(|a| Smask {
        width: d.w,
        height: d.h,
        data: flate(&a),
    });
    PreparedImage {
        width: d.w,
        height: d.h,
        color: PdfColor::Rgb,
        filter: PdfFilter::Flate,
        data: flate(&d.rgb),
        smask,
        decode_cmyk: false,
        source_bytes,
    }
}

pub fn encode_jpeg(rgb: &[u8], w: u32, h: u32, quality: u8) -> Result<Vec<u8>> {
    if w > u16::MAX as u32 || h > u16::MAX as u32 {
        bail!("图片尺寸超出 JPEG 编码上限");
    }
    let mut out = Vec::new();
    let enc = jpeg_encoder::Encoder::new(&mut out, quality);
    enc.encode(rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| anyhow::anyhow!("JPEG 编码失败：{}", e))?;
    Ok(out)
}

fn flate(data: &[u8]) -> Vec<u8> {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::new(3));
    let _ = enc.write_all(data);
    enc.finish().unwrap_or_default()
}

struct JpegInfo {
    w: u32,
    h: u32,
    components: u8,
}

fn jpeg_info(raw: &[u8]) -> Option<JpegInfo> {
    let mut i = 2usize;
    while i + 9 < raw.len() {
        if raw[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = raw[i + 1];
        if m == 0xFF {
            i += 1;
            continue;
        }
        if m == 0xD8 || m == 0x01 || (0xD0..=0xD7).contains(&m) {
            i += 2;
            continue;
        }
        let len = ((raw[i + 2] as usize) << 8) | raw[i + 3] as usize;
        if matches!(m, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let h = ((raw[i + 5] as u32) << 8) | raw[i + 6] as u32;
            let w = ((raw[i + 7] as u32) << 8) | raw[i + 8] as u32;
            let components = raw[i + 9];
            if w > 0 && h > 0 {
                return Some(JpegInfo { w, h, components });
            }
            return None;
        }
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

fn jpeg_passthrough(raw: Vec<u8>, info: JpegInfo, source_bytes: u64) -> PreparedImage {
    let color = match info.components {
        1 => PdfColor::Gray,
        4 => PdfColor::Cmyk,
        _ => PdfColor::Rgb,
    };
    PreparedImage {
        width: info.w,
        height: info.h,
        color,
        filter: PdfFilter::Dct,
        data: raw,
        smask: None,
        decode_cmyk: info.components == 4,
        source_bytes,
    }
}

struct PngInfo {
    w: u32,
    h: u32,
    depth: u8,
    color_type: u8,
    interlace: u8,
    /// IDAT 数据块在文件中的 [start, end) 区间
    idat: Vec<(usize, usize)>,
    saw_iend: bool,
}

fn png_info(raw: &[u8]) -> Option<PngInfo> {
    if raw.len() < 33 || !raw.starts_with(&[0x89, b'P', b'N', b'G']) {
        return None;
    }
    let mut i = 8usize;
    let mut info = PngInfo {
        w: 0,
        h: 0,
        depth: 0,
        color_type: 0,
        interlace: 0,
        idat: Vec::new(),
        saw_iend: false,
    };
    while i + 8 <= raw.len() {
        let len = u32::from_be_bytes([raw[i], raw[i + 1], raw[i + 2], raw[i + 3]]) as usize;
        let typ = &raw[i + 4..i + 8];
        let body_start = i + 8;
        let body_end = body_start.checked_add(len)?;
        if body_end > raw.len() {
            return None;
        }
        let body = &raw[body_start..body_end];
        match typ {
            b"IHDR" => {
                if body.len() < 13 {
                    return None;
                }
                info.w = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
                info.h = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
                info.depth = body[8];
                info.color_type = body[9];
                info.interlace = body[12];
            }
            b"IDAT" => info.idat.push((body_start, body_end)),
            b"IEND" => {
                info.saw_iend = true;
                break;
            }
            _ => {}
        }
        i = body_end + 4;
    }
    if info.w == 0 || info.h == 0 {
        return None;
    }
    Some(info)
}

/// 原地把所有 IDAT 块压到缓冲区开头，避免再分配一份图片大小的内存。
fn png_passthrough(mut raw: Vec<u8>, source_bytes: u64) -> Result<PreparedImage, Vec<u8>> {
    let Some(info) = png_info(&raw) else {
        return Err(raw);
    };
    if !info.saw_iend || info.depth != 8 || info.interlace != 0 {
        return Err(raw);
    }
    let (color, colors) = match info.color_type {
        0 => (PdfColor::Gray, 1),
        2 => (PdfColor::Rgb, 3),
        // 有 alpha / 调色板：走解码兜底，才能正确拆 SMask
        _ => return Err(raw),
    };
    let mut dst = 0usize;
    for (s, e) in &info.idat {
        let n = e - s;
        if *s != dst {
            raw.copy_within(*s..*e, dst);
        }
        dst += n;
    }
    raw.truncate(dst);
    Ok(PreparedImage {
        width: info.w,
        height: info.h,
        color,
        filter: PdfFilter::PngPredictor {
            colors,
            columns: info.w,
        },
        data: raw,
        smask: None,
        decode_cmyk: false,
        source_bytes,
    })
}

#[derive(Clone, Copy, Debug)]
struct WebpInfo {
    lossless: bool,
}

fn webp_info(raw: &[u8]) -> Option<WebpInfo> {
    if raw.len() < 16 || &raw[0..4] != b"RIFF" || &raw[8..12] != b"WEBP" {
        return None;
    }
    let mut i = 12usize;
    let mut lossless = false;
    while i + 8 <= raw.len() {
        let fourcc = &raw[i..i + 4];
        let sz = u32::from_le_bytes([raw[i + 4], raw[i + 5], raw[i + 6], raw[i + 7]]) as usize;
        match fourcc {
            b"VP8 " => lossless = false,
            b"VP8L" => lossless = true,
            _ => {}
        }
        i += 8 + sz + (sz & 1);
    }
    Some(WebpInfo { lossless })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_target_only_shrinks() {
        assert_eq!(dpi_target(4000, 3000, 4000.0, 3000.0), None);
        let t = dpi_target(4000, 3000, 272.0, 204.0).unwrap();
        assert_eq!(t, (272, 204));
    }
}
