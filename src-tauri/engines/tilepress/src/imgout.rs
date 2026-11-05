//! 把排好的页面渲染成位图，并导出成 WebP / AVIF / PNG。
//!
//! PDF 里没法直塞 WebP / AVIF，所以「导出图片」这条路是把整页按指定 DPI
//! 合成成一张位图，再用对应编码器压一遍：A4 300 DPI = 2480×3508。

use crate::encode::{resize_rgb_owned, resize_rgba_owned};
use crate::LayoutDoc;
use anyhow::{Context, Result};
use image::{Rgb, RgbImage};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutFmt {
    Webp,
    Avif,
    Png,
}

impl OutFmt {
    pub fn ext(self) -> &'static str {
        match self {
            OutFmt::Webp => "webp",
            OutFmt::Avif => "avif",
            OutFmt::Png => "png",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OutFmt::Webp => "WebP",
            OutFmt::Avif => "AVIF",
            OutFmt::Png => "PNG",
        }
    }
}

/// None = 保留每张原图的像素数；Some = 用户明确指定整页 DPI。
pub fn page_pixel_density(layout: &LayoutDoc, page: usize, dpi: Option<u32>, fmt: OutFmt) -> Result<f32> {
    let pl = layout.pages.get(page).context("页号越界")?;
    let density = if let Some(dpi) = dpi {
        dpi.clamp(36, 600) as f32 / 25.4
    } else {
        let mut density = 300.0f32 / 25.4;
        for cell in &pl.cells {
            let meta = layout.images.get(cell.image).context("排版引用的图片不存在")?;
            anyhow::ensure!(cell.dw > 0.0 && cell.dh > 0.0, "图片绘制尺寸无效");
            // 为像素边界取整预留 1px，保证自动模式不会把原图缩小。
            density = density.max((meta.width as f32 + 1.0) / cell.dw)
                .max((meta.height as f32 + 1.0) / cell.dh);
        }
        density
    };
    let w = (layout.page_w_mm * density).round() as u64;
    let h = (layout.page_h_mm * density).round() as u64;
    if fmt == OutFmt::Webp && (w > 16383 || h > 16383) {
        anyhow::bail!("保留原图像素需要 {}×{} px，超过 WebP 单边 16383 px 上限。请减少每页张数，或手动指定 DPI。", w, h);
    }
    anyhow::ensure!(w > 0 && h > 0 && w.saturating_mul(h) <= 100_000_000,
        "本页需要 {}×{} px，超出整页合成的像素预算。请减少每页张数，或手动指定 DPI。", w, h);
    Ok(density)
}

/// 把一页合成成 RGB 像素（白底）。`px_per_mm` = dpi / 25.4。
pub fn render_page_rgb(
    layout: &LayoutDoc,
    page: usize,
    px_per_mm: f32,
) -> Result<(Vec<u8>, u32, u32)> {
    let w = (layout.page_w_mm * px_per_mm).round().max(1.0) as u32;
    let h = (layout.page_h_mm * px_per_mm).round().max(1.0) as u32;
    let mut canvas = RgbImage::from_pixel(w, h, Rgb([255, 255, 255]));
    let Some(pl) = layout.pages.get(page) else {
        anyhow::bail!("页号越界：{}", page);
    };

    for cell in &pl.cells {
        let meta = layout.images.get(cell.image).context("排版引用的图片不存在")?;
        let src = crate::encode::open_any(&meta.path)
            .with_context(|| format!("无法合成图片：{}", meta.path.display()))?;
        let has_alpha = src.color().has_alpha();
        let (sw, sh) = (src.width(), src.height());
        if sw == 0 || sh == 0 {
            continue;
        }

        let (dx, dy, dw, dh) = rect_px(cell.dx, cell.dy, cell.dw, cell.dh, px_per_mm);
        if dw == 0 || dh == 0 {
            continue;
        }

        // 缩到格子大小：带 alpha 的先合成到白底再取 RGB，避免透明处变黑
        let resized = if has_alpha {
            let rgba = src.into_rgba8();
            resize_rgba_owned(sw, sh, rgba.into_raw(), dw, dh).map(|v| {
                let mut out = Vec::with_capacity((dw * dh * 3) as usize);
                for p in v.chunks_exact(4) {
                    let a = p[3] as u32;
                    let blend = |c: u8| -> u8 {
                        ((c as u32 * a + 255 * (255 - a)) / 255) as u8
                    };
                    out.extend_from_slice(&[blend(p[0]), blend(p[1]), blend(p[2])]);
                }
                out
            })
        } else {
            let rgb = src.into_rgb8();
            resize_rgb_owned(sw, sh, rgb.into_raw(), dw, dh)
        };
        let raw = resized.with_context(|| format!("缩放图片失败：{}", meta.path.display()))?;
        let draw_img = RgbImage::from_raw(dw, dh, raw).context("图片缩放后尺寸不匹配")?;

        let (cx, cy, cw, ch) = rect_px(cell.x, cell.y, cell.w, cell.h, px_per_mm);

        // 与画布、格子同时求交
        let ix0 = cx.max(dx).max(0);
        let iy0 = cy.max(dy).max(0);
        let ix1 = (cx + cw as i64).min(dx + dw as i64).min(w as i64);
        let iy1 = (cy + ch as i64).min(dy + dh as i64).min(h as i64);
        if ix1 <= ix0 || iy1 <= iy0 {
            continue;
        }
        let sx = (ix0 - dx).max(0) as u32;
        let sy = (iy0 - dy).max(0) as u32;
        let sw2 = ((ix1 - ix0) as u32).min(dw.saturating_sub(sx));
        let sh2 = ((iy1 - iy0) as u32).min(dh.saturating_sub(sy));
        if sw2 == 0 || sh2 == 0 {
            continue;
        }
        let part = image::imageops::crop_imm(&draw_img, sx, sy, sw2, sh2).to_image();
        image::imageops::overlay(&mut canvas, &part, ix0, iy0);
    }

    Ok((canvas.into_raw(), w, h))
}

