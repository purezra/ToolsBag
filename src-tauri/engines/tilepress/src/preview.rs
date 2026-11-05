//! 整页预览合成：按 3 px/mm 把一页里的所有图片画到一张位图上。
//!
//! 预览和最终 PDF 读的是同一份 [`LayoutDoc`]，坐标完全一致，前端不做二次计算。
//! 结果缓存到临时目录，key = 排版参数哈希 + 页号。

use crate::{LayoutDoc, PreviewProgress};
use anyhow::{Context, Result};
use image::RgbImage;
use serde::Serialize;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const PX_PER_MM: f32 = 3.0;

#[derive(Clone, Debug, Serialize)]
pub struct PreviewImage {
    pub page: usize,
    pub width: u32,
    pub height: u32,
    pub path: String,
    /// 直接给前端 <img src> 用的 data URL，避免额外配置 asset 协议
    pub data_url: String,
    pub cached: bool,
}

#[derive(Clone)]
pub struct PreviewCache {
    pub root: PathBuf,
}

impl PreviewCache {
    pub fn for_layout(root: &Path, layout: &LayoutDoc) -> Self {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        // 渲染版本和实际排版都参与缓存键，避免不同方案或升级后复用旧预览。
        2u32.hash(&mut h);
        layout.page_w_mm.to_bits().hash(&mut h);
        layout.page_h_mm.to_bits().hash(&mut h);
        layout.margin_mm.to_bits().hash(&mut h);
        layout.gutter_mm.to_bits().hash(&mut h);
        layout.mode.hash(&mut h);
        layout.per_page.hash(&mut h);
        layout.pages.len().hash(&mut h);
        for img in &layout.images {
            img.path.hash(&mut h);
            img.width.hash(&mut h);
            img.height.hash(&mut h);
            img.bytes.hash(&mut h);
            if let Ok(meta) = std::fs::metadata(&img.path) {
                meta.len().hash(&mut h);
                meta.modified().ok().hash(&mut h);
            }
        }
        for page in &layout.pages {
            page.cells.len().hash(&mut h);
            for cell in &page.cells {
                cell.image.hash(&mut h);
                for value in [cell.x, cell.y, cell.w, cell.h, cell.dx, cell.dy, cell.dw, cell.dh] {
                    value.to_bits().hash(&mut h);
                }
            }
        }
        let dir = root.join(format!("{:016x}", h.finish()));
        PreviewCache { root: dir }
    }

    pub fn page_path(&self, page: usize) -> PathBuf {
        self.root.join(format!("page_{:04}.png", page))
    }

    /// 合成一页；已缓存则直接返回。`cached` 表示命中缓存。
    pub fn render(&self, layout: &LayoutDoc, page: usize) -> Result<(PathBuf, bool)> {
        let path = self.page_path(page);
        if path.exists() {
            return Ok((path, true));
        }
        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("创建预览缓存目录失败：{}", self.root.display()))?;

        // 与 WebP / AVIF / PNG 共用同一套合成器，只有像素密度不同。
        let (rgb, w, h) = crate::imgout::render_page_rgb(layout, page, PX_PER_MM)?;
        let canvas = RgbImage::from_raw(w, h, rgb).context("预览位图尺寸不匹配")?;

        // 原子写：先写临时文件再改名，避免前端读到半张图
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let tmp = self.root.join(format!(
            ".page_{:04}_{}_{}.tmp",
            page,
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        canvas
            .save_with_format(&tmp, image::ImageFormat::Png)
            .with_context(|| format!("预览图写入失败：{}", tmp.display()))?;
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        std::fs::rename(&tmp, &path)
            .with_context(|| format!("预览图改名失败：{}", path.display()))?;
        Ok((path, false))
    }

    /// 后台并行预生成全部页（rayon）。进度按页回调。
    pub fn prewarm(
        &self,
        layout: Arc<LayoutDoc>,
        cancel: Arc<AtomicBool>,
        progress: &(dyn Fn(PreviewProgress) + Sync),
    ) -> Result<usize> {
        let total = layout.pages.len();
        let done = AtomicUsize::new(0);
        let start = Instant::now();
        // 预览单页要同时解码几十张图；2 路并行能把峰值压在 300MB 以内，
        // 同时仍然明显快于逐页串行。
        let workers = crate::worker_count().min(2).max(1);
        let pool = crate::build_pool(workers);
        let this = self.clone();

        pool.install(|| -> Result<()> {
            use rayon::prelude::*;
            (0..total).into_par_iter().try_for_each(|p| -> Result<()> {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(());
                }
                this.render(&layout, p)?;
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                progress(PreviewProgress {
                    current: n,
                    total,
                    percent: 100.0 * n as f32 / total.max(1) as f32,
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    message: format!("预览生成中 {}/{}", n, total),
                });
                Ok(())
            })
        })?;
        Ok(done.load(Ordering::Relaxed))
    }
}

pub fn page_pixels(w_mm: f32, h_mm: f32) -> (u32, u32) {
    (
        (w_mm * PX_PER_MM).round().max(1.0) as u32,
        (h_mm * PX_PER_MM).round().max(1.0) as u32,
    )
}

/// 把毫米矩形转成像素矩形 `(x, y, w, h)`。
/// 用 round 保证相邻格子不重叠、不留缝。
pub fn rect_px(x: f32, y: f32, w_mm: f32, h_mm: f32) -> (i64, i64, u32, u32) {
    let x0 = (x * PX_PER_MM).round() as i64;
    let y0 = (y * PX_PER_MM).round() as i64;
    let x1 = ((x + w_mm) * PX_PER_MM).round() as i64;
    let y1 = ((y + h_mm) * PX_PER_MM).round() as i64;
    (x0, y0, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32)
}
