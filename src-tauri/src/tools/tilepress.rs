//! Tauri 后端：便携路径、配置、四阶段任务命令。

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tilepress::encode::Quality;
use tilepress::pdf::Mode;
use tilepress::preview::{PreviewCache, PreviewImage};
use tilepress::LayoutKind;
use tilepress::{
    build_layout, estimate, export_pdf, make_preview_cache, recommend as recommend_count, scan_dir,
    LayoutDoc, LayoutOptions, LayoutProgress, LayoutSummary, PageKind, PreviewProgress, ScanInfo,
    ScanProgress, ScanResult,
};

// ---------- 便携路径 ----------

#[derive(Clone, Debug, Serialize)]
pub struct PathsInfo {
    pub exe_dir: String,
    pub data_dir: String,
    pub config_path: String,
    pub cache_dir: String,
    pub webview_dir: String,
    pub portable: bool,
}

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub exe_dir: PathBuf,
    pub data_dir: PathBuf,
    pub config_path: PathBuf,
    pub cache_dir: PathBuf,
    pub webview_dir: PathBuf,
    pub preview_root: PathBuf,
    pub portable: bool,
}

impl AppPaths {
    fn info(&self) -> PathsInfo {
        PathsInfo {
            exe_dir: self.exe_dir.to_string_lossy().to_string(),
            data_dir: self.data_dir.to_string_lossy().to_string(),
            config_path: self.config_path.to_string_lossy().to_string(),
            cache_dir: self.cache_dir.to_string_lossy().to_string(),
            webview_dir: self.webview_dir.to_string_lossy().to_string(),
            portable: self.portable,
        }
    }
}

fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".tilepress-write-test-{}", std::process::id()));
    match std::fs::write(&probe, b"ok") {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

pub fn resolve_paths() -> AppPaths {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let portable = dir_writable(&exe_dir);
    let data_dir = if portable {
        exe_dir.clone()
    } else {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        base.join("tilepress")
    };

    let _ = std::fs::create_dir_all(&data_dir);
    let cache_dir = data_dir.join("tilepress-cache");
    let _ = std::fs::create_dir_all(&cache_dir);
    let webview_dir = cache_dir.join("webview2");
    let _ = std::fs::create_dir_all(&webview_dir);
    let preview_root = cache_dir.join("preview");
    let _ = std::fs::create_dir_all(&preview_root);

    AppPaths {
        exe_dir,
        config_path: data_dir.join("tilepress-config.json"),
        data_dir,
        cache_dir,
        webview_dir,
        preview_root,
        portable,
    }
}

// ---------- 偏好设置 ----------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub export_format: String,
    pub image_dpi: Option<u32>,
    pub image_quality: u8,
    pub page: PageKind,
    pub margin_mm: f32,
    pub gutter_mm: f32,
    pub layout: LayoutKind,
    pub mode: Mode,
    pub per_page: Option<usize>,
    pub quality: Quality,
    pub dpi: Option<u32>,
    pub preview_mode: String,
    #[serde(default)]
    pub min_w: Option<u32>,
    #[serde(default)]
    pub min_h: Option<u32>,
    #[serde(default)]
    pub ar_min: Option<f32>,
    #[serde(default)]
    pub ar_max: Option<f32>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            export_format: "pdf".to_string(),
            image_dpi: None,
            image_quality: 100,
            page: PageKind::A4P,
            margin_mm: 10.0,
            gutter_mm: 1.0,
            layout: LayoutKind::Mosaic,
            mode: Mode::Cover,
            per_page: None,
            quality: Quality::Lossless,
            dpi: None,
            preview_mode: "scroll".to_string(),
            min_w: None,
            min_h: None,
            ar_min: None,
            ar_max: None,
        }
    }
}

fn read_prefs(path: &Path) -> Prefs {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write_prefs(path: &Path, prefs: &Prefs) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(prefs).map_err(|e| format!("配置序列化失败：{}", e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("配置写入失败：{}", e))?;
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("配置保存失败：{}", e))
}

