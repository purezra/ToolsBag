//! tilepress 核心库：扫描图片 → 切分树排版 → 流式写出 PDF / 合成预览。
//!
//! GUI（Tauri）与 CLI（`main.rs`）都只调用这里的高层 API。
//! 排版结果 [`LayoutDoc`] 是预览与导出的唯一数据源，两边不重算。

pub mod encode;
pub mod imgout;
pub mod imgprobe;
pub mod layout;
pub mod pdf;
pub mod preview;

use anyhow::{bail, Context, Result};
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use encode::{dpi_target, prepare, Quality};
use layout::{anneal, pool_refine, Ctx, Rect};
use pdf::{Mode, PageCell, PdfWriter};
use preview::{page_pixels, PreviewCache};

/// 支持的幅面（毫米）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageKind {
    /// A4 竖版 210×297
    A4P,
    /// A4 横版 297×210
    A4L,
    /// A3 竖版 297×420
    A3P,
    /// A3 横版 420×297
    A3L,
}

impl Default for PageKind {
    fn default() -> Self {
        PageKind::A4P
    }
}

impl PageKind {
    pub fn dims_mm(self) -> (f32, f32) {
        match self {
            PageKind::A4P => (210.0, 297.0),
            PageKind::A4L => (297.0, 210.0),
            PageKind::A3P => (297.0, 420.0),
            PageKind::A3L => (420.0, 297.0),
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            PageKind::A4P => "a4p",
            PageKind::A4L => "a4l",
            PageKind::A3P => "a3p",
            PageKind::A3L => "a3l",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PageKind::A4P => "A4 竖版",
            PageKind::A4L => "A4 横版",
            PageKind::A3P => "A3 竖版",
            PageKind::A3L => "A3 横版",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "a4p" => Some(PageKind::A4P),
            "a4l" => Some(PageKind::A4L),
            "a3p" => Some(PageKind::A3P),
            "a3l" => Some(PageKind::A3L),
            _ => None,
        }
    }
}

/// 版式：拼贴（每格按图片比例自适应，白边最少）或等大网格（所有格子尺寸一致）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutKind {
    /// 拼贴：格子大小随图片比例变化，填充率最高
    Mosaic,
    /// 等面积行式：不裁剪、每张图面积接近、间距严格一致（默认）
    Grid,
}

impl Default for LayoutKind {
    fn default() -> Self {
        LayoutKind::Grid
    }
}

impl LayoutKind {
    pub fn key(self) -> &'static str {
        match self {
            LayoutKind::Mosaic => "mosaic",
            LayoutKind::Grid => "grid",
        }
    }
    pub fn from_key(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mosaic" | "collage" => Some(LayoutKind::Mosaic),
            "grid" | "uniform" => Some(LayoutKind::Grid),
            _ => None,
        }
    }
}

/// 扫描到的一张图片（只含尺寸和文件大小，不含像素）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageMeta {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

/// 导入阶段结果
#[derive(Clone, Debug)]
pub struct ScanResult {
    pub files: usize,
    pub skipped: usize,
    pub images: Vec<ImageMeta>,
    pub total_bytes: u64,
    pub landscape: usize,
    pub portrait: usize,
    pub square: usize,
    pub ms_scan: u64,
}

/// 给前端的导入摘要（不含 2000 条路径）
#[derive(Clone, Debug, Serialize)]
pub struct ScanInfo {
    pub files: usize,
    pub count: usize,
    pub skipped: usize,
    pub total_bytes: u64,
    pub avg_bytes: u64,
    pub landscape: usize,
    pub portrait: usize,
    pub square: usize,
    pub ms_scan: u64,
    /// 长宽比直方图：bin 中心比例 + 张数，供前端画分布图
    pub histogram: Vec<HistBin>,
}

/// 直方图的一根柱子
#[derive(Clone, Debug, Serialize)]
pub struct HistBin {
    /// 该 bin 的长宽比中心值
    pub ratio: f32,
    pub count: usize,
}

/// 排版 / 导出参数
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutOptions {
    pub page: PageKind,
    /// 页边距 mm
    pub margin_mm: f32,
    /// 图片间距 mm（每页仅 1 张时自动忽略）
    pub gutter_mm: f32,
    /// 每页张数
    pub per_page: usize,
    /// 逐页张数方案（各页可不等）；给了就优先于 per_page
    pub per_page_plan: Option<Vec<usize>>,
    /// 指定总页数时优先于 per_page
    pub pages: Option<usize>,
    /// 版式：拼贴 / 等大网格
    pub layout: LayoutKind,
    pub mode: Mode,
    /// 输出质量档
    pub quality: Quality,
    /// 输出 DPI 上限；None = 不限
    pub dpi: Option<u32>,
    pub seed: u64,
    /// 每页退火迭代次数
    pub iters: usize,
    /// 每页张数是否均摊（避免尾页过稀）
    pub even: bool,
    /// 相近比例优先排布：按长宽比聚簇，最集中的比例排前面，零散比例留到最后几页
    pub cluster: bool,
    /// 过滤：最小宽度（像素），None = 不限
    pub min_w: Option<u32>,
    /// 过滤：最小高度（像素）
    pub min_h: Option<u32>,
    /// 过滤：长宽比下限（宽/高）
    pub ar_min: Option<f32>,
    /// 过滤：长宽比上限（宽/高）
    pub ar_max: Option<f32>,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            page: PageKind::A4P,
            margin_mm: 10.0,
            gutter_mm: 2.0,
            per_page: 53,
            per_page_plan: None,
            pages: None,
            layout: LayoutKind::Grid,
            mode: Mode::Cover,
            quality: Quality::Lossless,
            dpi: None,
            seed: 42,
            iters: 40_000,
            even: true,
            cluster: true,
            min_w: None,
            min_h: None,
            ar_min: None,
            ar_max: None,
        }
    }
}

