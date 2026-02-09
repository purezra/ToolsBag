use crate::models::{Convert3Request, Convert3Result, EpubConvertRequest, EpubConvertResult, FileEntry, ImageIssue};
use crate::utils::{emit_progress, ensure_dir, timestamped_log};
use exif::{In, Reader, Tag};
use image::codecs::jpeg::{JpegDecoder, JpegEncoder};
use image::{image_dimensions, ColorType, ExtendedColorType, GenericImageView, ImageBuffer, ImageDecoder, ImageFormat, ImageReader, Luma, LumaA, Rgba, RgbaImage};
use image::imageops;
use jpeg_decoder::{Decoder as RawJpegDecoder, PixelFormat};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use rayon::prelude::*;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tiff::decoder::{Decoder, DecodingResult};
use uuid::Uuid;
use walkdir::WalkDir;
use zip::write::FileOptions;
use zip::ZipWriter;
pub fn list_images(input_dir: PathBuf, recursive: bool) -> Result<Vec<FileEntry>, String> {
  let mut files = Vec::new();
  for entry in WalkDir::new(&input_dir)
    .follow_links(false)
    .into_iter()
    .filter_map(|e| e.ok())
  {
    if entry.file_type().is_file() {
      let path = entry.path().to_path_buf();
      if let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()) {
        if ["jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp"].contains(&ext.as_str()) {
          let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
          files.push(FileEntry {
            name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            path,
            format: ext,
            size: meta.len(),
          });
        }
      }
    }
    if !recursive && entry.depth() > 0 {
      continue;
    }
  }
  Ok(files)
}

struct PageBuf {
  w: u32,
  h: u32,
  data: Vec<u8>,
  filter: Option<&'static str>,
  color_space: &'static str,
  bits: u8,
  smask: Option<Vec<u8>>,
  rotated_90_or_270: bool,
}

pub fn convert(app: AppHandle, req: Convert3Request) -> Result<Convert3Result, String> {
  let id = Uuid::new_v4();
  let input = req.input_dir.clone();
  if !input.exists() {
    return Err(format!("输入目录不存在: {}", input.to_string_lossy()));
  }
  if !input.is_dir() {
    return Err(format!("输入路径不是目录: {}", input.to_string_lossy()));
  }
  let input_name = input.file_name().and_then(|s| s.to_str()).unwrap_or("output");
  let base_dir = input.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
  let batch_size = req.batch_size.unwrap_or(0);
  let is_single = batch_size == 0;

  let requested_output = req.output_dir.clone();
  // Output naming rules:
  // - Single PDF merge (batch_size == 0): place in the input folder's parent with name "<input>_合并.pdf".
  //   If user provides a custom name (file name or path), only the file name is used and ".pdf" is appended if missing.
  // - Batched output keeps existing directory-based behavior.
  let (output_dir, output_path) = if is_single {
    let custom_name = requested_output
      .as_ref()
      .and_then(|p| p.file_name())
      .and_then(|n| n.to_str())
      .map(|name| {
        if name.to_ascii_lowercase().ends_with(".pdf") {
          name.to_string()
        } else {
          format!("{name}.pdf")
        }
      });
    let final_name = custom_name.unwrap_or_else(|| format!("{input_name}_合并.pdf"));
    let dir = base_dir.clone();
    (dir.clone(), dir.join(final_name))
  } else {
    match requested_output {
      Some(p) => {
        if p
          .extension()
          .and_then(|e| e.to_str())
          .map(|e| e.eq_ignore_ascii_case("pdf"))
          .unwrap_or(false)
        {
          let dir = p.parent().map(|v| v.to_path_buf()).unwrap_or_else(|| base_dir.clone());
          (dir.clone(), dir)
        } else {
          (p.clone(), p)
        }
      }
      None => {
        let dir = base_dir.join(format!("{input_name}_合并"));
        (dir.clone(), dir)
      }
    }
  };

  ensure_dir(&output_dir).map_err(|e| format!("创建输出目录失败: {} ({})", output_dir.to_string_lossy(), e))?;
  let log_path = timestamped_log(&output_dir, "conversion3_log");

  let files = list_images(input.clone(), true)?;
  let paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
  if paths.is_empty() {
    return Err("未找到图像文件".into());
  }

  let original_bytes: u64 = files.iter().map(|f| f.size).sum();
  let mut output_bytes: u64 = 0;
  let mut problems = Vec::new();
  let warnings: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

  if is_single {
    let total = paths.len().max(1);
    let mut doc = Document::with_version("1.5");
    let mut page_ids: Vec<ObjectId> = Vec::new();
    let results = paths
      .par_iter()
      .enumerate()
      .map(|(idx, p)| {
        let w = warnings.clone();
        prepare_pages(p, idx, &app, id, total, &w)
      })
      .collect::<Vec<_>>();
    for res in results {
      match res {
        Ok(list) => {
          for page_buf in list {
            insert_page(&mut doc, page_buf, &mut page_ids)
          }
        }
        Err(issue) => problems.push(issue),
      }
    }
    build_catalog(&mut doc, &page_ids)?;
    let mut bufw = BufWriter::new(File::create(&output_path).map_err(|e| e.to_string())?);
    doc.save_to(&mut bufw).map_err(|e| e.to_string())?;
    output_bytes = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
  } else {
    let total = paths.len();
    let bs = batch_size.max(1);
    let batches = (total + bs - 1) / bs;
    for (batch_idx, chunk) in paths.chunks(bs).enumerate() {
      emit_progress(
        &app,
        "convert3",
        batch_idx + 1,
        batches,
        format!("batch {}", batch_idx + 1).as_str(),
        id,
      );
      let mut doc = Document::with_version("1.5");
      let mut page_ids: Vec<ObjectId> = Vec::new();
      let results = chunk
        .par_iter()
        .enumerate()
        .map(|(idx, p)| {
          let w = warnings.clone();
          prepare_pages(p, idx, &app, id, bs, &w)
        })
        .collect::<Vec<_>>();
      for res in results {
        match res {
          Ok(list) => {
            for page_buf in list {
              insert_page(&mut doc, page_buf, &mut page_ids)
            }
          }
          Err(issue) => problems.push(issue),
        }
      }
      build_catalog(&mut doc, &page_ids)?;
      let pdf_name = format!("{}_batch{:03}.pdf", input_name, batch_idx + 1);
      let pdf_path = output_dir.join(pdf_name);
      let mut bufw = BufWriter::new(File::create(&pdf_path).map_err(|e| e.to_string())?);
      doc.save_to(&mut bufw).map_err(|e| e.to_string())?;
      output_bytes += std::fs::metadata(&pdf_path).map(|m| m.len()).unwrap_or(0);
    }
  }

  let mut log_full = format!(
    "Input: {:?}\nOutput: {:?}\nTotal files: {}\nProblems: {}\nInput bytes: {}\nOutput bytes: {}\n",
    input,
    if is_single { output_path.clone() } else { output_dir.clone() },
    paths.len(),
    problems.len(),
    original_bytes,
    output_bytes
  );
  if let Ok(ws) = warnings.lock() {
    if !ws.is_empty() {
      log_full.push_str("Warnings:\n");
      for w in ws.iter() {
        log_full.push_str("- ");
        log_full.push_str(w);
        log_full.push('\n');
      }
    }
  }
  let _ = std::fs::write(&log_path, log_full);

  Ok(Convert3Result {
    batches: if is_single { 1 } else { (paths.len() + batch_size.max(1) - 1) / batch_size.max(1) },
    output_path: if is_single { output_path } else { output_dir },
    original_bytes,
    output_bytes,
    problems,
    log_path,
  })
}