/// 毫米矩形 → 像素矩形（与预览同一套取整规则，相邻格子不留缝）
fn rect_px(x: f32, y: f32, w_mm: f32, h_mm: f32, px_per_mm: f32) -> (i64, i64, u32, u32) {
    let x0 = (x * px_per_mm).round() as i64;
    let y0 = (y * px_per_mm).round() as i64;
    let x1 = ((x + w_mm) * px_per_mm).round() as i64;
    let y1 = ((y + h_mm) * px_per_mm).round() as i64;
    (x0, y0, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32)
}

/// 编码一张 RGB 位图
pub fn encode_image(rgb: &[u8], w: u32, h: u32, fmt: OutFmt, quality: u8) -> Result<Vec<u8>> {
    let quality = quality.clamp(1, 100);
    match fmt {
        OutFmt::Webp => {
            let enc = webp::Encoder::from_rgb(rgb, w, h);
            let encoded = if quality == 100 {
                // 无损模式中的 quality 是压缩搜索力度，不是画质。
                // 快速策略仍逐像素无损，以较大文件换取明显更短的导出时间。
                let mut config = webp::WebPConfig::new()
                    .map_err(|e| anyhow::anyhow!("创建 WebP 编码配置失败：{:?}", e))?;
                config.lossless = 1;
                config.quality = 25.0;
                config.method = 0;
                config.thread_level = 1;
                config.near_lossless = 100;
                config.exact = 1;
                enc.encode_advanced(&config)
            } else {
                enc.encode_simple(false, quality as f32)
            };
            encoded
                .map(|bytes| bytes.to_vec())
                .map_err(|e| anyhow::anyhow!("WebP 编码失败：{:?}", e))
        }
        OutFmt::Avif => {
            let pixels: Vec<rgb::RGB8> = rgb
                .chunks_exact(3)
                .map(|p| rgb::RGB8::new(p[0], p[1], p[2]))
                .collect();
            let mut enc = ravif::Encoder::new()
                .with_quality(quality as f32)
                // 画质由 quality / 色彩模型决定；采用均衡搜索，避免逐页耗时过长。
                // 同质量档位的像素结果可能有极小差异，文件通常稍大。
                .with_speed(8)
                .with_num_threads(Some(crate::worker_count()));
            if quality == 100 {
                // 截图、细字和彩边避免 RGB↔YCbCr 的舍入；AV1 量化仍不宣称无损。
                enc = enc.with_internal_color_model(ravif::ColorModel::RGB)
                    .with_bit_depth(ravif::BitDepth::Eight);
            }
            let out = enc
                .encode_rgb(imgref::Img::new(
                    &pixels[..],
                    w as usize,
                    h as usize,
                ))
                .map_err(|e| anyhow::anyhow!("AVIF 编码失败：{}", e))?;
            Ok(out.avif_file)
        }
        OutFmt::Png => {
            let img = RgbImage::from_raw(w, h, rgb.to_vec())
                .context("PNG 位图尺寸不匹配")?;
            let mut buf = Vec::new();
            image::codecs::png::PngEncoder::new(&mut buf)
                .write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgb8)
                .context("PNG 编码失败")?;
            Ok(buf)
        }
    }
}

/// 导出一页到磁盘，返回文件路径
pub fn write_page(
    layout: &LayoutDoc,
    page: usize,
    dpi: u32,
    fmt: OutFmt,
    quality: u8,
    out_path: &Path,
) -> Result<(PathBuf, u64)> {
    write_page_with_resolution(layout, page, Some(dpi), fmt, quality, out_path)
}

pub fn write_page_with_resolution(
    layout: &LayoutDoc,
    page: usize,
    dpi: Option<u32>,
    fmt: OutFmt,
    quality: u8,
    out_path: &Path,
) -> Result<(PathBuf, u64)> {
    let px_per_mm = page_pixel_density(layout, page, dpi, fmt)?;
    let (rgb, w, h) = render_page_rgb(layout, page, px_per_mm)?;
    let bytes = encode_image(&rgb, w, h, fmt, quality)?;
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(out_path, &bytes)
        .with_context(|| format!("写入失败：{}", out_path.display()))?;
    Ok((out_path.to_path_buf(), bytes.len() as u64))
}

use image::ImageEncoder;
