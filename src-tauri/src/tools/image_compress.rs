//! 图片压缩工具：JPG/PNG → AVIF/JXL
//!
//! 纯本地离线处理。JPG/JPEG → JXL 的“原始 JPEG 无损封装”优先使用
//! libjxl/cjxl sidecar（可由 djxl 还原原 JPEG）；其他 JXL 场景使用 jxl-encoder
//! （纯 Rust，VarDCT+Modular 双模），按 distance 划分三档：
//!   - 无损 (d=0)        → LosslessConfig (Modular)
//!   - 近无损 (d=1.0)    → LossyConfig::new(1.0) (VarDCT，视觉无损)
//!   - 有损 (d=1.0~15)   → LossyConfig::new(distance) (VarDCT)
//!
//! HDR：JXL 路径支持 BT.2100 PQ 编码（输入需 16-bit）；AVIF 编码器（ravif）
//! 暂不支持 PQ/HLG 传输函数，keep_hdr 在 AVIF 模式下无效。

extern crate image as image_crate;

use image_crate::imageops::FilterType;
use image_crate::{DynamicImage, GenericImageView, ImageReader};
use imgref::Img;
use jxl_encoder::{LosslessConfig, LossyConfig, PixelLayout};
use rayon::prelude::*;
use rgb::{FromSlice, RGB8, RGBA8};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use tauri::Emitter;
use walkdir::WalkDir;
use zip::write::FileOptions;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Jxl,
    Avif,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressMode {
    Lossless,
    NearLossless,
    Lossy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressConfig {
    pub output_format: OutputFormat,
    pub mode: CompressMode,
    pub quality: u8,
    /// JXL 有损档的 butteraugli distance（1.0=视觉无损，越大体积越小）。
    /// 近无损档固定 1.0；无损档忽略。
    #[serde(default)]
    pub distance: f32,
    pub keep_exif: bool,
    pub max_dimension: u32,
    pub output_dir: String,
    pub zip_output: bool,
    pub jxl_effort: u8,
    pub jxl_jpeg_lossless: bool,
    pub avif_color_quality: u8,
    pub avif_alpha_quality: u8,
    pub keep_hdr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressInput {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
    pub has_alpha: bool,
    pub has_exif: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressAnalysis {
    pub files: Vec<CompressInput>,
    pub skipped: Vec<String>,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressResult {
    pub input_path: String,
    pub output_path: Option<String>,
    pub success: bool,
    pub original_size: u64,
    pub output_size: u64,
    pub compression_ratio: f32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressSummary {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub original_total_size: u64,
    pub output_total_size: u64,
    pub compression_ratio: f32,
    pub zip_path: Option<String>,
    pub results: Vec<CompressResult>,
}

#[derive(Debug, Clone)]
struct PlannedOutput {
    input: CompressInput,
    output_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressProgress {
    pub current: usize,
    pub total: usize,
    pub current_file: String,
    pub phase: String,
}

static RESOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_resource_dir(path: PathBuf) {
    let _ = RESOURCE_DIR.set(path);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibjxlStatus {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub message: String,
}

fn cjxl_exe_name() -> &'static str {
    if cfg!(windows) { "cjxl.exe" } else { "cjxl" }
}

fn cjxl_candidates() -> Vec<PathBuf> {
    let exe = cjxl_exe_name();
    let mut candidates = Vec::new();
    if let Some(res_dir) = RESOURCE_DIR.get() {
        candidates.push(res_dir.join(exe));
        candidates.push(res_dir.join("bin").join("libjxl").join(exe));
        candidates.push(res_dir.join("libjxl").join(exe));
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            candidates.push(dir.join(exe));
            candidates.push(dir.join("bin").join("libjxl").join(exe));
            candidates.push(dir.join("resources").join(exe));
            candidates.push(dir.join("resources").join("bin").join("libjxl").join(exe));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("src-tauri").join("bin").join("libjxl").join(exe));
        candidates.push(cwd.join("bin").join("libjxl").join(exe));
        candidates.push(cwd.join(exe));
    }
    candidates.push(PathBuf::from(exe));
    candidates
}

fn run_command_hidden(cmd: &mut Command) -> std::io::Result<std::process::Output> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output()
}

fn cjxl_version(path: &Path) -> Option<String> {
    let out = run_command_hidden(Command::new(path).arg("--version")).ok()?;
    if !out.status.success() {
        return None;
    }
    let text = if out.stdout.is_empty() { out.stderr } else { out.stdout };
    let version = String::from_utf8_lossy(&text).trim().to_string();
    if version.is_empty() { None } else { Some(version) }
}

fn cjxl_is_usable(path: &Path) -> bool {
    cjxl_version(path).is_some()
}

fn find_cjxl() -> Option<PathBuf> {
    cjxl_candidates().into_iter().find(|p| cjxl_is_usable(p))
}

#[tauri::command]
pub async fn get_libjxl_status() -> Result<LibjxlStatus, String> {
    Ok(match find_cjxl() {
        Some(path) => LibjxlStatus {
            available: true,
            version: cjxl_version(&path),
            path: Some(path.to_string_lossy().to_string()),
            message: "libjxl/cjxl 可用，JPG/JPEG 可启用原始 JPEG 无损封装".into(),
        },
        None => LibjxlStatus {
            available: false,
            path: None,
            version: None,
            message: "未找到 cjxl；请将 libjxl 的 cjxl.exe 放到 src-tauri/bin/libjxl/ 或打包资源目录".into(),
        },
    })
}

fn is_supported_input(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()).as_deref(),
        Some("jpg" | "jpeg" | "png")
    )
}

fn analyze_one(path: &Path) -> Result<CompressInput, String> {
    // 导入阶段只读尺寸（不解码像素），大幅提升大文件夹导入速度。
    // alpha 的精确判断推迟到编码阶段（编码时本来就要解码）。
    let (width, height) = image_crate::image_dimensions(path)
        .map_err(|e| format!("无法读取图片尺寸: {e}"))?;
    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    // 启发式：PNG 可能有 alpha，JPG/JPEG 一定无 alpha。
    let has_alpha = matches!(ext.as_str(), "png");
    // MVP：仅记录可能有 EXIF；实际保留 EXIF 依赖编码器能力，AVIF 路径暂不写回 EXIF。
    let has_exif = matches!(ext.as_str(), "jpg" | "jpeg");
    Ok(CompressInput {
        path: path.to_string_lossy().to_string(),
        name: path.file_name().and_then(|n| n.to_str()).unwrap_or("image").to_string(),
        ext,
        width,
        height,
        file_size,
        has_alpha,
        has_exif,
    })
}

#[tauri::command]
pub async fn analyze_compress_inputs(paths: Vec<String>, recursive: bool) -> Result<CompressAnalysis, String> {
    tokio::task::spawn_blocking(move || {
        let mut candidates = Vec::<PathBuf>::new();
        let mut skipped = Vec::<String>::new();
        for raw in paths {
            let p = PathBuf::from(raw);
            if p.is_file() {
                if is_supported_input(&p) { candidates.push(p); }
                else { skipped.push(p.to_string_lossy().to_string()); }
            } else if p.is_dir() {
                if recursive {
                    for entry in WalkDir::new(&p).follow_links(false).into_iter().filter_map(Result::ok) {
                        let path = entry.path().to_path_buf();
                        if path.is_file() {
                            if is_supported_input(&path) { candidates.push(path); }
                            else { skipped.push(path.to_string_lossy().to_string()); }
                        }
                    }
                } else {
                    for entry in std::fs::read_dir(&p).map_err(|e| e.to_string())? {
                        let path = entry.map_err(|e| e.to_string())?.path();
                        if path.is_file() {
                            if is_supported_input(&path) { candidates.push(path); }
                            else { skipped.push(path.to_string_lossy().to_string()); }
                        }
                    }
                }
            } else {
                skipped.push(p.to_string_lossy().to_string());
            }
        }
        candidates.sort();
        candidates.dedup();
        let results: Vec<Result<CompressInput, String>> = candidates.par_iter().map(|p| analyze_one(p)).collect();
        let mut files = Vec::new();
        for (path, res) in candidates.iter().zip(results) {
            match res {
                Ok(info) => files.push(info),
                Err(e) => skipped.push(format!("{} ({})", path.display(), e)),
            }
        }
        let total_size = files.iter().map(|f| f.file_size).sum();
        Ok(CompressAnalysis { files, skipped, total_size })
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

fn resize_if_needed(img: DynamicImage, max_dimension: u32) -> DynamicImage {
    if max_dimension == 0 { return img; }
    let (w, h) = img.dimensions();
    let long = w.max(h);
    if long <= max_dimension { return img; }
    let scale = max_dimension as f64 / long as f64;
    let nw = ((w as f64 * scale).round() as u32).max(1);
    let nh = ((h as f64 * scale).round() as u32).max(1);
    img.resize(nw, nh, FilterType::Lanczos3)
}

fn unique_output_path(dir: &Path, stem: &str, ext: &str, reserved: &mut HashSet<PathBuf>) -> PathBuf {
    for i in 0..10_000 {
        let file_name = if i == 0 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem} ({i}).{ext}")
        };
        let p = dir.join(file_name);
        if !p.exists() && reserved.insert(p.clone()) {
            return p;
        }
    }
    dir.join(format!("{stem} (10000).{ext}"))
}

fn plan_outputs(files: Vec<CompressInput>, config: &CompressConfig) -> Result<Vec<PlannedOutput>, String> {
    let out_dir = PathBuf::from(&config.output_dir);
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("创建输出目录失败: {e}"))?;
    let out_ext = match config.output_format { OutputFormat::Jxl => "jxl", OutputFormat::Avif => "avif" };
    let mut reserved = HashSet::<PathBuf>::new();
    Ok(files
        .into_iter()
        .map(|input| {
            let input_path = PathBuf::from(&input.path);
            let stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
            let output_path = unique_output_path(&out_dir, stem, out_ext, &mut reserved)
                .to_string_lossy()
                .to_string();
            PlannedOutput { input, output_path }
        })
        .collect())
}

fn write_new_file(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("写入失败: {e}"))?;
    file.write_all(data).map_err(|e| format!("写入失败: {e}"))
}

/// 返回 CPU 逻辑核数的一半，用于限制外层图片并发，给编码器内层并行留出线程。
fn num_cpus_half() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get() / 2)
        .unwrap_or(1)
}