fn prepare_pages(
  path: &Path,
  idx: usize,
  app: &AppHandle,
  id: Uuid,
  total: usize,
  warnings: &Arc<Mutex<Vec<String>>>,
) -> Result<Vec<PageBuf>, ImageIssue> {
  emit_progress(app, "convert3", idx + 1, total, path.to_string_lossy().as_ref(), id);

  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .map(|s| s.to_ascii_lowercase())
    .unwrap_or_default();

  let reader = ImageReader::open(path)
    .and_then(|r| r.with_guessed_format())
    .map_err(|e| ImageIssue {
      path: path.to_path_buf(),
      reasons: vec![e.to_string()],
    })?;
  let format = reader.format();
  let (w, h) = image_dimensions(path).map_err(|e| ImageIssue {
    path: path.to_path_buf(),
    reasons: vec![e.to_string()],
  })?;

  // WebP 和 PNG 都采用 JPEG 压缩优化，避免文件体积暴涨
  if ext == "webp" {
    return prepare_webp_optimized(path, warnings);
  }

  if ext == "png" {
    return prepare_png_optimized(path, warnings);
  }

  if let Some(f) = format {
    let fmt = format_to_ext(f);
    if !fmt.is_empty() && !ext.is_empty() && fmt != ext {
      note_warning(warnings, path, &format!("扩展名与实际格式不符: 扩展名 {} / 实际 {}", ext, fmt));
    }
  }

  if (w as u64) * (h as u64) > 100_000_000 {
    note_warning(warnings, path, "超大图像 (>100MP)，处理可能耗时");
  }

  let is_jpeg = matches!(format, Some(ImageFormat::Jpeg));
  let jpeg_rgb = is_jpeg && jpeg_is_rgb(path);

  if is_jpeg && !jpeg_rgb && jpeg_is_cmyk(path) {
    let rgb_data = convert_cmyk_to_rgb(path)?;
    return Ok(vec![PageBuf {
      w,
      h,
      data: rgb_data,
      filter: Some("DCTDecode"),
      color_space: "DeviceRGB",
      bits: 8,
      smask: None,
      rotated_90_or_270: false,
    }]);
  }

  // HEIC/HEIF 拒绝并提示
  if is_heic(path) {
    return Err(ImageIssue {
      path: path.to_path_buf(),
      reasons: vec!["HEIC/HEIF 格式暂不支持，请转换为 JPG/PNG".into()],
    });
  }

  // 多页 TIFF 处理
  if matches!(format, Some(ImageFormat::Tiff)) {
    match handle_multipage_tiff(path, warnings) {
      Ok(list) => return Ok(list),
      Err(e) => return Err(e),
    }
  }

  let (data, filter, color_space, bits, smask, rotated_90_or_270) = if jpeg_rgb {
    if !jpeg_has_eoi(path) {
      note_warning(warnings, path, "JPEG 文件可能不完整（缺少 FFD9）");
    }
    (std::fs::read(path).map_err(|e| ImageIssue {
      path: path.to_path_buf(),
      reasons: vec![e.to_string()],
    })?, Some("DCTDecode"), "DeviceRGB", 8, None, false)
  } else {
    let dyn_img = ImageReader::open(path)
      .and_then(|r| r.with_guessed_format())
      .map_err(|e| ImageIssue {
        path: path.to_path_buf(),
        reasons: vec![e.to_string()],
      })?
      .decode()
      .map_err(|e| ImageIssue {
        path: path.to_path_buf(),
        reasons: vec![e.to_string()],
      })?;

    let (dyn_img, rotated_90_or_270) = apply_exif_rotation(dyn_img, path, warnings);

    match dyn_img.color() {
      ColorType::L8 | ColorType::La8 | ColorType::L16 | ColorType::La16 => {
        if dyn_img.color().has_alpha() {
          note_warning(warnings, path, "灰度带透明通道，已按白底合成");
        }
        if matches!(dyn_img.color(), ColorType::L16 | ColorType::La16) {
          note_warning(warnings, path, "16位灰度，已转换为 8 位");
        }
        let (gray, mask) = if dyn_img.color().has_alpha() {
          let la = dyn_img.to_luma_alpha8();
          let alpha: Vec<u8> = la.pixels().map(|p| p[1]).collect();
          (flatten_luma_alpha_to_luma(&la), Some(alpha))
        } else {
          (dyn_img.to_luma8(), None)
        };
        let data = gray.into_raw();
        (data, None, "DeviceGray", 8, mask, rotated_90_or_270)
      }
      _ => {
        if dyn_img.color().has_alpha() {
          note_warning(warnings, path, "含透明通道，已按白底合成移除 alpha");
        }
        match dyn_img.color() {
          ColorType::Rgb16 | ColorType::Rgba16 => {
            note_warning(warnings, path, "16位高位深，已转换为 8 位");
          }
          ColorType::Rgb8 | ColorType::Rgba8 => {}
          other => {
            note_warning(warnings, path, &format!("非 RGB 颜色空间 {:?}，已转换为 RGB", other));
          }
        }
        let (rgb_data, smask) = if dyn_img.color().has_alpha() {
          let rgba = dyn_img.to_rgba8();
          (flatten_rgba_to_rgb(&rgba).into_raw(), Some(extract_alpha(&rgba)))
        } else {
          (dyn_img.to_rgb8().into_raw(), None)
        };
        (rgb_data, None, "DeviceRGB", 8, smask, rotated_90_or_270)
      }
    }
  };

  Ok(vec![PageBuf {
    w,
    h,
    data,
    filter,
    color_space,
    bits,
    smask,
    rotated_90_or_270,
  }])
}

