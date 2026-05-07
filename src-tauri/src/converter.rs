use crate::models::{ConvertRequest, ConvertResult};
use crate::utils::{emit_progress, ensure_dir, timestamped_log};
use image::codecs::png::PngEncoder;
use image::{ImageEncoder, ImageFormat};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use uuid::Uuid;
use walkdir::WalkDir;

pub fn convert(app: AppHandle, req: ConvertRequest) -> Result<ConvertResult, String> {
    let id = Uuid::new_v4();
    let input = req.input_dir.clone();
    let input_name = input
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let base_dir = input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let default_single = base_dir.join(format!("{input_name}_合成.pdf"));

    let output_path = req.output_dir.clone().unwrap_or(default_single.clone());
    let batch_size = req.batch_size.unwrap_or(0);
    let is_single = batch_size == 0;
    let output_dir = if is_single {
        output_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or(base_dir.clone())
    } else {
        output_path.clone()
    };
    ensure_dir(&output_dir).map_err(|e| e.to_string())?;
    let log_path = timestamped_log(&output_dir, "conversion_log");

    let mut files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(&input).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }

    if files.is_empty() {
        return Err("未找到图像文件".into());
    }

    let mut problems = Vec::new();

    if is_single {
        let total = files.len().max(1);
        let mut doc = Document::with_version("1.5");
        let mut page_ids: Vec<ObjectId> = Vec::new();
        for (idx, path) in files.iter().enumerate() {
            emit_progress(
                &app,
                "convert",
                idx + 1,
                total,
                path.to_string_lossy().as_ref(),
                id,
            );
            match add_page_from_image(&mut doc, path, idx) {
                Ok(Some(pid)) => page_ids.push(pid),
                Ok(None) => problems.push(path.to_string_lossy().to_string()),
                Err(e) => problems.push(format!("{} => {}", path.to_string_lossy(), e)),
            }
        }
        build_catalog(&mut doc, &page_ids)?;
        let mut bufw = BufWriter::new(File::create(&output_path).map_err(|e| e.to_string())?);
        doc.save_to(&mut bufw).map_err(|e| e.to_string())?;
    } else {
        let total = files.len();
        let batch_size = batch_size.max(1);
        let batches = total.div_ceil(batch_size);
        for (batch_idx, chunk) in files.chunks(batch_size).enumerate() {
            emit_progress(
                &app,
                "convert",
                batch_idx + 1,
                batches,
                format!("batch {}", batch_idx + 1).as_str(),
                id,
            );
            let mut doc = Document::with_version("1.5");
            let mut page_ids: Vec<ObjectId> = Vec::new();
            for (idx, path) in chunk.iter().enumerate() {
                match add_page_from_image(&mut doc, path, idx) {
                    Ok(Some(pid)) => page_ids.push(pid),
                    Ok(None) => problems.push(path.to_string_lossy().to_string()),
                    Err(e) => problems.push(format!("{} => {}", path.to_string_lossy(), e)),
                }
            }
            build_catalog(&mut doc, &page_ids)?;
            let pdf_name = format!("{}_batch{:03}.pdf", input_name, batch_idx + 1);
            let pdf_path = output_dir.join(pdf_name);
            let mut bufw = BufWriter::new(File::create(pdf_path).map_err(|e| e.to_string())?);
            doc.save_to(&mut bufw).map_err(|e| e.to_string())?;
        }
    }

    let log_content = format!(
        "Input: {:?}\nOutput: {:?}\nTotal files: {}\n",
        input,
        if is_single {
            output_path.clone()
        } else {
            output_dir.clone()
        },
        files.len()
    );
    let _ = std::fs::write(&log_path, log_content);

    Ok(ConvertResult {
        batches: if is_single {
            1
        } else {
            files.len().div_ceil(batch_size.max(1))
        },
        output_path: if is_single { output_path } else { output_dir },
        log_path,
        problems: problems
            .into_iter()
            .map(|p| crate::models::ImageIssue {
                path: PathBuf::from(p.clone()),
                reasons: vec!["转换失败或跳过".into()],
            })
            .collect(),
    })
}

fn add_page_from_image(
    doc: &mut Document,
    path: &Path,
    img_idx: usize,
) -> Result<Option<ObjectId>, String> {
    let format = image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .map_err(|e| e.to_string())?
        .format();

    // read dimensions once
    let (w, h) = image::image_dimensions(path).map_err(|e| e.to_string())?;

    // preserve original jpeg bytes when possible
    let (buf, filter) = match format {
        Some(ImageFormat::Jpeg) => {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            (data, "DCTDecode")
        }
        _ => {
            // fallback: lossless PNG encode
            let dyn_img = image::open(path).map_err(|e| e.to_string())?;
            let rgb = dyn_img.to_rgb8();
            let mut data: Vec<u8> = Vec::new();
            let encoder = PngEncoder::new(&mut data);
            encoder
                .write_image(&rgb, w, h, image::ExtendedColorType::Rgb8)
                .map_err(|e| e.to_string())?;
            (data, "FlateDecode")
        }
    };

    let img_id = doc.new_object_id();
    let stream = Stream::new(
        dictionary! {
          "Type" => "XObject",
          "Subtype" => "Image",
          "Width" => w as i64,
          "Height" => h as i64,
          "ColorSpace" => "DeviceRGB",
          "BitsPerComponent" => 8,
          "Filter" => filter,
          "Length" => buf.len() as i64
        },
        buf,
    );
    doc.objects.insert(img_id, Object::Stream(stream));

    let content_stream = format!("q {} 0 0 {} 0 0 cm /Im{} Do Q", w, h, img_idx);
    let content_id = doc.add_object(Stream::new(dictionary! {}, content_stream.into_bytes()));

    let resources = dictionary! {
      "XObject" => {
        let mut d = lopdf::Dictionary::new();
        d.set(format!("Im{img_idx}"), img_id);
        d
      }
    };
    let page_id = doc.new_object_id();
    let page = dictionary! {
      "Type" => "Page",
      "Parent" => (0, 0),
      "Resources" => resources,
      "MediaBox" => vec![0.into(), 0.into(), (w as f64).into(), (h as f64).into()],
      "Contents" => content_id,
    };
    doc.objects.insert(page_id, Object::Dictionary(page));
    Ok(Some(page_id))
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