fn can_use_jpeg_lossless(input: &CompressInput, config: &CompressConfig) -> bool {
    matches!(config.output_format, OutputFormat::Jxl)
        && config.jxl_jpeg_lossless
        && config.max_dimension == 0
        && matches!(input.ext.as_str(), "jpg" | "jpeg")
}

fn encode_jpeg_lossless_with_cjxl(input_path: &Path, out_path: &Path, effort: u8) -> Result<u64, String> {
    let cjxl = find_cjxl().ok_or_else(|| "未找到 cjxl，无法执行 JPEG 原始无损封装".to_string())?;
    let effort = effort.clamp(1, 10).to_string();
    let mut cmd = Command::new(&cjxl);
    cmd.arg(input_path)
        .arg(out_path)
        .arg("--lossless_jpeg=1")
        .arg("-e")
        .arg(&effort);
    let output = run_command_hidden(&mut cmd)
        .map_err(|e| format!("启动 cjxl 失败: {e}"))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(out_path);
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(format!("cjxl JPEG 无损封装失败: {detail}"));
    }
    std::fs::metadata(out_path).map(|m| m.len()).map_err(|e| e.to_string())
}

fn compress_one(plan: &PlannedOutput, config: &CompressConfig) -> CompressResult {
    let input = &plan.input;
    let original_size = input.file_size;
    let input_path = PathBuf::from(&input.path);
    let out_path = PathBuf::from(&plan.output_path);

    if can_use_jpeg_lossless(input, config) {
        let res = encode_jpeg_lossless_with_cjxl(&input_path, &out_path, config.jxl_effort);
        return match res {
            Ok(output_size) => CompressResult {
                input_path: input.path.clone(),
                output_path: Some(out_path.to_string_lossy().to_string()),
                success: true,
                original_size,
                output_size,
                compression_ratio: if original_size > 0 { output_size as f32 / original_size as f32 } else { 0.0 },
                error: None,
            },
            Err(e) => CompressResult {
                input_path: input.path.clone(),
                output_path: None,
                success: false,
                original_size,
                output_size: 0,
                compression_ratio: 0.0,
                error: Some(e),
            },
        };
    }

    match config.output_format {
        OutputFormat::Jxl => {
            let res = (|| -> Result<u64, String> {
                // 只有 libjxl/cjxl 路径支持 JPEG 原始无损封装；当前像素编码路径
                // 用于 PNG、缩放后的 JPEG、或未启用该选项的 JPEG。
                let img = ImageReader::open(&input_path)
                    .map_err(|e| format!("打开失败: {e}"))?
                    .with_guessed_format()
                    .map_err(|e| format!("格式检测失败: {e}"))?
                    .decode()
                    .map_err(|e| format!("解码失败: {e}"))?;
                let img = resize_if_needed(img, config.max_dimension);
                let (w, h) = img.dimensions();
                let has_alpha = img.color().has_alpha();

                // keep_hdr 在 JXL 路径中表示“保留 16-bit 像素精度”。不要把普通
                // 16-bit SDR PNG 强行标记为 BT.2100 PQ；否则颜色管理会被误导。
                // 当前不会解析输入 ICC/HDR 元数据，统一按编码器默认色彩空间写入。
                let keep_16bit = config.keep_hdr
                    && matches!(
                        img.color(),
                        image_crate::ColorType::Rgb16
                            | image_crate::ColorType::Rgba16
                            | image_crate::ColorType::La16
                            | image_crate::ColorType::L16
                    );

                let (layout, pixels): (PixelLayout, Vec<u8>) = if keep_16bit {
                    if has_alpha {
                        let rgba = img.to_rgba16();
                        // u16 像素按原生字节序展开为 u8（jxl-encoder 按布局解读字节）
                        (PixelLayout::Rgba16, rgba.into_raw().into_iter().flat_map(u16::to_ne_bytes).collect())
                    } else {
                        let rgb = img.to_rgb16();
                        (PixelLayout::Rgb16, rgb.into_raw().into_iter().flat_map(u16::to_ne_bytes).collect())
                    }
                } else if has_alpha {
                    let rgba = img.to_rgba8();
                    (PixelLayout::Rgba8, rgba.into_raw())
                } else {
                    let rgb = img.to_rgb8();
                    (PixelLayout::Rgb8, rgb.into_raw())
                };

                // effort 库支持 1–10，clamp 到合法区间。
                let effort = config.jxl_effort.clamp(1, 10);

                // 三档分流：无损=Modular；近无损/有损=VarDCT(distance)。
                // with_threads(0) 使用环境 rayon 池（由 compress_images 安装的限定池），
                // 不创建专用池，避免每图 ThreadPoolBuilder 开销，并与外层图片并发协作。
                let jxl_bytes = match config.mode {
                    CompressMode::Lossless => {
                        let cfg = LosslessConfig::new().with_effort(effort).with_threads(0);
                        cfg.encode(&pixels, w, h, layout)
                            .map_err(|e| format!("JXL 无损编码失败: {e}"))?
                    }
                    CompressMode::NearLossless => {
                        // d=1.0 是 libjxl 官方视觉无损基准
                        let cfg = LossyConfig::new(1.0).with_effort(effort).with_threads(0);
                        cfg.encode(&pixels, w, h, layout)
                            .map_err(|e| format!("JXL 近无损编码失败: {e}"))?
                    }
                    CompressMode::Lossy => {
                        // distance 1.0–15.0，越大体积越小
                        let distance = config.distance.clamp(1.0, 15.0);
                        let cfg = LossyConfig::new(distance).with_effort(effort).with_threads(0);
                        cfg.encode(&pixels, w, h, layout)
                            .map_err(|e| format!("JXL 有损编码失败 (d={distance}): {e}"))?
                    }
                };

                write_new_file(&out_path, &jxl_bytes)?;
                std::fs::metadata(&out_path).map(|m| m.len()).map_err(|e| e.to_string())
            })();
            match res {
                Ok(output_size) => CompressResult {
                    input_path: input.path.clone(),
                    output_path: Some(out_path.to_string_lossy().to_string()),
                    success: true,
                    original_size,
                    output_size,
                    compression_ratio: if original_size > 0 { output_size as f32 / original_size as f32 } else { 0.0 },
                    error: None,
                },
                Err(e) => CompressResult { input_path: input.path.clone(), output_path: None, success: false, original_size, output_size: 0, compression_ratio: 0.0, error: Some(e) },
            }
        }
        OutputFormat::Avif => {
            let res = (|| -> Result<u64, String> {
                let img = ImageReader::open(&input_path)
                    .map_err(|e| format!("打开失败: {e}"))?
                    .with_guessed_format()
                    .map_err(|e| format!("格式检测失败: {e}"))?
                    .decode()
                    .map_err(|e| format!("解码失败: {e}"))?;
                let img = resize_if_needed(img, config.max_dimension);
                let (w, h) = img.dimensions();
                let has_alpha = img.color().has_alpha();

                // 三档推导色彩/Alpha 质量。ravif 支持分离的 alpha quality，
                // 这是绕过 image::AvifEncoder（锁死 color=alpha）的原因。
                let (color_q, alpha_q): (f32, f32) = match config.mode {
                    CompressMode::Lossless => (100.0, 100.0),
                    // AVIF 无独立近无损档：近无损按高质量有损处理
                    CompressMode::NearLossless => (
                        config.avif_color_quality.clamp(80, 100) as f32,
                        config.avif_alpha_quality.clamp(80, 100) as f32,
                    ),
                    CompressMode::Lossy => (
                        config.avif_color_quality.clamp(1, 100) as f32,
                        config.avif_alpha_quality.clamp(1, 100) as f32,
                    ),
                };

                let encoder = ravif::Encoder::new()
                    .with_quality(color_q)
                    .with_alpha_quality(alpha_q)
                    .with_speed(4) // balanced
                    .with_bit_depth(ravif::BitDepth::Ten); // 10-bit 内部精度，即便 8-bit 输入也更好

                let encoded = if has_alpha {
                    let rgba = img.to_rgba8();
                    let pixels: &[RGBA8] = rgba.as_raw().as_rgba();
                    let img_ref = Img::new(pixels, w as usize, h as usize);
                    encoder
                        .encode_rgba(img_ref)
                        .map_err(|e| format!("AVIF 编码失败: {e}"))
                } else {
                    let rgb = img.to_rgb8();
                    let pixels: &[RGB8] = rgb.as_raw().as_rgb();
                    let img_ref = Img::new(pixels, w as usize, h as usize);
                    encoder
                        .encode_rgb(img_ref)
                        .map_err(|e| format!("AVIF 编码失败: {e}"))
                }?;

                write_new_file(&out_path, &encoded.avif_file)?;
                std::fs::metadata(&out_path).map(|m| m.len()).map_err(|e| e.to_string())
            })();
            match res {
                Ok(output_size) => CompressResult {
                    input_path: input.path.clone(),
                    output_path: Some(out_path.to_string_lossy().to_string()),
                    success: true,
                    original_size,
                    output_size,
                    compression_ratio: if original_size > 0 { output_size as f32 / original_size as f32 } else { 0.0 },
                    error: None,
                },
                Err(e) => CompressResult { input_path: input.path.clone(), output_path: None, success: false, original_size, output_size: 0, compression_ratio: 0.0, error: Some(e) },
            }
        }
    }
}