fn prepare_png_optimized(path: &Path, _warnings: &Arc<Mutex<Vec<String>>>) -> Result<Vec<PageBuf>, ImageIssue> {
  let img = ImageReader::open(path)
    .and_then(|r| r.with_guessed_format())
    .map_err(|e| ImageIssue {
      path: path.to_path_buf(),
      reasons: vec![e.to_string()],
    })?
    .decode()
    .map_err(|e| ImageIssue {
      path: path.to_path_buf(),
      reasons: vec![e.to_string()],
    })?;

  let (w, h) = img.dimensions();
  let color_type = img.color();
  
  // 最终我们要得到的 JPEG 数据
  let jpeg_data = if color_type.has_alpha() {
    // 方案：如果有透明度，手动混合到白底上
    let rgba = img.to_rgba8();
    let mut rgb_image = image::ImageBuffer::new(w, h);

    for (x, y, pixel) in rgba.enumerate_pixels() {
      let alpha = pixel[3] as f32 / 255.0;
      let inv_alpha = 1.0 - alpha;

      // 混合公式: 颜色 = 原色 * Alpha + 白色(255) * (1 - Alpha)
      let r = (pixel[0] as f32 * alpha + 255.0 * inv_alpha) as u8;
      let g = (pixel[1] as f32 * alpha + 255.0 * inv_alpha) as u8;
      let b = (pixel[2] as f32 * alpha + 255.0 * inv_alpha) as u8;

      rgb_image.put_pixel(x, y, image::Rgb([r, g, b]));
    }

    // 编码为 JPEG (质量 80，体积小，无透明)
    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, 95)//png转换质量
      .encode_image(&rgb_image)
      .map_err(|e| ImageIssue {
        path: path.to_path_buf(),
        reasons: vec![format!("JPEG encoding failed: {}", e)],
      })?;
    buf
  } else {
    // 如果原图就没有透明度，直接转 JPEG
    let rgb = img.to_rgb8();
    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, 80)
      .encode_image(&rgb)
      .map_err(|e| ImageIssue {
        path: path.to_path_buf(),
        reasons: vec![format!("JPEG encoding failed: {}", e)],
      })?;
    buf
  };

  // 返回结果：注意 filter 是 DCTDecode (JPEG)，且 smask 是 None (无透明蒙版)
  Ok(vec![PageBuf {
    w,
    h,
    data: jpeg_data,
    filter: Some("DCTDecode"), // 告诉 PDF 这是 JPEG
    color_space: "DeviceRGB",
    bits: 8,
    smask: None, // 彻底移除透明蒙版，体积最小
    rotated_90_or_270: false,
  }])
}

/// WebP 格式优化处理：将 WebP 转换为 JPEG 压缩格式，避免 PDF 文件体积暴涨
fn prepare_webp_optimized(path: &Path, warnings: &Arc<Mutex<Vec<String>>>) -> Result<Vec<PageBuf>, ImageIssue> {
  let img = ImageReader::open(path)
    .and_then(|r| r.with_guessed_format())
    .map_err(|e| ImageIssue {
      path: path.to_path_buf(),
      reasons: vec![format!("无法打开 WebP 文件: {}", e)],
    })?
    .decode()
    .map_err(|e| ImageIssue {
      path: path.to_path_buf(),
      reasons: vec![format!("WebP 解码失败: {}", e)],
    })?;

  let (w, h) = img.dimensions();
  let color_type = img.color();

  // 记录警告信息
  if color_type.has_alpha() {
    note_warning(warnings, path, "WebP 含透明通道，已按白底合成");
  }

  // 将 WebP 转换为 JPEG 压缩格式
  let jpeg_data = if color_type.has_alpha() {
    // 有透明度时，混合到白底上
    let rgba = img.to_rgba8();
    let mut rgb_image = ImageBuffer::new(w, h);

    for (x, y, pixel) in rgba.enumerate_pixels() {
      let alpha = pixel[3] as f32 / 255.0;
      let inv_alpha = 1.0 - alpha;

      let r = (pixel[0] as f32 * alpha + 255.0 * inv_alpha) as u8;
      let g = (pixel[1] as f32 * alpha + 255.0 * inv_alpha) as u8;
      let b = (pixel[2] as f32 * alpha + 255.0 * inv_alpha) as u8;

      rgb_image.put_pixel(x, y, image::Rgb([r, g, b]));
    }

    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, 92) // WebP 使用较高质量保持图像细节
      .encode_image(&rgb_image)
      .map_err(|e| ImageIssue {
        path: path.to_path_buf(),
        reasons: vec![format!("JPEG 编码失败: {}", e)],
      })?;
    buf
  } else {
    // 无透明度，直接转 JPEG
    let rgb = img.to_rgb8();
    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, 92)
      .encode_image(&rgb)
      .map_err(|e| ImageIssue {
        path: path.to_path_buf(),
        reasons: vec![format!("JPEG 编码失败: {}", e)],
      })?;
    buf
  };

  Ok(vec![PageBuf {
    w,
    h,
    data: jpeg_data,
    filter: Some("DCTDecode"), // JPEG 压缩
    color_space: "DeviceRGB",
    bits: 8,
    smask: None,
    rotated_90_or_270: false,
  }])
}