// ---------- 应用状态 ----------

struct Inner {
    scan: Option<Arc<ScanResult>>,
    scan_dir: Option<PathBuf>,
    layout: Option<Arc<LayoutDoc>>,
    preview: Option<Arc<PreviewCache>>,
    cancel: Arc<AtomicBool>,
}

pub struct TilepressState {
    paths: AppPaths,
    prefs: Mutex<Prefs>,
    inner: Mutex<Inner>,
}

impl TilepressState {
    pub fn new(paths: AppPaths) -> Self {
        // 启动时清掉上一轮的预览缓存，避免长期占用磁盘
        let _ = std::fs::remove_dir_all(&paths.preview_root);
        let _ = std::fs::create_dir_all(&paths.preview_root);
        // 便携模式首次启动就把配置文件落下来，方便验收
        if !paths.config_path.exists() {
            // 兼容旧名（pipdf-config.json）：有就搬过来，别丢用户偏好
            let legacy = paths.config_path.with_file_name("pipdf-config.json");
            if legacy.exists() {
                let _ = std::fs::rename(&legacy, &paths.config_path);
            } else {
                let _ = write_prefs(&paths.config_path, &Prefs::default());
            }
        }
        let prefs = read_prefs(&paths.config_path);
        TilepressState {
            paths,
            prefs: Mutex::new(prefs),
            inner: Mutex::new(Inner {
                scan: None,
                scan_dir: None,
                layout: None,
                preview: None,
                cancel: Arc::new(AtomicBool::new(false)),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn begin_op(&self) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.lock().cancel = flag.clone();
        flag
    }

    fn cancel(&self) {
        self.lock().cancel.store(true, Ordering::Relaxed);
    }
}

// ---------- 命令 ----------

#[tauri::command]
pub fn app_paths(state: State<'_, TilepressState>) -> PathsInfo {
    state.paths.info()
}

#[tauri::command]
pub fn load_config(state: State<'_, TilepressState>) -> Prefs {
    state
        .prefs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[tauri::command]
pub fn save_config(prefs: Prefs, state: State<'_, TilepressState>) -> Result<(), String> {
    write_prefs(&state.paths.config_path, &prefs)?;
    *state.prefs.lock().unwrap_or_else(|e| e.into_inner()) = prefs;
    Ok(())
}

#[tauri::command]
pub async fn scan_folder(
    dir: String,
    on_progress: Channel<ScanProgress>,
    state: State<'_, TilepressState>,
) -> Result<ScanInfo, String> {
    let cancel = state.begin_op();
    let dir_path = PathBuf::from(&dir);
    let result = tauri::async_runtime::spawn_blocking(move || {
        scan_dir(&dir_path, cancel, &|p| {
            let _ = on_progress.send(p);
        })
    })
    .await
    .map_err(|e| format!("扫描线程异常：{}", e))?
    .map_err(|e| format!("{:#}", e))?;

    let info = result.info();
    {
        let mut inner = state.lock();
        inner.scan = Some(Arc::new(result));
        inner.scan_dir = Some(PathBuf::from(&dir));
        inner.layout = None;
        inner.preview = None;
    }
    Ok(info)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn recommend(
    state: State<'_, TilepressState>,
    page: PageKind,
    margin_mm: f32,
    gutter_mm: f32,
    min_per: Option<usize>,
    max_per: Option<usize>,
    min_w: Option<u32>,
    min_h: Option<u32>,
    ar_min: Option<f32>,
    ar_max: Option<f32>,
) -> tilepress::Recommendation {
    // 推荐只是辅助功能：万一内部断言炸了，也不能把整个窗口带走
    let guard = state.lock();
    let res =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match guard.scan.as_ref() {
            Some(scan) => {
                let kept = tilepress::filter_images(&scan.images, min_w, min_h, ar_min, ar_max);
                tilepress::recommend_bounded(&kept, page, margin_mm, gutter_mm, min_per, max_per)
            }
            None => recommend_count(page, margin_mm, 0),
        }));
    res.unwrap_or_else(|_| recommend_count(page, margin_mm, 0))
}

/// 过滤后还剩多少张（给界面显示用）
#[derive(serde::Serialize)]
pub struct FilterStats {
    total: usize,
    kept: usize,
}

#[tauri::command]
pub fn filter_stats(
    min_w: Option<u32>,
    min_h: Option<u32>,
    ar_min: Option<f32>,
    ar_max: Option<f32>,
    state: State<'_, TilepressState>,
) -> FilterStats {
    let guard = state.lock();
    match guard.scan.as_ref() {
        Some(scan) => {
            let kept = tilepress::filter_images(&scan.images, min_w, min_h, ar_min, ar_max);
            FilterStats {
                total: scan.images.len(),
                kept: kept.len(),
            }
        }
        None => FilterStats { total: 0, kept: 0 },
    }
}

#[tauri::command]
pub fn estimate_export(
    opts: LayoutOptions,
    state: State<'_, TilepressState>,
) -> Result<tilepress::Estimate, String> {
    let inner = state.lock();
    let scan = inner
        .scan
        .as_ref()
        .ok_or_else(|| "请先导入图片文件夹".to_string())?;
    Ok(estimate(scan, &opts))
}

#[tauri::command]
pub async fn build_layout_cmd(
    opts: LayoutOptions,
    on_progress: Channel<LayoutProgress>,
    state: State<'_, TilepressState>,
) -> Result<LayoutSummary, String> {
    let scan = state
        .lock()
        .scan
        .clone()
        .ok_or_else(|| "请先导入图片文件夹".to_string())?;
    let cancel = state.begin_op();
    let layout = tauri::async_runtime::spawn_blocking(move || {
        build_layout(&scan, &opts, cancel, &|p| {
            let _ = on_progress.send(p);
        })
    })
    .await
    .map_err(|e| format!("排版线程异常：{}", e))?
    .map_err(|e| format!("{:#}", e))?;

    let preview = Arc::new(make_preview_cache(&state.paths.preview_root, &layout));
    let summary = LayoutSummary {
        images: layout.images.len(),
        pages: layout.pages.len(),
        per_page: layout.per_page,
        fill_avg: layout.fill_avg,
        page_w_mm: layout.page_w_mm,
        page_h_mm: layout.page_h_mm,
        ms_scan: layout.ms_scan,
        ms_layout: layout.ms_layout,
        total_bytes: layout.images.iter().map(|i| i.bytes).sum(),
    };
    {
        let mut inner = state.lock();
        // 旧版式的预览缓存可以删掉
        if let Some(old) = inner.preview.take() {
            if old.root != preview.root {
                let _ = std::fs::remove_dir_all(&old.root);
            }
        }
        inner.layout = Some(Arc::new(layout));
        inner.preview = Some(preview);
    }
    Ok(summary)
}

#[tauri::command]
pub async fn render_page_preview(
    page_idx: usize,
    state: State<'_, TilepressState>,
) -> Result<PreviewImage, String> {
    let (layout, cache) = {
        let inner = state.lock();
        (
            inner
                .layout
                .clone()
                .ok_or_else(|| "排版尚未完成".to_string())?,
            inner
                .preview
                .clone()
                .ok_or_else(|| "预览缓存未初始化".to_string())?,
        )
    };
    let page = page_idx;
    let (w, h) = tilepress::preview_page_pixels(layout.page_w_mm, layout.page_h_mm);
    let layout_for_render = layout.clone();
    let (path, cached) =
        tauri::async_runtime::spawn_blocking(move || cache.render(&layout_for_render, page))
            .await
            .map_err(|e| format!("预览线程异常：{}", e))?
            .map_err(|e| format!("{:#}", e))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("读取预览图失败：{}", e))?;
    let data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    Ok(PreviewImage {
        page,
        width: w,
        height: h,
        path: path.to_string_lossy().to_string(),
        data_url,
        cached,
    })
}

#[tauri::command]
pub async fn prewarm_previews(
    on_progress: Channel<PreviewProgress>,
    state: State<'_, TilepressState>,
) -> Result<usize, String> {
    let (layout, cache) = {
        let inner = state.lock();
        (
            inner
                .layout
                .clone()
                .ok_or_else(|| "排版尚未完成".to_string())?,
            inner
                .preview
                .clone()
                .ok_or_else(|| "预览缓存未初始化".to_string())?,
        )
    };
    let cancel = state.begin_op();
    tauri::async_runtime::spawn_blocking(move || {
        cache.prewarm(layout, cancel, &|p| {
            let _ = on_progress.send(p);
        })
    })
    .await
    .map_err(|e| format!("预览线程异常：{}", e))?
    .map_err(|e| format!("{:#}", e))
}

#[tauri::command]
pub async fn export_pdf_cmd(
    out_path: String,
    on_progress: Channel<tilepress::ExportProgress>,
    state: State<'_, TilepressState>,
) -> Result<tilepress::ExportResult, String> {
    let layout = state
        .lock()
        .layout
        .clone()
        .ok_or_else(|| "排版尚未完成".to_string())?;
    let cancel = state.begin_op();
    let out = PathBuf::from(&out_path);
    tauri::async_runtime::spawn_blocking(move || {
        export_pdf(layout, &out, cancel, &|p| {
            let _ = on_progress.send(p);
        })
    })
    .await
    .map_err(|e| format!("导出线程异常：{}", e))?
    .map_err(|e| format!("{:#}", e))
}

/// 导出图片的参数
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImgExport {
    fmt: tilepress::imgout::OutFmt,
    dpi: Option<u32>,
    quality: u8,
    out_dir: String,
    base_name: String,
}

#[derive(Clone, serde::Serialize)]
pub struct ImgResult {
    files: Vec<String>,
    bytes: u64,
    pages: usize,
    out_dir: String,
    ms: u64,
    speed_bps: f64,
}

#[tauri::command]
pub async fn export_images_cmd(
    req: ImgExport,
    on_progress: Channel<tilepress::ExportProgress>,
    state: State<'_, TilepressState>,
) -> Result<ImgResult, String> {
    let layout = state
        .lock()
        .layout
        .clone()
        .ok_or_else(|| "排版尚未完成".to_string())?;
    let cancel = state.begin_op();
    let dir = PathBuf::from(&req.out_dir);
    let fmt = req.fmt;
    let dpi = req.dpi.map(|dpi| dpi.clamp(36, 600));
    let quality = req.quality.clamp(10, 100);
    let base = req.base_name.clone();
    // 输出前缀只能是文件名，拒绝路径分隔符，避免意外写入子目录。
    if base.trim().is_empty() || base.chars().any(|c| r#"\/:*?"<>|"#.contains(c)) {
        return Err("输出文件名前缀不合法".into());
    }

    tauri::async_runtime::spawn_blocking(move || {
        use rayon::prelude::*;
        use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
        let start = std::time::Instant::now();
        let total = layout.pages.len();
        let _ = on_progress.send(tilepress::ExportProgress {
            pages_done: 0,
            pages_total: total,
            bytes_written: 0,
            bytes_total: 0,
            speed_bps: 0.0,
            percent: 0.0,
            elapsed_ms: 0,
            current_file: None,
            message: format!("正在合成并编码 {} 图片…", fmt.label()),
        });
        // 每页独立渲染 + 编码，2–4 路并行（每页位图 10–30MB，别开太猛）
        // AVIF 编码器自己会吃满多核，再叠加页级并行反而更慢
        let workers = if fmt == tilepress::imgout::OutFmt::Avif {
            1
        } else {
            tilepress::worker_count().clamp(1, 4)
        };
        let pool = tilepress::build_pool(workers);
        let done = AtomicUsize::new(0);
        let written = AtomicU64::new(0);
        // 多页同时完成时，串行发送进度，避免后到的旧页数把界面进度倒退。
        let progress_lock = Mutex::new(());
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let results: Vec<Result<(String, u64), String>> = pool.install(|| {
            (0..total)
                .into_par_iter()
                .map(|page| {
                    if cancel.load(Ordering::Relaxed) || cancelled.load(Ordering::Relaxed) {
                        cancelled.store(true, Ordering::Relaxed);
                        return Err("已取消".to_string());
                    }
                    let name = if total == 1 {
                        format!("{}.{}", base, fmt.ext())
                    } else {
                        format!("{}_p{}.{}", base, page + 1, fmt.ext())
                    };
                    let path = dir.join(&name);
                    let r = tilepress::imgout::write_page_with_resolution(
                        &layout, page, dpi, fmt, quality, &path,
                    )
                    .map(|(p, n)| (p.display().to_string(), n))
                    .map_err(|e| format!("{:#}", e));
                    if let Ok((_, n)) = &r {
                        let _guard = progress_lock.lock().unwrap_or_else(|e| e.into_inner());
                        let bytes = written.fetch_add(*n, Ordering::Relaxed) + n;
                        let cur = done.fetch_add(1, Ordering::Relaxed) + 1;
                        let elapsed = start.elapsed();
                        let _ = on_progress.send(tilepress::ExportProgress {
                            pages_done: cur,
                            pages_total: total,
                            bytes_written: bytes,
                            bytes_total: if cur == total { bytes } else { 0 },
                            speed_bps: bytes as f64 / elapsed.as_secs_f64().max(1e-6),
                            percent: 100.0 * cur as f32 / total.max(1) as f32,
                            elapsed_ms: elapsed.as_millis() as u64,
                            current_file: Some(name),
                            message: format!("导出 {} 图片 {}/{}", fmt.label(), cur, total),
                        });
                    }
                    r
                })
                .collect()
        });

        let mut files = Vec::with_capacity(total);
        let mut bytes = 0u64;
        for r in results {
            let (f, n) = r?;
            files.push(f);
            bytes += n;
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("已取消".to_string());
        }
        let elapsed = start.elapsed();
        Ok(ImgResult {
            pages: total,
            files,
            bytes,
            out_dir: dir.display().to_string(),
            ms: elapsed.as_millis() as u64,
            speed_bps: bytes as f64 / elapsed.as_secs_f64().max(1e-6),
        })
    })
    .await
    .map_err(|e| format!("导出线程异常：{}", e))?
}

#[tauri::command]
pub fn cancel_current(state: State<'_, TilepressState>) {
    state.cancel();
}

#[tauri::command]
pub fn startup_dir() -> Option<String> {
    std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-') && Path::new(a).is_dir())
}

#[tauri::command]
pub fn open_pdf(app: AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("打开 PDF 失败：{}", e))
}

#[tauri::command]
pub fn reveal_pdf(app: AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| format!("打开文件夹失败：{}", e))
}

#[tauri::command]
pub fn open_output_folder(app: AppHandle, path: String) -> Result<(), String> {
    if !Path::new(&path).is_dir() {
        return Err("图片输出文件夹不存在".into());
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("打开文件夹失败：{}", e))
}

#[tauri::command]
pub fn copy_image_files(paths: Vec<String>) -> Result<(), String> {
    if paths.is_empty() {
        return Err("没有可复制的图片".into());
    }
    for path in &paths {
        if !Path::new(path).is_file() {
            return Err(format!("图片文件不存在：{}", path));
        }
    }
    #[cfg(windows)]
    {
        use clipboard_win::{formats, Clipboard, Setter};
        let _clipboard =
            Clipboard::new_attempts(10).map_err(|e| format!("剪贴板暂时不可用：{}", e))?;
        formats::FileList
            .write_clipboard(paths.as_slice())
            .map_err(|e| format!("复制图片文件失败：{}", e))
    }
    #[cfg(not(windows))]
    Err("当前平台不支持复制图片文件".into())
}