fn zip_success_outputs(results: &[CompressResult], output_dir: &Path) -> Result<Option<(String, u64)>, String> {
    let success_paths: Vec<PathBuf> = results
        .iter()
        .filter(|r| r.success)
        .filter_map(|r| r.output_path.as_ref())
        .map(PathBuf::from)
        .collect();
    if success_paths.is_empty() {
        return Ok(None);
    }

    let zip_path = unique_zip_path(output_dir);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&zip_path)
        .map_err(|e| format!("创建 ZIP 失败: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for path in success_paths {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
        zip.start_file(name, options).map_err(|e| format!("写入 ZIP 失败: {e}"))?;
        let bytes = std::fs::read(&path).map_err(|e| format!("读取输出文件失败: {e}"))?;
        zip.write_all(&bytes).map_err(|e| format!("写入 ZIP 失败: {e}"))?;
    }
    zip.finish().map_err(|e| format!("完成 ZIP 失败: {e}"))?;
    let size = std::fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0);
    Ok(Some((zip_path.to_string_lossy().to_string(), size)))
}

fn unique_zip_path(output_dir: &Path) -> PathBuf {
    let mut p = output_dir.join("compressed_images.zip");
    if !p.exists() { return p; }
    for i in 1..10_000 {
        p = output_dir.join(format!("compressed_images ({i}).zip"));
        if !p.exists() { return p; }
    }
    output_dir.join("compressed_images (10000).zip")
}