fn insert_page(doc: &mut Document, page_buf: PageBuf, page_ids: &mut Vec<ObjectId>) {
  let PageBuf {
    w,
    h,
    data,
    filter,
    color_space,
    bits,
    smask,
    rotated_90_or_270,
  } = page_buf;

  let (final_w, final_h) = if rotated_90_or_270 { (h, w) } else { (w, h) };

  let img_id = doc.new_object_id();
  let mut img_dict = dictionary! {
    "Type" => "XObject",
    "Subtype" => "Image",
    "Width" => final_w as i64,
    "Height" => final_h as i64,
    "ColorSpace" => color_space,
    "BitsPerComponent" => bits as i64,
    "Length" => data.len() as i64
  };
  if let Some(f) = filter {
    img_dict.set("Filter", f);
  }

  let mut stream = Stream::new(img_dict, data);
  if filter.is_none() {
    stream = stream.with_compression(true);
  }
  if let Some(mask) = smask {
    let smask_id = doc.new_object_id();
    let smask_dict = dictionary! {
      "Type" => "XObject",
      "Subtype" => "Image",
      "Width" => final_w as i64,
      "Height" => final_h as i64,
      "ColorSpace" => "DeviceGray",
      "BitsPerComponent" => 8,
      "Length" => mask.len() as i64
    };
    let smask_stream = Stream::new(smask_dict, mask).with_compression(true);
    doc.objects.insert(smask_id, Object::Stream(smask_stream));
    stream.dict.set("SMask", smask_id);
  }

  doc.objects.insert(img_id, Object::Stream(stream));

  let content_stream = format!("q {} 0 0 {} 0 0 cm /Im0 Do Q", final_w, final_h);
  let content_id = doc.add_object(Stream::new(dictionary! {}, content_stream.into_bytes()));

  let mut xobj = lopdf::Dictionary::new();
  xobj.set("Im0", img_id);
  let resources = dictionary! {
    "XObject" => xobj
  };

  let page_id = doc.new_object_id();
  let page_dict = dictionary! {
    "Type" => "Page",
    "Parent" => (0,0),
    "Resources" => resources,
    "MediaBox" => vec![0.into(), 0.into(), (final_w as f64).into(), (final_h as f64).into()],
    "Contents" => content_id,
  };
  doc.objects.insert(page_id, Object::Dictionary(page_dict));
  page_ids.push(page_id);
}

fn build_catalog(doc: &mut Document, page_ids: &[ObjectId]) -> Result<(), String> {
  if page_ids.is_empty() {
    return Ok(());
  }
  let pages_id = doc.new_object_id();
  let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
  let pages_dict = dictionary! {
    "Type" => "Pages",
    "Kids" => kids,
    "Count" => page_ids.len() as i64
  };
  doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

  for pid in page_ids {
    if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(pid) {
      dict.set("Parent", pages_id);
    }
  }

  let catalog_id = doc.new_object_id();
  let catalog = dictionary! {
    "Type" => "Catalog",
    "Pages" => pages_id
  };
  doc.objects.insert(catalog_id, Object::Dictionary(catalog));
  doc.trailer.set("Root", catalog_id);
  Ok(())
}

fn jpeg_is_rgb(path: &Path) -> bool {
  if let Ok(file) = File::open(path) {
    if let Ok(decoder) = JpegDecoder::new(BufReader::new(file)) {
      return matches!(decoder.color_type(), ColorType::Rgb8);
    }
  }
  false
}

fn jpeg_is_cmyk(path: &Path) -> bool {
  if let Ok(file) = File::open(path) {
    let mut decoder = RawJpegDecoder::new(BufReader::new(file));
    if decoder.decode().is_ok() {
      if let Some(info) = decoder.info() {
        return matches!(info.pixel_format, PixelFormat::CMYK32);
      }
    }
  }
  false
}