/// 按「分辨率 / 长宽比」过滤图片；上下限互填反了会自动交换
pub fn filter_images(
    images: &[ImageMeta],
    min_w: Option<u32>,
    min_h: Option<u32>,
    ar_min: Option<f32>,
    ar_max: Option<f32>,
) -> Vec<ImageMeta> {
    let (lo, hi) = match (ar_min, ar_max) {
        (Some(a), Some(b)) if a > b => (Some(b), Some(a)),
        (a, b) => (a, b),
    };
    let lo = lo.filter(|v| v.is_finite() && *v > 0.0);
    let hi = hi.filter(|v| v.is_finite() && *v > 0.0);
    images
        .iter()
        .filter(|m| {
            if let Some(w) = min_w {
                if m.width < w {
                    return false;
                }
            }
            if let Some(h) = min_h {
                if m.height < h {
                    return false;
                }
            }
            let ar = m.width as f32 / m.height.max(1) as f32;
            if let Some(v) = lo {
                if ar < v {
                    return false;
                }
            }
            if let Some(v) = hi {
                if ar > v {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect()
}

/// 一个图片格（毫米，页面左上角原点）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CellLayout {
    /// 指向 `LayoutDoc::images`
    pub image: usize,
    /// 格子矩形（裁切边界）
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// 图片实际绘制矩形（cover 时超出格子）
    pub dx: f32,
    pub dy: f32,
    pub dw: f32,
    pub dh: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageLayout {
    pub cells: Vec<CellLayout>,
    /// 0–1 填充率
    pub fill: f32,
}

/// 排版结果：预览和导出都读这一份。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutDoc {
    pub page_w_mm: f32,
    pub page_h_mm: f32,
    pub margin_mm: f32,
    pub gutter_mm: f32,
    pub mode: Mode,
    pub quality: Quality,
    pub dpi: Option<u32>,
    pub per_page: usize,
    pub images: Vec<ImageMeta>,
    pub pages: Vec<PageLayout>,
    pub fill_avg: f32,
    pub ms_scan: u64,
    pub ms_layout: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LayoutSummary {
    pub images: usize,
    pub pages: usize,
    pub per_page: usize,
    pub fill_avg: f32,
    pub page_w_mm: f32,
    pub page_h_mm: f32,
    pub ms_scan: u64,
    pub ms_layout: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanProgress {
    pub scanned: usize,
    pub total: usize,
    pub usable: usize,
    pub bytes: u64,
    pub percent: f32,
    pub elapsed_ms: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LayoutProgress {
    pub current: usize,
    pub total: usize,
    pub percent: f32,
    pub elapsed_ms: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PreviewProgress {
    pub current: usize,
    pub total: usize,
    pub percent: f32,
    pub elapsed_ms: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExportProgress {
    pub pages_done: usize,
    pub pages_total: usize,
    pub bytes_written: u64,
    pub bytes_total: u64,
    pub speed_bps: f64,
    pub percent: f32,
    pub elapsed_ms: u64,
    pub current_file: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExportResult {
    pub pages: usize,
    pub bytes: u64,
    pub ms: u64,
    pub speed_bps: f64,
    pub skipped: usize,
    pub pdf_path: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Estimate {
    pub bytes: u64,
    pub pages: usize,
    pub images: usize,
    /// 是否因为"原图无损 + DPI 不限"而等于原图总和
    pub is_original_sum: bool,
}

/// 推荐结果：按内框面积估算每页张数
#[derive(Clone, Debug, Serialize)]
pub struct Recommendation {
    pub mid: usize,
    pub lo: usize,
    pub hi: usize,
    pub pages: usize,
    /// 逐页张数方案（各页可以不等）
    pub plan: Vec<usize>,
    /// 该方案的预估平均填充率
    pub plan_fill: f32,
    /// 备选方案（按评分排序，最多 3 个，第一个 = plan）
    pub options: Vec<PlanOption>,
}

/// 一个候选分配方案
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanOption {
    pub pages: usize,
    /// 逐页张数
    pub plan: Vec<usize>,
    /// 平均填充率（0–1）
    pub fill: f32,
    /// 平均面积差倍数（max/min，越接近 1 越均匀）
    pub spread: f32,
    /// 每页张数的极值
    pub min: usize,
    pub max: usize,
    /// 是否因约束无解而放宽（此时不满足用户填的上下限）
    pub relaxed: bool,
}

/// 按这个文件夹真实的长宽比分布，算一套逐页张数方案。
/// 图先按比例聚簇排序，再用 DP 切成连续的若干页，每页张数可不等，
/// 目标是各页填充率之和最大（等价于整体最饱满、白边最少）。
pub fn plan_pages(aspects: &[f32], inner: Rect, gutter: f32, mid: usize) -> (Vec<usize>, f32) {
    let n = aspects.len();
    if n == 0 {
        return (Vec::new(), 0.0);
    }
    let mid = mid.max(1);
    let p0 = ((n + mid - 1) / mid).max(1);
    let mut best: Option<(Vec<usize>, f32)> = None;
    let mut cands = vec![p0];
    if p0 > 1 {
        cands.push(p0 - 1);
    }
    cands.push(p0 + 1);
    cands.push(p0 + 2);
    for pages in cands {
        if pages == 0 || pages > n {
            continue;
        }
        let Some((plan, avg)) = dp_split(aspects, inner, gutter, pages) else {
            continue;
        };
        let take = match &best {
            None => true,
            Some((bp, ba)) => {
                avg > *ba + 1e-4 || ((avg - *ba).abs() <= 1e-4 && plan.len() < bp.len())
            }
        };
        if take {
            best = Some((plan, avg));
        }
    }
    best.unwrap_or_else(|| (vec![n], 0.0))
}

/// 固定页数的最优切分：dp[p][i] = 用 p 页排完前 i 张的最大填充总和
fn dp_split(asp: &[f32], r: Rect, gutter: f32, pages: usize) -> Option<(Vec<usize>, f32)> {
    dp_split_bounded(asp, r, gutter, pages, 0, usize::MAX)
}

/// 带用户上下限的 DP：每页张数必须落在 [lo_cap, hi_cap] 且不超出平均值 ±20%
fn dp_split_bounded(
    asp: &[f32],
    r: Rect,
    gutter: f32,
    pages: usize,
    lo_cap: usize,
    hi_cap: usize,
) -> Option<(Vec<usize>, f32)> {
    let n = asp.len();
    if pages == 0 || pages > n {
        return None;
    }
    // 硬约束：平均值 ±20%，再与用户给的上下限取交集
    let avg = n as f32 / pages as f32;
    let lo = ((avg * 0.80).floor() as usize).max(1).max(lo_cap);
    let hi = ((avg * 1.20).ceil() as usize).min(n).min(hi_cap.max(1));
    if hi < lo {
        return None;
    }
    let step = (((hi - lo) / 24).max(1)) as usize;

    let stride = n + 1;
    let mut dp = vec![f32::NEG_INFINITY; (pages + 1) * stride];
    let mut from = vec![usize::MAX; (pages + 1) * stride];
    dp[0] = 0.0;

    for p in 1..=pages {
        for i in 0..=n {
            let cur = dp[(p - 1) * stride + i];
            if cur == f32::NEG_INFINITY {
                continue;
            }
            let kmax = hi.min(n - i);
            if kmax < lo {
                // 剩下的张数已经凑不出合法的一页，这条路径作废
                continue;
            }
            let mut k = lo;
            while k <= kmax {
                let (f, _) = layout::estimate_fill(&asp[i..i + k], r, gutter);
                let v = cur + f;
                if v > dp[p * stride + i + k] {
                    dp[p * stride + i + k] = v;
                    from[p * stride + i + k] = i;
                }
                k += step;
            }
            // 保证末尾的 k 也试一次（避免步长跳过）
            let (f, _) = layout::estimate_fill(&asp[i..i + kmax], r, gutter);
            let v = cur + f;
            if v > dp[p * stride + i + kmax] {
                dp[p * stride + i + kmax] = v;
                from[p * stride + i + kmax] = i;
            }
        }
    }

    let total = dp[pages * stride + n];
    if total == f32::NEG_INFINITY || total <= 0.0 {
        return None;
    }
    let mut plan = Vec::with_capacity(pages);
    let mut i = n;
    for p in (1..=pages).rev() {
        let prev = from[p * stride + i];
        if prev == usize::MAX || prev >= i {
            return None;
        }
        plan.push(i - prev);
        i = prev;
    }
    if i != 0 {
        return None;
    }
    plan.reverse();
    Some((plan, total / pages as f32))
}

/// 逐页张数方案的真实质量：用最终出图同一套排版跑一遍，返回 (平均填充, 平均面积差)
fn eval_plan(asp: &[f32], plan: &[usize], r: Rect, gutter: f32) -> (f32, f32) {
    let mut i = 0usize;
    let mut fill_sum = 0.0f32;
    let mut spread_sum = 0.0f32;
    let mut pages = 0usize;
    for &k in plan {
        if k == 0 || i + k > asp.len() {
            return (0.0, f32::MAX);
        }
        let (f, sp) = layout::page_quality(&asp[i..i + k], r, gutter);
        fill_sum += f;
        spread_sum += sp.min(50.0);
        pages += 1;
        i += k;
    }
    if pages == 0 || i != asp.len() {
        return (0.0, f32::MAX);
    }
    (fill_sum / pages as f32, spread_sum / pages as f32)
}

fn equal_split(n: usize, pages: usize) -> Vec<usize> {
    let base = n / pages;
    let rem = n % pages;
    (0..pages)
        .map(|p| base + if p < rem { 1 } else { 0 })
        .collect()
}

/// 把抽样方案按比例放大到全量，并修正到总数正好等于 n
fn scale_plan(plan: &[usize], n: usize) -> Vec<usize> {
    let total: usize = plan.iter().sum();
    if total == 0 {
        return vec![n];
    }
    let mut out: Vec<usize> = plan
        .iter()
        .map(|&k| ((k as f64 * n as f64 / total as f64).round() as usize).max(1))
        .collect();
    // 修正总量
    loop {
        let s: usize = out.iter().sum();
        if s == n {
            break;
        }
        if s < n {
            let idx = out
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| **v)
                .map(|(i, _)| i)
                .unwrap_or(0);
            out[idx] += 1;
        } else {
            let idx = out
                .iter()
                .enumerate()
                .filter(|(_, v)| **v > 1)
                .max_by_key(|(_, v)| **v)
                .map(|(i, _)| i);
            match idx {
                Some(i) => out[i] -= 1,
                None => break,
            }
        }
    }
    out
}

/// 动态推荐：拿这个文件夹真实的长宽比分布，选出「最好看」的逐页张数方案。
/// 做法：按比例聚簇排序 → 在代表性子集上枚举候选方案 → 用最终排版引擎真实打分。
pub fn recommend_for(
    images: &[ImageMeta],
    page: PageKind,
    margin_mm: f32,
    gutter_mm: f32,
) -> Recommendation {
    recommend_bounded(images, page, margin_mm, gutter_mm, None, None)
}

/// 带「每页不少于 m 张 / 不多于 n 张」约束的推荐，返回最多 3 个候选方案。
pub fn recommend_bounded(
    images: &[ImageMeta],
    page: PageKind,
    margin_mm: f32,
    gutter_mm: f32,
    min_per: Option<usize>,
    max_per: Option<usize>,
) -> Recommendation {
    let inner = inner_rect(page, margin_mm);
    let n = images.len();
    if n == 0 {
        return Recommendation {
            mid: 1,
            lo: 1,
            hi: 1,
            pages: 0,
            plan: Vec::new(),
            plan_fill: 0.0,
            options: Vec::new(),
        };
    }
    let aspects: Vec<f32> = images
        .iter()
        .map(|m| m.width as f32 / m.height.max(1) as f32)
        .collect();
    let mut by_asp: Vec<usize> = (0..n).collect();
    by_asp.sort_by(|&x, &y| {
        aspects[x]
            .partial_cmp(&aspects[y])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let order = cluster_order(&aspects, &by_asp);
    let ordered: Vec<f32> = order.iter().map(|&i| aspects[i]).collect();

    // 抽样规模：小文件夹直接用全量真实排版打分；
    // 大文件夹抽 400 张（每页张数的 8 倍以上，各页比例构成不会被打散）
    let sn = n.min(400);
    let full_eval = sn == n;
    let sample: Vec<f32> = if n <= sn {
        ordered.clone()
    } else {
        (0..sn).map(|i| ordered[i * n / sn]).collect()
    };

    let mid_area = (inner.area() / 1000.0).round().max(1.0) as usize;
    let p0 = ((n + mid_area - 1) / mid_area).max(1);

    // 用户给的每页张数上下限（None = 不限）；手输过程中可能 min > max，这里交换掉
    let (min_per, max_per) = match (min_per, max_per) {
        (Some(a), Some(b)) if a > b => (Some(b), Some(a)),
        other => other,
    };
    let lo_cap = min_per.unwrap_or(0).max(1).min(n.max(1));
    let hi_cap = max_per.unwrap_or(usize::MAX).max(1).max(lo_cap);

    let mut cands: Vec<Vec<usize>> = Vec::new();
    let mut page_counts: Vec<usize> = Vec::new();

    // 页数范围：既要装得下（每页 ≤ hi_cap），也不能太稀（每页 ≥ lo_cap）
    let p_lo = ((n + hi_cap - 1) / hi_cap).max(1).min(sn);
    let p_hi = (n / lo_cap.max(1)).max(1).min(sn);
    let (p_lo, p_hi) = if p_hi < p_lo { (p_lo, p_lo) } else { (p_lo, p_hi) };

    let mut try_pages: Vec<usize> = Vec::new();
    // 先放用户约束下的可行页数（最多 6 个，均匀取）
    if min_per.is_some() || max_per.is_some() {
        let span = p_hi - p_lo + 1;
        let take = span.min(6);
        for k in 0..take {
            let p = p_lo + (k * (span - 1)) / (take.max(2) - 1).max(1);
            if !try_pages.contains(&p) {
                try_pages.push(p);
            }
        }
        if !try_pages.contains(&p_hi) {
            try_pages.push(p_hi);
        }
    }
    // 再加按面积估出来的页数附近几个
    for d in [-1i64, 0, 1, 2] {
        let p = (p0 as i64 + d).max(1) as usize;
        if p >= p_lo && p <= p_hi && !try_pages.contains(&p) {
            try_pages.push(p);
        }
    }
    if try_pages.is_empty() {
        try_pages.push(p0.clamp(p_lo, p_hi));
    }

    for p in try_pages {
        if p > sn || page_counts.contains(&p) {
            continue;
        }
        page_counts.push(p);
        // 估算器给一个起点，再配一个均分方案
        if let Some((dp, _)) = dp_split_bounded(&sample, inner, gutter_mm, p, lo_cap, hi_cap) {
            cands.push(dp.clone());
            // 相邻页挪 1~2 张，破一下估算器的局部最优（大文件夹只做少量试探省时间）
            let max_shift = if full_eval { 2 } else { 1 };
            for shift in 1..=max_shift {
                for i in 0..dp.len().saturating_sub(1) {
                    if dp[i] > shift {
                        let mut v = dp.clone();
                        v[i] -= shift;
                        v[i + 1] += shift;
                        cands.push(v);
                    }
                }
            }
        }
        cands.push(equal_split(sn, p));
    }

    // 用真实排版打分，收集所有可行方案
    // 页数越少越省纸：超出「约束允许的最少页数」时每多一页扣一点分
    let p_lo_user = if max_per.is_some() {
        ((n + hi_cap - 1) / hi_cap).max(1)
    } else {
        1
    };
    let mut scored: Vec<(Vec<usize>, f32, f32, f32)> = Vec::new(); // (plan, score, fill, spread)
    for plan in &cands {
        let (fill, spread) = eval_plan(&sample, plan, inner, gutter_mm);
        if fill <= 0.0 {
            continue;
        }
        // 好看 = 填得满 + 大小均匀 − 多余页数的代价
        let extra = plan.len().saturating_sub(p_lo_user) as f32;
        let score = fill - 0.10 * spread.max(1.0).ln() - 0.02 * extra;
        scored.push((plan.clone(), score, fill, spread));
    }
    // 同分档（0.4% 以内）里页数少的优先
    scored.sort_by(|a, b| {
        let ba = (a.1 / 0.004).round();
        let bb = (b.1 / 0.004).round();
        bb.partial_cmp(&ba)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.len().cmp(&b.0.len()))
            .then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    // 去重（逐页张数完全相同 / 页数相同且只差 1 张的视为同一方案），取前 3
    let mut picked: Vec<(Vec<usize>, f32, f32)> = Vec::new();
    for (plan, _score, fill, spread) in &scored {
        let dup = picked.iter().any(|(p, _, _)| {
            if p.len() != plan.len() {
                return false;
            }
            let diff: usize = p.iter().zip(plan).map(|(a, b)| a.abs_diff(*b)).sum();
            diff <= 2
        });
        if dup {
            continue;
        }
        picked.push((plan.clone(), *fill, *spread));
        if picked.len() >= 3 {
            break;
        }
    }
    if picked.is_empty() {
        picked.push((equal_split(sn, p0), 0.0, 1.0));
    }

    // 逐个方案：放大到全量 → 夹进约束区间 → 配平到总数 = n
    let finish = |plan_s: &[usize]| -> Vec<usize> {
        let plan = if sn == n { plan_s.to_vec() } else { scale_plan(plan_s, n) };
        let pages = plan.len().max(1);
        let avg = n as f32 / pages as f32;
        let hi_c = ((avg * 1.20).ceil() as usize).min(hi_cap).max(1);
        let lo_c = ((avg * 0.80).floor() as usize).max(1).max(lo_cap).min(hi_c);
        let mut v: Vec<usize> = plan.iter().map(|&k| k.clamp(lo_c, hi_c)).collect();
        let mut guard = 0usize;
        loop {
            guard += 1;
            let ssum: usize = v.iter().sum();
            if ssum == n || v.is_empty() || guard > n * 4 {
                break;
            }
            if ssum < n {
                let i = v
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| **x < hi_c)
                    .min_by_key(|(_, x)| **x)
                    .map(|(i, _)| i);
                match i {
                    Some(i) => v[i] += 1,
                    None => break,
                }
            } else {
                let i = v
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| **x > lo_c)
                    .max_by_key(|(_, x)| **x)
                    .map(|(i, _)| i);
                match i {
                    Some(i) => v[i] -= 1,
                    None => break,
                }
            }
        }
        if v.iter().sum::<usize>() != n || v.is_empty() {
            // 约束互相矛盾（例如 60 张要求每页 40–45 张）时，退回均分，保证总数正确
            let k = v.len().max(1);
            let base = n / k;
            let rem = n % k;
            v = (0..k).map(|i| base + if i < rem { 1 } else { 0 }).collect();
        }
        v
    };

    let options: Vec<PlanOption> = picked
        .iter()
        .map(|(p, fill, spread)| {
            let plan = finish(p);
            let mn = *plan.iter().min().unwrap_or(&1);
            let mx = *plan.iter().max().unwrap_or(&1);
            let relaxed = min_per.map(|m| mn < m).unwrap_or(false)
                || max_per.map(|m| mx > m).unwrap_or(false);
            PlanOption {
                pages: plan.len(),
                plan,
                fill: *fill,
                spread: *spread,
                min: mn,
                max: mx,
                relaxed,
            }
        })
        .collect();

    let first = match options.first() {
        Some(o) => o.clone(),
        None => return Recommendation {
            mid: 1,
            lo: 1,
            hi: 1,
            pages: 0,
            plan: Vec::new(),
            plan_fill: 0.0,
            options: Vec::new(),
        },
    };
    let mut sorted = first.plan.clone();
    sorted.sort_unstable();
    if sorted.is_empty() {
        return Recommendation {
            mid: 1,
            lo: 1,
            hi: 1,
            pages: 0,
            plan: Vec::new(),
            plan_fill: 0.0,
            options: Vec::new(),
        };
    }
    Recommendation {
        mid: sorted[sorted.len() / 2],
        lo: first.min,
        hi: first.max,
        pages: first.pages,
        plan: first.plan,
        plan_fill: first.fill,
        options,
    }
}

/// 页面内框（扣除页边距后的可排版区域）
pub fn inner_rect(page: PageKind, margin_mm: f32) -> Rect {
    let (w, h) = page.dims_mm();
    Rect {
        x: margin_mm,
        y: margin_mm,
        w: (w - 2.0 * margin_mm).max(0.0),
        h: (h - 2.0 * margin_mm).max(0.0),
    }
}

pub fn recommend(page: PageKind, margin_mm: f32, count: usize) -> Recommendation {
    let inner = inner_rect(page, margin_mm);
    let mid_area = (inner.area() / 1000.0).round().max(1.0) as usize;
    let lo = ((mid_area as f32 / 1.35).round() as usize).max(1);
    let hi = (mid_area as f32 * 1.35).round() as usize;

    // 经典模式能不能整齐铺满，取决于张数能不能凑成合适的行列。
    // 按最常见的竖构图（2:3）在区间里试一遍，挑填充率最高的张数。
    let a0 = 0.667f32;
    let mut best = mid_area;
    let mut best_fill = 0.0f32;
    for n in lo..=hi {
        let asp: Vec<f32> = vec![a0; n];
        let slots: Vec<u32> = (0..n as u32).collect();
        let (cells, used_h) = layout::uniform_cells(&slots, &asp, inner, 1.0);
        if cells.is_empty() {
            continue;
        }
        // 内容超出页内高度会画到页外，这种张数直接排除
        if used_h > inner.h * 1.005 {
            continue;
        }
        let covered: f32 = cells.iter().map(|(_, r)| r.w * r.h).sum();
        let fill = covered / inner.area();
        if fill > best_fill + 1e-4 {
            best_fill = fill;
            best = n;
        }
    }
    let mid = best;
    let pages = if count == 0 { 0 } else { (count + mid - 1) / mid };
    let plan: Vec<usize> = (0..pages)
        .map(|p| mid.min(count.saturating_sub(p * mid)))
        .collect();
    Recommendation {
        mid,
        lo,
        hi,
        options: vec![PlanOption {
            pages,
            plan: plan.clone(),
            fill: best_fill,
            spread: 1.0,
            min: lo,
            max: hi,
            relaxed: false,
        }],
        pages,
        plan,
        plan_fill: best_fill,
    }
}

/// 递归扫描目录下的图片（按路径排序）
pub fn scan_images(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        bail!("图片文件夹不存在：{}", dir.display());
    }
    if !dir.is_dir() {
        bail!("路径不是文件夹：{}", dir.display());
    }
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let ext = p
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(
                    ext.as_str(),
                    "jpg" | "jpeg" | "png" | "webp" | "bmp" | "gif" | "tif" | "tiff"
                ) {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

// ---------- 线程池 ----------

/// 工作线程数：物理核数 - 1，最多 8，给系统至少留 1 核。
pub fn worker_count() -> usize {
    let phys = num_cpus::get_physical().max(2);
    (phys - 1).clamp(1, 8)
}

#[cfg(windows)]
fn set_below_normal_priority() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
    };
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

#[cfg(not(windows))]
fn set_below_normal_priority() {}

/// 建一个低优先级 rayon 线程池，避免把前台程序拖到掉帧。
pub fn build_pool(threads: usize) -> rayon::ThreadPool {
    let threads = threads.max(1);
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|i| format!("tilepress-worker-{}", i))
        .spawn_handler(|thread| {
            let mut b = std::thread::Builder::new();
            if let Some(name) = thread.name() {
                b = b.name(name.to_string());
            }
            if let Some(size) = thread.stack_size() {
                b = b.stack_size(size);
            }
            b.spawn(move || {
                set_below_normal_priority();
                thread.run();
            })?;
            Ok(())
        })
        .build()
        .unwrap_or_else(|_| rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap())
}

// ---------- 导入 ----------

/// 扫描 + 并行只读文件头拿尺寸。进度事件按 50ms 节流。
pub fn scan_dir(
    dir: &Path,
    cancel: Arc<AtomicBool>,
    progress: &(dyn Fn(ScanProgress) + Sync),
) -> Result<ScanResult> {
    let t0 = Instant::now();
    let files = scan_images(dir)?;
    let total = files.len();
    progress(ScanProgress {
        scanned: 0,
        total,
        usable: 0,
        bytes: 0,
        percent: 0.0,
        elapsed_ms: 0,
        message: format!("扫描 {} 个文件…", total),
    });

    let done = AtomicUsize::new(0);
    let usable = AtomicUsize::new(0);
    let bytes = AtomicU64::new(0);
    let last_emit = AtomicU64::new(0);
    let pool = build_pool(worker_count());

    let results: Vec<Option<(u32, u32, u64)>> = pool.install(|| {
        use rayon::prelude::*;
        files
            .par_iter()
            .map(|f| {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let len = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
                let dim = imgprobe::probe(f);
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                bytes.fetch_add(len, Ordering::Relaxed);
                if dim.is_some() {
                    usable.fetch_add(1, Ordering::Relaxed);
                }
                let now = t0.elapsed().as_millis() as u64;
                let prev = last_emit.load(Ordering::Relaxed);
                if now.saturating_sub(prev) >= 50
                    && last_emit
                        .compare_exchange(prev, now, Ordering::Relaxed, Ordering::Relaxed)
                        .is_ok()
                {
                    let u = usable.load(Ordering::Relaxed);
                    let b = bytes.load(Ordering::Relaxed);
                    progress(ScanProgress {
                        scanned: n,
                        total,
                        usable: u,
                        bytes: b,
                        percent: 100.0 * n as f32 / total.max(1) as f32,
                        elapsed_ms: now,
                        message: format!("已扫描 {}/{}", n, total),
                    });
                }
                dim.map(|(w, h)| (w, h, len))
            })
            .collect()
    });

    if cancel.load(Ordering::Relaxed) {
        bail!("已取消");
    }

    let mut images = Vec::with_capacity(usable.load(Ordering::Relaxed));
    let mut skipped = 0usize;
    let mut total_bytes = 0u64;
    let (mut landscape, mut portrait, mut square) = (0usize, 0usize, 0usize);
    for (f, r) in files.iter().zip(results.into_iter()) {
        match r {
            Some((w, h, len)) => {
                let a = w as f32 / h.max(1) as f32;
                if a > 1.02 {
                    landscape += 1;
                } else if a < 0.98 {
                    portrait += 1;
                } else {
                    square += 1;
                }
                total_bytes += len;
                images.push(ImageMeta {
                    path: f.clone(),
                    width: w,
                    height: h,
                    bytes: len,
                });
            }
            None => skipped += 1,
        }
    }

    let ms_scan = t0.elapsed().as_millis() as u64;
    progress(ScanProgress {
        scanned: total,
        total,
        usable: images.len(),
        bytes: total_bytes,
        percent: 100.0,
        elapsed_ms: ms_scan,
        message: format!("共 {} 张可用图片", images.len()),
    });

    Ok(ScanResult {
        files: total,
        skipped,
        images,
        total_bytes,
        landscape,
        portrait,
        square,
        ms_scan,
    })
}

impl ScanResult {
    pub fn info(&self) -> ScanInfo {
        let count = self.images.len();
        ScanInfo {
            files: self.files,
            count,
            skipped: self.skipped,
            total_bytes: self.total_bytes,
            avg_bytes: if count > 0 {
                self.total_bytes / count as u64
            } else {
                0
            },
            landscape: self.landscape,
            portrait: self.portrait,
            square: self.square,
            ms_scan: self.ms_scan,
            histogram: aspect_histogram(&self.images, 24),
        }
    }
}

/// 按 ln(长宽比) 等宽分 bins 个箱，统计各箱张数
pub fn aspect_histogram(images: &[ImageMeta], bins: usize) -> Vec<HistBin> {
    if images.is_empty() || bins == 0 {
        return Vec::new();
    }
    let lo = 0.35f32.ln();
    let hi = 2.8f32.ln();
    let step = (hi - lo) / bins as f32;
    let mut counts = vec![0usize; bins];
    for im in images {
        if im.width == 0 || im.height == 0 {
            continue;
        }
        let a = (im.width as f32 / im.height as f32).clamp(0.35, 2.8);
        let idx = (((a.ln() - lo) / step) as usize).min(bins - 1);
        counts[idx] += 1;
    }
    counts
        .into_iter()
        .enumerate()
        .map(|(i, c)| HistBin {
            ratio: (lo + step * (i as f32 + 0.5)).exp(),
            count: c,
        })
        .collect()
}

/// 体积预估：无损档 = 原图总和；重编码档按压缩比 × DPI 降采样比例估算。
pub fn estimate_size(scan: &ScanResult, opts: &LayoutOptions) -> Estimate {
    let kept = filter_images(
        &scan.images,
        opts.min_w,
        opts.min_h,
        opts.ar_min,
        opts.ar_max,
    );
    let n_img = kept.len();
    let (n_pages, per_page, _) = page_plan(n_img, opts.per_page, opts.pages, opts.even);
    let inner = inner_rect(opts.page, opts.margin_mm);
    let avg_side_mm = (inner.area() / per_page.max(1) as f32).sqrt();
    let is_original_sum = opts.quality == Quality::Lossless && opts.dpi.is_none();

    let mut bytes = 0f64;
    for img in &kept {
        let mut b = img.bytes as f64;
        if is_original_sum {
            bytes += b;
            continue;
        }
        if let Some(dpi) = opts.dpi {
            let need = avg_side_mm / 25.4 * dpi as f32;
            let s = (need / img.width.max(1) as f32)
                .min(need / img.height.max(1) as f32)
                .min(1.0);
            b *= (s * s) as f64;
        }
        let ratio = match opts.quality {
            Quality::Lossless => 1.0,
            q => q.size_ratio(),
        };
        bytes += b * ratio as f64;
    }
    Estimate {
        bytes: (bytes * 1.01) as u64,
        pages: n_pages,
        images: n_img,
        is_original_sum,
    }
}

// ---------- 排版 ----------

fn page_plan(
    n_img: usize,
    per_page: usize,
    pages: Option<usize>,
    even: bool,
) -> (usize, usize, Vec<usize>) {
    let (n_pages, per) = match pages {
        Some(k) if k > 0 => {
            let k = k.min(n_img.max(1));
            (k, (n_img + k - 1) / k)
        }
        _ => {
            let per = per_page.max(1);
            ((n_img + per - 1) / per, per)
        }
    };
    let counts: Vec<usize> = if even && n_pages > 1 {
        let base = n_img / n_pages;
        let rem = n_img % n_pages;
        (0..n_pages)
            .map(|p| base + if p < rem { 1 } else { 0 })
            .collect()
    } else {
        (0..n_pages)
            .map(|p| {
                let used_n = p * per;
                per.min(n_img.saturating_sub(used_n))
            })
            .collect()
    };
    (n_pages, per, counts)
}

fn draw_rect(cell: Rect, img_aspect: f32, mode: Mode) -> (f32, f32, f32, f32) {
    let ca = cell.aspect();
    let (dw, dh) = match mode {
        Mode::Cover => {
            if img_aspect > ca {
                (cell.h * img_aspect, cell.h)
            } else {
                (cell.w, cell.w / img_aspect)
            }
        }
        Mode::Fit => {
            if img_aspect > ca {
                (cell.w, cell.w / img_aspect)
            } else {
                (cell.h * img_aspect, cell.h)
            }
        }
    };
    let dx = cell.x + (cell.w - dw) / 2.0;
    let dy = cell.y + (cell.h - dh) / 2.0;
    (dx, dy, dw, dh)
}

/// 只排版，不写 PDF。结果存入 `LayoutDoc`，预览和导出共用。
pub fn build_layout(
    scan: &ScanResult,
    opts: &LayoutOptions,
    cancel: Arc<AtomicBool>,
    progress: &(dyn Fn(LayoutProgress) + Sync),
) -> Result<LayoutDoc> {
    if opts.per_page == 0 && opts.pages.unwrap_or(0) == 0 {
        bail!("每页张数至少为 1");
    }
    if opts.margin_mm < 0.0 {
        bail!("页边距不能为负数");
    }
    let (pw, ph) = opts.page.dims_mm();
    if opts.margin_mm * 2.0 >= pw.min(ph) {
        bail!(
            "页边距 {} mm 过大，{}（{:.0}×{:.0} mm）放不下",
            opts.margin_mm,
            opts.page.label(),
            pw,
            ph
        );
    }
    let kept = filter_images(
        &scan.images,
        opts.min_w,
        opts.min_h,
        opts.ar_min,
        opts.ar_max,
    );
    let dropped = scan.images.len() - kept.len();
    if kept.is_empty() {
        bail!(
            "过滤后没有可用图片了（原 {} 张全部被过滤掉），请放宽条件",
            scan.images.len()
        );
    }
    let filtered_scan = ScanResult {
        files: scan.files,
        skipped: scan.skipped,
        images: kept,
        ..scan.clone()
    };
    let scan = &filtered_scan;
    let n_img = scan.images.len();
    if n_img == 0 {
        bail!("没有可用图片");
    }
    let _ = dropped;

    let aspects: Vec<f32> = scan
        .images
        .iter()
        .map(|m| m.width as f32 / m.height.max(1) as f32)
        .collect();
    let inner = inner_rect(opts.page, opts.margin_mm);
    let inner_area = inner.area();

    let (n_pages, per_page, counts) = match &opts.per_page_plan {
        Some(plan) if !plan.is_empty() => {
            let total: usize = plan.iter().sum();
            if total == n_img {
                (plan.len(), *plan.iter().max().unwrap_or(&1), plan.clone())
            } else {
                page_plan(n_img, opts.per_page, opts.pages, opts.even)
            }
        }
        _ => page_plan(n_img, opts.per_page, opts.pages, opts.even),
    };

    // 等大网格的目标长宽比：取全体图片中位数，整页裁切量最小
    let _target_aspect = {
        let mut a = aspects.clone();
        a.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        if a.is_empty() {
            0.75
        } else {
            a[a.len() / 2]
        }
    };

    // 等大网格：把每页张数规整成 cols×rows，页数按实际格子数重算，避免丢图
    // 等面积行式排版自己决定每行几张，不再把每页张数规整成 cols×rows
    let grid: Option<(usize, usize, usize)> = None;

    let (n_pages, per_page, counts) = match grid {
        Some((_, _, cells)) => {
            let pages = (n_img + cells - 1) / cells;
            let base = n_img / pages;
            let rem = n_img % pages;
            let counts: Vec<usize> = (0..pages)
                .map(|p| base + if p < rem { 1 } else { 0 })
                .collect();
            (pages, cells, counts)
        }
        None => (n_pages, per_page, counts),
    };

    let t0 = Instant::now();
    let mut rng = StdRng::seed_from_u64(opts.seed);
    let mut used = vec![false; n_img];
    let mut pages: Vec<PageLayout> = Vec::with_capacity(n_pages);
    let mut fill_sum = 0.0f64;

    let mut by_asp: Vec<usize> = (0..n_img).collect();
    by_asp.sort_by(|&x, &y| {
        aspects[x]
            .partial_cmp(&aspects[y])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    progress(LayoutProgress {
        current: 0,
        total: n_pages,
        percent: 0.0,
        elapsed_ms: 0,
        message: format!("排版 0/{} 页", n_pages),
    });

    // 经典模式的分页顺序：勾选「相近比例优先」时按长宽比聚簇，
    // 最集中的比例排最前、零散比例留到最后几页；不勾选则按导入顺序。
    let classic_order: Vec<usize> = if opts.layout == LayoutKind::Grid {
        if opts.cluster {
            cluster_order(&aspects, &by_asp)
        } else {
            (0..n_img).collect()
        }
    } else {
        Vec::new()
    };

    for p in 0..n_pages {
        if cancel.load(Ordering::Relaxed) {
            bail!("已取消");
        }
        let take = counts[p];
        if take == 0 {
            break;
        }

        // 分层取样：从未用图里按长宽比跨度均匀挑 take 张
        let mut free: Vec<usize> = by_asp.iter().copied().filter(|&i| !used[i]).collect();
        free.sort_by(|&x, &y| {
            aspects[x]
                .partial_cmp(&aspects[y])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut cand: Vec<usize> = Vec::with_capacity(take);
        if opts.layout == LayoutKind::Grid {
            // 经典模式：按聚簇顺序取，最集中的比例排前面，零散比例留到最后
            cand.extend(classic_order.iter().copied().filter(|&i| !used[i]).take(take));
        } else if take >= free.len() {
            cand.extend(free.iter().copied());
        } else {
            for k in 0..take {
                let pos =
                    (k as f64 * (free.len() as f64 - 1.0) / (take as f64 - 1.0).max(1.0)) as usize;
                cand.push(free[pos.min(free.len() - 1)]);
            }
            cand.sort_unstable();
            cand.dedup();
            let mut it = free.iter().copied();
            while cand.len() < take {
                let x = it.next().unwrap();
                if !cand.contains(&x) {
                    cand.push(x);
                }
            }
        }

        let g = if take <= 1 { 0.0 } else { opts.gutter_mm };

        // 等大网格：所有格子尺寸一致，不做拼贴搜索
        if let Some((cols, rows, _)) = grid {
            let rects = layout::grid_cells(inner, g, cols, rows, take);
            let mut cells = Vec::with_capacity(rects.len());
            for (i, c) in rects.iter().enumerate() {
                let gi = cand[i];
                let ia = aspects[gi];
                let (dx, dy, dw, dh) = draw_rect(*c, ia, opts.mode);
                cells.push(CellLayout {
                    image: gi,
                    x: c.x,
                    y: c.y,
                    w: c.w,
                    h: c.h,
                    dx,
                    dy,
                    dw,
                    dh,
                });
                used[gi] = true;
            }
            let cell_area = rects.first().map(|r| r.w * r.h).unwrap_or(0.0);
            let empty = (cols * rows).saturating_sub(rects.len()) as f32;
            let fill = 1.0 - (empty * cell_area) / inner_area;
            fill_sum += fill as f64;
            pages.push(PageLayout { cells, fill });

            progress(LayoutProgress {
                current: p + 1,
                total: n_pages,
                percent: 100.0 * (p + 1) as f32 / n_pages.max(1) as f32,
                elapsed_ms: t0.elapsed().as_millis() as u64,
                message: format!("排版 {}/{} 页", p + 1, n_pages),
            });
            continue;
        }

        let ctx = Ctx {
            inner,
            gutter: g,
            page_area: inner_area,
        };

        let mut page_imgs: Vec<u32> = cand.iter().map(|&i| i as u32).collect();
        let mut slot_asp: Vec<f32> = page_imgs.iter().map(|&i| aspects[i as usize]).collect();
        let slots: Vec<u32> = (0..take as u32).collect();

        let rects: Vec<(u32, Rect)>;
        let energy;
        if opts.layout == LayoutKind::Grid {
            // 等面积不裁剪：直接算格子，不走切分树也不退火
            let (mut cs, used_h) = layout::uniform_cells(&slots, &slot_asp, inner, g);
            // uniform_cells 已约束在内框中，剩余留白垂直均分，绝不借用页边距。
            let pad = ((inner.h - used_h) * 0.5).max(0.0);
            for (_, r) in cs.iter_mut() {
                r.y += pad;
            }
            let covered: f32 = cs.iter().map(|(_, r)| r.w * r.h).sum();
            energy = (ctx.page_area - covered).max(0.0);
            rects = cs;
        } else {
            let mut tree = layout::seed_best(&slots, &slot_asp, inner, &ctx);
            if take > 1 {
            anneal(
                &mut tree,
                &ctx,
                &mut slot_asp,
                &mut page_imgs,
                opts.iters,
                &mut rng,
                0.0025,
                1e-6,
            );
            pool_refine(
                &mut tree,
                &ctx,
                &mut slot_asp,
                &mut page_imgs,
                &aspects,
                &used,
                400,
                &mut rng,
                3,
            );
            energy = anneal(
                &mut tree,
                &ctx,
                &mut slot_asp,
                &mut page_imgs,
                opts.iters / 3,
                &mut rng,
                0.0008,
                1e-6,
            );
        } else {
            energy = ctx.energy(&tree, &slot_asp);
        }
            let mut v = Vec::new();
            tree.collect(inner, &mut v);
            rects = v
                .into_iter()
                .map(|(s, r)| (s, layout::leaf_cell_pub(r, g, inner)))
                .collect();
        }

        for &gidx in &page_imgs {
            used[gidx as usize] = true;
        }
        let fill = 1.0 - energy / ctx.page_area;
        fill_sum += fill as f64;

        let mut cells = Vec::with_capacity(rects.len());
        for (slot, c) in &rects {
            let c = *c;
            let gi = page_imgs[*slot as usize] as usize;
            let ia = aspects[gi];
            let draw_mode = if opts.layout == LayoutKind::Grid { Mode::Fit } else { opts.mode };
            let (dx, dy, dw, dh) = draw_rect(c, ia, draw_mode);
            cells.push(CellLayout {
                image: gi,
                x: c.x,
                y: c.y,
                w: c.w,
                h: c.h,
                dx,
                dy,
                dw,
                dh,
            });
        }
        pages.push(PageLayout { cells, fill });

        progress(LayoutProgress {
            current: p + 1,
            total: n_pages,
            percent: 100.0 * (p + 1) as f32 / n_pages.max(1) as f32,
            elapsed_ms: t0.elapsed().as_millis() as u64,
            message: format!("排版 {}/{} 页", p + 1, n_pages),
        });
    }

    let ms_layout = t0.elapsed().as_millis() as u64;
    let avg_fill = fill_sum / pages.len().max(1) as f64;
    Ok(LayoutDoc {
        page_w_mm: pw,
        page_h_mm: ph,
        margin_mm: opts.margin_mm,
        gutter_mm: opts.gutter_mm,
        mode: opts.mode,
        quality: opts.quality,
        dpi: opts.dpi,
        per_page,
        images: scan.images.clone(),
        pages,
        fill_avg: avg_fill as f32,
        ms_scan: scan.ms_scan,
        ms_layout,
    })
}

// ---------- 导出 ----------

struct UniqueImage {
    path: PathBuf,
    target: Option<(u32, u32)>,
    source_bytes: u64,
}

struct ExportMeter {
    pages_done: usize,
    pages_total: usize,
    bytes_written: u64,
    total_source: u64,
    processed_source: u64,
    last_emit: Instant,
    t0: Instant,
}

impl ExportMeter {
    fn emit(
        &mut self,
        force: bool,
        current_file: Option<String>,
        progress: &(dyn Fn(ExportProgress) + Sync),
    ) {
        let elapsed = self.t0.elapsed();
        if !force && self.last_emit.elapsed() < Duration::from_millis(50) {
            return;
        }
        self.last_emit = Instant::now();
        let secs = elapsed.as_secs_f64().max(1e-6);
        let percent = if self.total_source > 0 {
            100.0 * self.processed_source as f32 / self.total_source as f32
        } else {
            0.0
        };
        progress(ExportProgress {
            pages_done: self.pages_done,
            pages_total: self.pages_total,
            bytes_written: self.bytes_written,
            bytes_total: self.total_source,
            speed_bps: self.bytes_written as f64 / secs,
            percent: percent.min(100.0),
            elapsed_ms: elapsed.as_millis() as u64,
            current_file,
            message: format!(
                "写入 PDF {}/{} 页 · {:.1} MB/s",
                self.pages_done,
                self.pages_total,
                self.bytes_written as f64 / 1048576.0 / secs
            ),
        });
    }
}

/// 用同一份 [`LayoutDoc`] 流式写出 PDF。
///
/// 生产者：rayon 线程并行「读文件 → 解码/缩放/编码」；
/// 消费者：当前线程顺序写盘并记录偏移。队列有界，内存峰值与图片总数无关。
pub fn export_pdf(
    layout: Arc<LayoutDoc>,
    out: &Path,
    cancel: Arc<AtomicBool>,
    progress: &(dyn Fn(ExportProgress) + Sync),
) -> Result<ExportResult> {
    let t0 = Instant::now();

    // 1. 按首次出现顺序收集唯一图片，并计算每张图的 DPI 目标像素
    let mut unique_of: Vec<usize> = vec![usize::MAX; layout.images.len()];
    let mut unique: Vec<UniqueImage> = Vec::new();
    let mut need: Vec<(f32, f32)> = Vec::new();
    let mut page_unique: Vec<Vec<usize>> = Vec::with_capacity(layout.pages.len());
    let mut unique_pages: Vec<Vec<usize>> = Vec::new();

    if let Some(dpi) = layout.dpi {
        need.resize(layout.images.len(), (0.0, 0.0));
        for page in &layout.pages {
            for c in &page.cells {
                let w = c.dw / 25.4 * dpi as f32;
                let h = c.dh / 25.4 * dpi as f32;
                let e = &mut need[c.image];
                e.0 = e.0.max(w);
                e.1 = e.1.max(h);
            }
        }
    }

    for (p, page) in layout.pages.iter().enumerate() {
        let mut ids = Vec::with_capacity(page.cells.len());
        for c in &page.cells {
            let uid = if unique_of[c.image] == usize::MAX {
                let meta = &layout.images[c.image];
                let target = if layout.dpi.is_some() {
                    dpi_target(meta.width, meta.height, need[c.image].0, need[c.image].1)
                } else {
                    None
                };
                let uid = unique.len();
                unique_of[c.image] = uid;
                unique.push(UniqueImage {
                    path: meta.path.clone(),
                    target,
                    source_bytes: meta.bytes,
                });
                unique_pages.push(Vec::new());
                uid
            } else {
                unique_of[c.image]
            };
            if !unique_pages[uid].contains(&p) {
                unique_pages[uid].push(p);
            }
            ids.push(uid);
        }
        page_unique.push(ids);
    }

    let pages_total = layout.pages.len();
    let total_source: u64 = unique.iter().map(|u| u.source_bytes).sum();
    let mut remaining: Vec<usize> = page_unique.iter().map(|v| v.len()).collect();
    let mut page_done: Vec<bool> = vec![false; pages_total];

    let mut writer = PdfWriter::new(out, layout.page_w_mm, layout.page_h_mm)
        .with_context(|| format!("创建 PDF 失败（可能无写权限）：{}", out.display()))?;

    let mut refs: Vec<Option<pdf::ImageRef>> = (0..unique.len()).map(|_| None).collect();
    let mut skipped = 0usize;
    let mut fatal: Option<anyhow::Error> = None;
    let mut meter = ExportMeter {
        pages_done: 0,
        pages_total,
        bytes_written: 0,
        total_source,
        processed_source: 0,
        last_emit: Instant::now(),
        t0,
    };

    // 4 worker + 队列 4：无损档里 WebP 需要解码，单张 4000px 图解码后可达几十 MB。
    // 峰值内存约 400MB，换取约 2 倍导出吞吐（2000 张 6.2GB 实测 ~140MB/s）。
    let (tx, rx) = crossbeam_channel::bounded::<(usize, Result<encode::PreparedImage>)>(4);
    let pool = build_pool(worker_count().min(4));
    let quality = layout.quality;
    let cancel_prod = cancel.clone();

    meter.emit(true, None, progress);

    pool.scope(|s| {
        for (idx, u) in unique.iter().enumerate() {
            let tx = tx.clone();
            let c = cancel_prod.clone();
            let path = u.path.clone();
            let target = u.target;
            s.spawn(move |_| {
                if c.load(Ordering::Relaxed) {
                    return;
                }
                let r = prepare(&path, target, quality);
                let _ = tx.send((idx, r));
            });
        }
        drop(tx);

        let mut received = 0usize;
        while received < unique.len() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let msg = match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(m) => m,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            };
            let (idx, res) = msg;
            received += 1;
            let current = unique[idx].path.file_name().map(|s| s.to_string_lossy().to_string());

            match res {
                Ok(img) => {
                    let source = img.source_bytes;
                    let written = img.data.len() as u64
                        + img.smask.as_ref().map(|s| s.data.len() as u64).unwrap_or(0);
                    match writer.write_image(&img) {
                        Ok(r) => {
                            refs[idx] = Some(r);
                            meter.bytes_written += written;
                        }
                        Err(e) => {
                            fatal = Some(anyhow::anyhow!("写入 PDF 失败：{}", e));
                            break;
                        }
                    }
                    meter.processed_source += source;
                }
                Err(_) => {
                    skipped += 1;
                    meter.processed_source += unique[idx].source_bytes;
                }
            }

            let mut page_completed = false;
            for &pg in &unique_pages[idx] {
                if remaining[pg] > 0 {
                    remaining[pg] -= 1;
                    if remaining[pg] == 0 && !page_done[pg] {
                        page_done[pg] = true;
                        meter.pages_done += 1;
                        page_completed = true;
                    }
                }
            }
            meter.emit(page_completed || received == unique.len(), current, progress);
        }
    });

    if let Some(e) = fatal {
        writer.abort();
        return Err(e);
    }
    if cancel.load(Ordering::Relaxed) {
        writer.abort();
        bail!("已取消");
    }

    // 2. 图片全部落盘后，再写每页的内容流和页面对象（内容流很小）
    for (p, page) in layout.pages.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            writer.abort();
            bail!("已取消");
        }
        let mut cells = Vec::with_capacity(page.cells.len());
        for (ci, c) in page.cells.iter().enumerate() {
            let uid = page_unique[p][ci];
            if let Some(ir) = refs[uid].as_ref() {
                cells.push(PageCell {
                    name: ir.name.clone(),
                    x: c.x,
                    y: c.y,
                    w: c.w,
                    h: c.h,
                    dx: c.dx,
                    dy: c.dy,
                    dw: c.dw,
                    dh: c.dh,
                });
            }
        }
        writer
            .write_page(&cells)
            .with_context(|| format!("写第 {} 页失败", p + 1))?;
    }

    let bytes = writer
        .finish()
        .with_context(|| format!("PDF 落盘失败：{}", out.display()))?;
    let ms = t0.elapsed().as_millis() as u64;
    let secs = t0.elapsed().as_secs_f64().max(1e-6);

    progress(ExportProgress {
        pages_done: pages_total,
        pages_total,
        bytes_written: bytes,
        bytes_total: total_source,
        speed_bps: bytes as f64 / secs,
        percent: 100.0,
        elapsed_ms: ms,
        current_file: None,
        message: format!("完成：{} 页 · {:.1} MB/s", pages_total, bytes as f64 / 1048576.0 / secs),
    });

    Ok(ExportResult {
        pages: pages_total,
        bytes,
        ms,
        speed_bps: bytes as f64 / secs,
        skipped,
        pdf_path: out.to_string_lossy().to_string(),
    })
}

/// 预览缓存根目录由调用方（Tauri 便携目录）决定。
pub fn make_preview_cache(root: &Path, layout: &LayoutDoc) -> PreviewCache {
    PreviewCache::for_layout(root, layout)
}

pub fn preview_page_pixels(w_mm: f32, h_mm: f32) -> (u32, u32) {
    page_pixels(w_mm, h_mm)
}

// ---------- CLI 兼容入口 ----------

/// 任务阶段（CLI 进度）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Scanning,
    Layout,
    WritingPdf,
    Done,
}

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub stage: Stage,
    pub current: usize,
    pub total: usize,
    pub percent: f32,
    pub message: String,
}

/// 按长宽比聚簇排序：ln(比例) 分箱，箱宽约 3%，数量多的箱排前面。
/// 同一箱内按比例排序。这样最集中的比例先排满前几页，零散比例自动落到最后几页。
fn cluster_order(aspects: &[f32], by_asp: &[usize]) -> Vec<usize> {
    let keyed: Vec<(i32, usize)> = by_asp
        .iter()
        .map(|&i| ((aspects[i].max(1e-3).ln() / 0.03).round() as i32, i))
        .collect();
    let mut counts: Vec<(i32, usize)> = Vec::new();
    for (k, _) in &keyed {
        match counts.iter_mut().find(|(kk, _)| kk == k) {
            Some(e) => e.1 += 1,
            None => counts.push((*k, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut out = Vec::with_capacity(by_asp.len());
    for (k, _) in counts {
        let mut v: Vec<usize> = keyed
            .iter()
            .filter(|(kk, _)| *kk == k)
            .map(|(_, i)| *i)
            .collect();
        v.sort_by(|&x, &y| {
            aspects[x]
                .partial_cmp(&aspects[y])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.extend(v);
    }
    out
}

#[derive(Clone, Debug)]
pub struct JobOptions {
    pub dir: PathBuf,
    pub page: PageKind,
    pub margin_mm: f32,
    pub gutter_mm: f32,
    pub per_page: usize,
    /// 逐页张数方案（各页可不等）
    pub per_page_plan: Option<Vec<usize>>,
    pub pages: Option<usize>,
    pub layout: LayoutKind,
    pub mode: Mode,
    pub quality: Quality,
    pub dpi: Option<u32>,
    pub out_pdf: Option<PathBuf>,
    pub out_json: Option<PathBuf>,
    pub seed: u64,
    pub iters: usize,
    pub even: bool,
    pub cluster: bool,
    pub limit: Option<usize>,
    pub min_w: Option<u32>,
    pub min_h: Option<u32>,
    pub ar_min: Option<f32>,
    pub ar_max: Option<f32>,
}

impl Default for JobOptions {
    fn default() -> Self {
        JobOptions {
            dir: PathBuf::from("."),
            page: PageKind::A4P,
            margin_mm: 10.0,
            gutter_mm: 2.0,
            per_page: 16,
            per_page_plan: None,
            pages: None,
            layout: LayoutKind::Grid,
            mode: Mode::Cover,
            quality: Quality::Lossless,
            dpi: None,
            out_pdf: None,
            out_json: None,
            seed: 42,
            iters: 40_000,
            even: true,
            cluster: true,
            limit: None,
            min_w: None,
            min_h: None,
            ar_min: None,
            ar_max: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct JobResult {
    pub images: usize,
    pub pages: usize,
    pub per_page: usize,
    pub fill_avg: f32,
    pub ms_scan: u64,
    pub ms_layout: u64,
    pub ms_pdf: u64,
    pub pdf_bytes: u64,
    pub pdf_path: Option<String>,
    pub page_w_mm: f32,
    pub page_h_mm: f32,
}

pub fn run_job(opts: JobOptions, progress: impl Fn(Progress) + Sync) -> Result<JobResult> {
    let cancel = Arc::new(AtomicBool::new(false));

    let mut scan = scan_dir(&opts.dir, cancel.clone(), &|p| {
        progress(Progress {
            stage: Stage::Scanning,
            current: p.scanned,
            total: p.total,
            percent: p.percent * 0.15,
            message: p.message,
        })
    })?;
    if let Some(l) = opts.limit {
        scan.images.truncate(l);
        scan.files = scan.images.len();
    }
    if scan.images.is_empty() {
        bail!("目录里没找到可用图片：{}", opts.dir.display());
    }

    let lo = LayoutOptions {
        page: opts.page,
        margin_mm: opts.margin_mm,
        gutter_mm: opts.gutter_mm,
        per_page: opts.per_page,
        per_page_plan: opts.per_page_plan.clone(),
        pages: opts.pages,
        layout: opts.layout,
        mode: opts.mode,
        quality: opts.quality,
        dpi: opts.dpi,
        seed: opts.seed,
        iters: opts.iters,
        even: opts.even,
        cluster: opts.cluster,
        min_w: opts.min_w,
        min_h: opts.min_h,
        ar_min: opts.ar_min,
        ar_max: opts.ar_max,
    };
    let layout = build_layout(&scan, &lo, cancel.clone(), &|p| {
        progress(Progress {
            stage: Stage::Layout,
            current: p.current,
            total: p.total,
            percent: 15.0 + p.percent * 0.8,
            message: p.message,
        })
    })?;

    if let Some(out) = opts.out_json.as_ref() {
        if let Some(parent) = out.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).ok();
            }
        }
        let json = serde_json::to_vec(&layout).context("版面 JSON 序列化失败")?;
        std::fs::write(out, json)
            .with_context(|| format!("版面数据写入失败：{}", out.display()))?;
    }

    let mut ms_pdf = 0u64;
    let mut pdf_bytes = 0u64;
    let mut pdf_path = None;
    if let Some(path) = opts.out_pdf.as_ref() {
        progress(Progress {
            stage: Stage::WritingPdf,
            current: 0,
            total: layout.pages.len(),
            percent: 95.0,
            message: "写入 PDF…".to_string(),
        });
        let doc = Arc::new(layout.clone());
        let r = export_pdf(doc, path, cancel.clone(), &|p| {
            progress(Progress {
                stage: Stage::WritingPdf,
                current: p.pages_done,
                total: p.pages_total,
                percent: 95.0 + p.percent * 0.05,
                message: p.message,
            })
        })?;
        ms_pdf = r.ms;
        pdf_bytes = r.bytes;
        pdf_path = Some(r.pdf_path);
    }

    let pages = layout.pages.len();
    progress(Progress {
        stage: Stage::Done,
        current: pages,
        total: pages,
        percent: 100.0,
        message: format!("完成：{} 页", pages),
    });

    Ok(JobResult {
        images: scan.images.len(),
        pages,
        per_page: layout.per_page,
        fill_avg: layout.fill_avg,
        ms_scan: scan.ms_scan,
        ms_layout: layout.ms_layout,
        ms_pdf,
        pdf_bytes,
        pdf_path,
        page_w_mm: layout.page_w_mm,
        page_h_mm: layout.page_h_mm,
    })
}

/// 估算命令需要的图片数（供前端显示）
pub fn estimate(scan: &ScanResult, opts: &LayoutOptions) -> Estimate {
    estimate_size(scan, opts)
}