#[tauri::command]
pub async fn compress_images(files: Vec<CompressInput>, config: CompressConfig, window: tauri::WebviewWindow) -> Result<CompressSummary, String> {
    tokio::task::spawn_blocking(move || {
        if files.is_empty() { return Err("没有可转换的图片".to_string()); }
        if config.output_dir.trim().is_empty() { return Err("请选择输出目录".to_string()); }
        let total = files.len();
        let planned = plan_outputs(files, &config)?;
        let counter = std::sync::atomic::AtomicUsize::new(0);
        // 有界并发：外层图片并发数限制为 CPU 核数的一半，给 jxl-encoder 内层并行
        // （with_threads(0) 使用本池）留出线程空间，避免过度订阅。
        // AVIF 路径无内层并行，但 ravif 自带 rayon 池，同样受益于外层减半。
        let outer_threads = (num_cpus_half()).max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(outer_threads)
            .build()
            .map_err(|e| format!("创建线程池失败: {e}"))?;
        let phase = match config.output_format {
            OutputFormat::Jxl => "JXL 编码",
            OutputFormat::Avif => "AVIF 编码",
        };
        let results: Vec<CompressResult> = pool.install(|| {
            planned.par_iter().map(|plan| {
                let result = compress_one(plan, &config);
                let cur = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                let _ = window.emit("image_compress_progress", CompressProgress {
                    current: cur,
                    total,
                    current_file: plan.input.path.clone(),
                    phase: phase.into(),
                });
                result
            }).collect()
        });
        let mut results = results;
        let success = results.iter().filter(|r| r.success).count();
        let mut failed = results.len() - success;
        let original_total_size: u64 = results.iter().map(|r| r.original_size).sum();
        let mut output_total_size: u64 = results.iter().map(|r| r.output_size).sum();
        let mut zip_path = None;
        if config.zip_output {
            let _ = window.emit("image_compress_progress", CompressProgress {
                current: success,
                total,
                current_file: String::new(),
                phase: "打包 ZIP".into(),
            });
            match zip_success_outputs(&results, &PathBuf::from(&config.output_dir)) {
                Ok(Some((path, size))) => {
                    output_total_size += size;
                    zip_path = Some(path);
                }
                Ok(None) => {}
                Err(e) => {
                    failed += 1;
                    results.push(CompressResult {
                        input_path: "ZIP".into(),
                        output_path: None,
                        success: false,
                        original_size: 0,
                        output_size: 0,
                        compression_ratio: 0.0,
                        error: Some(e),
                    });
                }
            }
        }
        Ok(CompressSummary {
            total,
            success,
            failed,
            original_total_size,
            output_total_size,
            compression_ratio: if original_total_size > 0 { output_total_size as f32 / original_total_size as f32 } else { 0.0 },
            zip_path,
            results,
        })
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}