fn convert_cmyk_to_rgb(path: &Path) -> Result<Vec<u8>, ImageIssue> {
  let file = File::open(path).map_err(|e| ImageIssue {
    path: path.to_path_buf(),
    reasons: vec![e.to_string()],
  })?;
  let mut decoder = RawJpegDecoder::new(BufReader::new(file));
  let pixels = decoder.decode().map_err(|e| ImageIssue {
    path: path.to_path_buf(),
    reasons: vec![e.to_string()],
  })?;
  let info = decoder.info().ok_or_else(|| ImageIssue {
    path: path.to_path_buf(),
    reasons: vec!["JPEG metadata missing".into()],
  })?;

  if !matches!(info.pixel_format, PixelFormat::CMYK32) {
    return Err(ImageIssue {
      path: path.to_path_buf(),
      reasons: vec!["JPEG is not CMYK".into()],
    });
  }

  let (width, height) = (info.width, info.height);
  let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
  for chunk in pixels.chunks_exact(4) {
    let c = chunk[0] as u32;
    let m = chunk[1] as u32;
    let y = chunk[2] as u32;
    let k = chunk[3] as u32;
    let r = ((255 - c) * (255 - k) / 255) as u8;
    let g = ((255 - m) * (255 - k) / 255) as u8;
    let b = ((255 - y) * (255 - k) / 255) as u8;
    rgb.push(r);
    rgb.push(g);
    rgb.push(b);
  }

  let mut out = Vec::new();
  JpegEncoder::new_with_quality(&mut out, 95)
    .encode(&rgb, width.into(), height.into(), ExtendedColorType::Rgb8)
    .map_err(|e| ImageIssue {
      path: path.to_path_buf(),
      reasons: vec![e.to_string()],
    })?;

  Ok(out)
}

fn flatten_luma_alpha_to_luma(img: &ImageBuffer<LumaA<u8>, Vec<u8>>) -> ImageBuffer<Luma<u8>, Vec<u8>> {
  let (w, h) = img.dimensions();
  let mut gray = ImageBuffer::new(w, h);
  for (x, y, pixel) in img.enumerate_pixels() {
    let LumaA([l, a]) = *pixel;
    let alpha = a as u32;
    let out_l = ((l as u32 * alpha + 255 * (255 - alpha)) / 255) as u8;
    gray.put_pixel(x, y, Luma([out_l]));
  }
  gray
}

fn flatten_rgba_to_rgb(img: &RgbaImage) -> ImageBuffer<image::Rgb<u8>, Vec<u8>> {
  let (w, h) = img.dimensions();
  let mut rgb = ImageBuffer::new(w, h);
  for (x, y, pixel) in img.enumerate_pixels() {
    let Rgba([r, g, b, a]) = *pixel;
    let alpha = a as u32;
    let out_r = ((r as u32 * alpha + 255 * (255 - alpha)) / 255) as u8;
    let out_g = ((g as u32 * alpha + 255 * (255 - alpha)) / 255) as u8;
    let out_b = ((b as u32 * alpha + 255 * (255 - alpha)) / 255) as u8;
    rgb.put_pixel(x, y, image::Rgb([out_r, out_g, out_b]));
  }
  rgb
}

fn extract_alpha(img: &RgbaImage) -> Vec<u8> {
  img.pixels().map(|p| p[3]).collect()
}

fn note_warning(warnings: &Arc<Mutex<Vec<String>>>, path: &Path, msg: &str) {
  if let Ok(mut w) = warnings.lock() {
    w.push(format!("{} -> {}", path.to_string_lossy(), msg));
  }
}

fn format_to_ext(fmt: ImageFormat) -> String {
  match fmt {
    ImageFormat::Jpeg => "jpeg",
    ImageFormat::Png => "png",
    ImageFormat::Gif => "gif",
    ImageFormat::Bmp => "bmp",
    ImageFormat::Tiff => "tiff",
    ImageFormat::WebP => "webp",
    _ => "",
  }
  .to_string()
}

fn jpeg_has_eoi(path: &Path) -> bool {
  if let Ok(bytes) = std::fs::read(path) {
    return bytes.len() > 2 && bytes[bytes.len() - 2..] == [0xFF, 0xD9];
  }
  false
}

fn is_heic(path: &Path) -> bool {
  if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
    let ext = ext.to_ascii_lowercase();
    return ext == "heic" || ext == "heif";
  }
  false
}

fn apply_exif_rotation(img: image::DynamicImage, path: &Path, warnings: &Arc<Mutex<Vec<String>>>) -> (image::DynamicImage, bool) {
  if let Ok(file) = std::fs::File::open(path) {
    let mut reader = BufReader::new(file);
    if let Ok(exif) = Reader::new().read_from_container(&mut reader) {
      if let Some(orientation) = exif.get_field(Tag::Orientation, In::PRIMARY).and_then(|f| f.value.get_uint(0)) {
        let base = img.to_rgba8();
        let swapped = matches!(orientation, 5 | 6 | 7 | 8);
        let rotated_buf = match orientation {
          2 => imageops::flip_horizontal(&base),
          3 => imageops::rotate180(&base),
          4 => imageops::flip_vertical(&base),
          5 => imageops::flip_horizontal(&imageops::rotate90(&base)),
          6 => imageops::rotate90(&base),
          7 => imageops::flip_horizontal(&imageops::rotate270(&base)),
          8 => imageops::rotate270(&base),
          _ => base,
        };
        if orientation != 1 {
          note_warning(warnings, path, &format!("应用 EXIF 旋转: {}", orientation));
        }
        return (image::DynamicImage::ImageRgba8(rotated_buf), swapped);
      }
    }
  }
  (img, false)
}

fn handle_multipage_tiff(path: &Path, warnings: &Arc<Mutex<Vec<String>>>) -> Result<Vec<PageBuf>, ImageIssue> {
  let file = File::open(path).map_err(|e| ImageIssue {
    path: path.to_path_buf(),
    reasons: vec![e.to_string()],
  })?;
  let mut decoder = Decoder::new(file).map_err(|e| ImageIssue {
    path: path.to_path_buf(),
    reasons: vec![e.to_string()],
  })?;

  let mut pages = Vec::new();
  loop {
    let color = decoder.colortype().unwrap_or(tiff::ColorType::RGB(8));
    let (w, h) = decoder.dimensions().unwrap_or((0, 0));
    match decoder.read_image() {
      Ok(DecodingResult::U8(data)) => {
        let (rgb, smask, cs) = convert_tiff_data_u8(&data, color, w, h, warnings, path)?;
        pages.push(PageBuf {
          w,
          h,
          data: rgb,
          filter: None,
          color_space: cs,
          bits: 8,
          smask,
          rotated_90_or_270: false,
        });
      }
      Ok(DecodingResult::U16(data)) => {
        let data_u8: Vec<u8> = data.iter().map(|v| (v >> 8) as u8).collect();
        let (rgb, smask, cs) = convert_tiff_data_u8(&data_u8, color, w, h, warnings, path)?;
        pages.push(PageBuf {
          w,
          h,
          data: rgb,
          filter: None,
          color_space: cs,
          bits: 8,
          smask,
          rotated_90_or_270: false,
        });
      }
      Ok(_) => {
        return Err(ImageIssue {
          path: path.to_path_buf(),
          reasons: vec!["不支持的 TIFF 数据格式".into()],
        })
      }
      Err(tiff::TiffError::IoError(_)) => break, // EOF
      Err(e) => {
        return Err(ImageIssue {
          path: path.to_path_buf(),
          reasons: vec![e.to_string()],
        })
      }
    }

    if let Err(_) = decoder.next_image() {
      break;
    }
  }

  if pages.is_empty() {
    return Err(ImageIssue {
      path: path.to_path_buf(),
      reasons: vec!["未能读取 TIFF 内容".into()],
    });
  }
  note_warning(warnings, path, &format!("多页 TIFF，页数 {}", pages.len()));
  Ok(pages)
}

fn convert_tiff_data_u8(
  data: &[u8],
  color: tiff::ColorType,
  w: u32,
  h: u32,
  warnings: &Arc<Mutex<Vec<String>>>,
  path: &Path,
) -> Result<(Vec<u8>, Option<Vec<u8>>, &'static str), ImageIssue> {
  let pixels = (w as usize)
    .checked_mul(h as usize)
    .ok_or_else(|| ImageIssue {
      path: path.to_path_buf(),
      reasons: vec!["TIFF 尺寸异常".into()],
    })?;

  match color {
    tiff::ColorType::Gray(8) => Ok((data.to_vec(), None, "DeviceGray")),
    tiff::ColorType::Gray(16) => {
      let buf: Vec<u8> = data.iter().step_by(2).cloned().collect();
      Ok((buf, None, "DeviceGray"))
    }
    tiff::ColorType::RGB(8) => Ok((data.to_vec(), None, "DeviceRGB")),
    tiff::ColorType::RGBA(8) => {
      let mut rgb = Vec::with_capacity(pixels * 3);
      let mut alpha = Vec::with_capacity(pixels);
      for chunk in data.chunks_exact(4) {
        rgb.extend_from_slice(&chunk[0..3]);
        alpha.push(chunk[3]);
      }
      Ok((rgb, Some(alpha), "DeviceRGB"))
    }
    _ => {
      note_warning(warnings, path, "TIFF 颜色类型非 RGB/Gray，已尝试转换");
      // Fallback: assume 3 channels
      let mut rgb = Vec::with_capacity(pixels * 3);
      for chunk in data.chunks(3) {
        if chunk.len() == 3 {
          rgb.extend_from_slice(chunk);
        }
      }
      Ok((rgb, None, "DeviceRGB"))
    }
  }
}

// ==================== EPUB 转换功能 ====================

/// EPUB 页面尺寸预设
#[derive(Debug, Clone, Copy)]
pub struct EpubPageSize {
  pub width: u32,
  pub height: u32,
  pub name: &'static str,
}

impl EpubPageSize {
  /// iPad Pro 12.9" (2452:1668 比例)
  pub const IPAD_PRO: EpubPageSize = EpubPageSize {
    width: 2452,
    height: 1668,
    name: "iPad Pro",
  };

  /// iPad Mini (2266:1488 比例)
  pub const IPAD_MINI: EpubPageSize = EpubPageSize {
    width: 2266,
    height: 1488,
    name: "iPad Mini",
  };

  /// A4 尺寸 (210mm x 297mm @ 150 DPI)
  pub const A4: EpubPageSize = EpubPageSize {
    width: 1240,
    height: 1754,
    name: "A4",
  };

  /// B5 尺寸 (176mm x 250mm @ 150 DPI)
  pub const B5: EpubPageSize = EpubPageSize {
    width: 1039,
    height: 1476,
    name: "B5",
  };

  pub fn from_name(name: &str) -> Option<EpubPageSize> {
    match name.to_lowercase().as_str() {
      "ipadpro" | "ipad_pro" | "ipad pro" => Some(Self::IPAD_PRO),
      "ipadmini" | "ipad_mini" | "ipad mini" => Some(Self::IPAD_MINI),
      "a4" => Some(Self::A4),
      "b5" => Some(Self::B5),
      _ => None,
    }
  }
}

/// 将图片转换为 EPUB 格式
pub fn convert_to_epub(app: AppHandle, req: EpubConvertRequest) -> Result<EpubConvertResult, String> {
  let id = Uuid::new_v4();
  let input = req.input_dir.clone();

  if !input.exists() {
    return Err(format!("输入目录不存在: {}", input.to_string_lossy()));
  }
  if !input.is_dir() {
    return Err(format!("输入路径不是目录: {}", input.to_string_lossy()));
  }

  let input_name = input.file_name().and_then(|s| s.to_str()).unwrap_or("output");
  let base_dir = input.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();

  // 解析页面尺寸
  let page_size = req.page_size
    .as_ref()
    .and_then(|s| EpubPageSize::from_name(s))
    .unwrap_or(EpubPageSize::IPAD_PRO);

  // 输出路径
  let output_path = req.output_dir
    .clone()
    .unwrap_or_else(|| base_dir.join(format!("{}.epub", input_name)));

  let output_path = if output_path.extension().map(|e| e.to_str().unwrap_or("")).unwrap_or("") != "epub" {
    output_path.with_extension("epub")
  } else {
    output_path
  };

  // 列出图片
  let files = list_images(input.clone(), true)?;
  if files.is_empty() {
    return Err("未找到图像文件".into());
  }

  let original_bytes: u64 = files.iter().map(|f| f.size).sum();
  let total = files.len();

  // 创建 EPUB 文件
  let file = File::create(&output_path).map_err(|e| format!("创建 EPUB 文件失败: {}", e))?;
  let mut zip = ZipWriter::new(file);
  let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
  let options_deflate = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

  // 写入 mimetype (必须是第一个文件，且不压缩)
  zip.start_file("mimetype", options).map_err(|e| e.to_string())?;
  zip.write_all(b"application/epub+zip").map_err(|e| e.to_string())?;

  // 创建 META-INF/container.xml
  zip.start_file("META-INF/container.xml", options_deflate).map_err(|e| e.to_string())?;
  zip.write_all(EPUB_CONTAINER_XML.as_bytes()).map_err(|e| e.to_string())?;

  // 并行处理图片（CPU 密集型操作）
  let processed_results: Vec<_> = files
    .par_iter()
    .enumerate()
    .map(|(idx, file_entry)| {
      emit_progress(&app, "epub_convert", idx + 1, total, file_entry.name.as_str(), id);
      
      let image_id = format!("img{:04}", idx + 1);
      let xhtml_id = format!("page{:04}", idx + 1);
      let image_filename = format!("images/{}.jpg", image_id);
      let xhtml_filename = format!("text/{}.xhtml", xhtml_id);

      match process_image_for_epub(&file_entry.path, page_size) {
        Ok(jpeg_data) => {
          let xhtml_content = generate_epub_page_xhtml(&image_filename, page_size);
          Ok((idx, image_id, xhtml_id, image_filename, xhtml_filename, jpeg_data, xhtml_content))
        }
        Err(e) => Err(e),
      }
    })
    .collect();

  // 按顺序写入 ZIP（保持页面顺序）
  let mut image_items = Vec::new();
  let mut spine_items = Vec::new();
  let mut problems = Vec::new();
  let mut successful_results: Vec<_> = processed_results
    .into_iter()
    .filter_map(|r| match r {
      Ok(data) => Some(data),
      Err(e) => {
        problems.push(e);
        None
      }
    })
    .collect();
  
  // 按索引排序确保顺序正确
  successful_results.sort_by_key(|r| r.0);

  for (_idx, image_id, xhtml_id, image_filename, xhtml_filename, jpeg_data, xhtml_content) in successful_results {
    // 写入图片
    zip.start_file(&format!("OEBPS/{}", image_filename), options_deflate).map_err(|e| e.to_string())?;
    zip.write_all(&jpeg_data).map_err(|e| e.to_string())?;

    // 写入 XHTML 页面
    zip.start_file(&format!("OEBPS/{}", xhtml_filename), options_deflate).map_err(|e| e.to_string())?;
    zip.write_all(xhtml_content.as_bytes()).map_err(|e| e.to_string())?;

    image_items.push((image_id, image_filename));
    spine_items.push((xhtml_id, xhtml_filename));
  }

  // 生成并写入 content.opf
  let content_opf = generate_content_opf(input_name, &image_items, &spine_items);
  zip.start_file("OEBPS/content.opf", options_deflate).map_err(|e| e.to_string())?;
  zip.write_all(content_opf.as_bytes()).map_err(|e| e.to_string())?;

  // 生成并写入 toc.ncx
  let toc_ncx = generate_toc_ncx(input_name, &spine_items);
  zip.start_file("OEBPS/toc.ncx", options_deflate).map_err(|e| e.to_string())?;
  zip.write_all(toc_ncx.as_bytes()).map_err(|e| e.to_string())?;

  // 生成并写入 nav.xhtml
  let nav_xhtml = generate_nav_xhtml(&spine_items);
  zip.start_file("OEBPS/nav.xhtml", options_deflate).map_err(|e| e.to_string())?;
  zip.write_all(nav_xhtml.as_bytes()).map_err(|e| e.to_string())?;

  // 写入 CSS
  zip.start_file("OEBPS/styles/style.css", options_deflate).map_err(|e| e.to_string())?;
  zip.write_all(EPUB_STYLE_CSS.as_bytes()).map_err(|e| e.to_string())?;

  zip.finish().map_err(|e| e.to_string())?;

  let output_bytes = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);

  Ok(EpubConvertResult {
    output_path,
    original_bytes,
    output_bytes,
    page_count: spine_items.len(),
    page_size: page_size.name.to_string(),
    problems,
  })
}

/// 处理单张图片：调整尺寸并转换为 JPEG
fn process_image_for_epub(path: &Path, page_size: EpubPageSize) -> Result<Vec<u8>, ImageIssue> {
  let img = ImageReader::open(path)
    .and_then(|r| r.with_guessed_format())
    .map_err(|e| ImageIssue {
      path: path.to_path_buf(),
      reasons: vec![format!("无法打开图片: {}", e)],
    })?
    .decode()
    .map_err(|e| ImageIssue {
      path: path.to_path_buf(),
      reasons: vec![format!("图片解码失败: {}", e)],
    })?;

  let (orig_w, orig_h) = img.dimensions();

  // 判断图片方向：竖屏图片(高>宽)用竖屏目标尺寸，横屏图片用横屏目标尺寸
  let is_portrait = orig_h > orig_w;
  let (target_w, target_h) = if is_portrait {
    // 竖屏图片：目标尺寸保持竖屏 (窄高)
    (page_size.height.min(page_size.width), page_size.height.max(page_size.width))
  } else {
    // 横屏图片：目标尺寸保持横屏 (宽矮)
    (page_size.height.max(page_size.width), page_size.height.min(page_size.width))
  };

  // 计算缩放比例：取短边比例，让图片填满目标区域
  // 例如：图片 1280x1807，目标 1668x2420
  // scale_w = 1668/1280 ≈ 1.303
  // scale_h = 2420/1807 ≈ 1.339
  // 取较小值 1.303，等比例放大 1.303 倍
  let scale_w = target_w as f32 / orig_w as f32;
  let scale_h = target_h as f32 / orig_h as f32;
  let scale = scale_w.min(scale_h); // 允许放大，取较小比例确保图片完全在目标区域内

  let new_w = ((orig_w as f32 * scale).round() as u32).max(1);
  let new_h = ((orig_h as f32 * scale).round() as u32).max(1);

  // 调整尺寸 - 使用 Triangle 算法更快，质量仍可接受
  let resized = if (scale - 1.0).abs() > 0.01 {
    img.resize(new_w, new_h, image::imageops::FilterType::Triangle)
  } else {
    img
  };

  // 处理透明度
  let rgb_img = if resized.color().has_alpha() {
    let rgba = resized.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut rgb = ImageBuffer::new(w, h);
    for (x, y, pixel) in rgba.enumerate_pixels() {
      let alpha = pixel[3] as f32 / 255.0;
      let inv_alpha = 1.0 - alpha;
      let r = (pixel[0] as f32 * alpha + 255.0 * inv_alpha) as u8;
      let g = (pixel[1] as f32 * alpha + 255.0 * inv_alpha) as u8;
      let b = (pixel[2] as f32 * alpha + 255.0 * inv_alpha) as u8;
      rgb.put_pixel(x, y, image::Rgb([r, g, b]));
    }
    rgb
  } else {
    resized.to_rgb8()
  };

  // 编码为 JPEG
  let mut buf = Vec::new();
  JpegEncoder::new_with_quality(&mut buf, 85)
    .encode_image(&rgb_img)
    .map_err(|e| ImageIssue {
      path: path.to_path_buf(),
      reasons: vec![format!("JPEG 编码失败: {}", e)],
    })?;

  Ok(buf)
}

/// 生成 EPUB 页面 XHTML
fn generate_epub_page_xhtml(image_path: &str, page_size: EpubPageSize) -> String {
  format!(
    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
  <meta charset="UTF-8"/>
  <meta name="viewport" content="width={}, height={}"/>
  <link rel="stylesheet" type="text/css" href="../styles/style.css"/>
  <title>Page</title>
</head>
<body>
  <div class="page">
    <img src="../{}" alt="page"/>
  </div>
</body>
</html>"#,
    page_size.width, page_size.height, image_path
  )
}

/// 生成 content.opf
fn generate_content_opf(title: &str, images: &[(String, String)], pages: &[(String, String)]) -> String {
  let mut manifest_items = String::new();
  let mut spine_items = String::new();

  // 添加图片到 manifest
  for (id, href) in images {
    manifest_items.push_str(&format!(
      r#"    <item id="{}" href="{}" media-type="image/jpeg"/>"#,
      id, href
    ));
    manifest_items.push('\n');
  }

  // 添加页面到 manifest 和 spine
  for (id, href) in pages {
    manifest_items.push_str(&format!(
      r#"    <item id="{}" href="{}" media-type="application/xhtml+xml"/>"#,
      id, href
    ));
    manifest_items.push('\n');
    spine_items.push_str(&format!(r#"    <itemref idref="{}"/>"#, id));
    spine_items.push('\n');
  }

  format!(
    r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="BookId">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="BookId">urn:uuid:{}</dc:identifier>
    <dc:title>{}</dc:title>
    <dc:language>zh-CN</dc:language>
    <meta property="dcterms:modified">{}</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="css" href="styles/style.css" media-type="text/css"/>
{}  </manifest>
  <spine toc="ncx">
{}  </spine>
</package>"#,
    Uuid::new_v4(),
    title,
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
    manifest_items,
    spine_items
  )
}

/// 生成 toc.ncx
fn generate_toc_ncx(title: &str, pages: &[(String, String)]) -> String {
  let mut nav_points = String::new();

  for (idx, (_id, href)) in pages.iter().enumerate() {
    nav_points.push_str(&format!(
      r#"    <navPoint id="navPoint-{}" playOrder="{}">
      <navLabel><text>第 {} 页</text></navLabel>
      <content src="{}"/>
    </navPoint>
"#,
      idx + 1,
      idx + 1,
      idx + 1,
      href
    ));
  }

  format!(
    r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head>
    <meta name="dtb:uid" content="urn:uuid:{}"/>
    <meta name="dtb:depth" content="1"/>
    <meta name="dtb:totalPageCount" content="0"/>
    <meta name="dtb:maxPageNumber" content="0"/>
  </head>
  <docTitle><text>{}</text></docTitle>
  <navMap>
{}  </navMap>
</ncx>"#,
    Uuid::new_v4(),
    title,
    nav_points
  )
}

/// 生成 nav.xhtml
fn generate_nav_xhtml(pages: &[(String, String)]) -> String {
  let mut nav_items = String::new();

  for (idx, (_id, href)) in pages.iter().enumerate() {
    nav_items.push_str(&format!(
      r#"      <li><a href="{}">第 {} 页</a></li>
"#,
      href,
      idx + 1
    ));
  }

  format!(
    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
  <meta charset="UTF-8"/>
  <title>目录</title>
</head>
<body>
  <nav epub:type="toc" id="toc">
    <h1>目录</h1>
    <ol>
{}    </ol>
  </nav>
</body>
</html>"#,
    nav_items
  )
}

const EPUB_CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

const EPUB_STYLE_CSS: &str = r#"body {
  margin: 0;
  padding: 0;
}

.page {
  width: 100%;
  height: 100%;
  display: flex;
  justify-content: center;
  align-items: center;
}

.page img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}"#;
