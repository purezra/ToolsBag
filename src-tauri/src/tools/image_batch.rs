use crate::models::{EpubConvertRequest, EpubConvertResult, FileEntry, ImageIssue};
use crate::utils::emit_progress;
use image::codecs::jpeg::JpegEncoder;
use image::{GenericImageView, ImageBuffer, ImageReader};
use rayon::prelude::*;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use uuid::Uuid;
use walkdir::WalkDir;
use zip::write::FileOptions;
use zip::ZipWriter;

pub fn list_images(input_dir: PathBuf, recursive: bool) -> Result<Vec<FileEntry>, String> {
    let mut walker = WalkDir::new(&input_dir).follow_links(false);
    if !recursive {
        walker = walker.max_depth(1);
    }
    let mut files = Vec::new();
    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let path = entry.path().to_path_buf();
            if let Some(ext) = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase())
            {
                if ["jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp"].contains(&ext.as_str()) {
                    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
                    files.push(FileEntry {
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        path,
                        format: ext,
                        size: meta.len(),
                    });
                }
            }
        }
    }
    Ok(files)
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
pub fn convert_to_epub(
    app: AppHandle,
    req: EpubConvertRequest,
) -> Result<EpubConvertResult, String> {
    let id = Uuid::new_v4();
    let input = req.input_dir.clone();

    if !input.exists() {
        return Err(format!("输入目录不存在: {}", input.to_string_lossy()));
    }
    if !input.is_dir() {
        return Err(format!("输入路径不是目录: {}", input.to_string_lossy()));
    }

    let input_name = input
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let base_dir = input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    // 解析页面尺寸
    let page_size = req
        .page_size
        .as_ref()
        .and_then(|s| EpubPageSize::from_name(s))
        .unwrap_or(EpubPageSize::IPAD_PRO);

    // 输出路径
    let output_path = req
        .output_dir
        .clone()
        .unwrap_or_else(|| base_dir.join(format!("{}.epub", input_name)));

    let output_path = if output_path
        .extension()
        .map(|e| e.to_str().unwrap_or(""))
        .unwrap_or("")
        != "epub"
    {
        output_path.with_extension("epub")
    } else {
        output_path
    };

    // 列出图片
    let files = list_images(input.clone(), req.recursive)?;
    if files.is_empty() {
        return Err("未找到图像文件".into());
    }

    let original_bytes: u64 = files.iter().map(|f| f.size).sum();
    let total = files.len();

    // 创建 EPUB 文件：先写入临时文件，全部完成后再原子重命名，
    // 避免中途失败时破坏已存在的输出文件或留下半成品。
    let tmp_path = output_path.with_extension("epub.tmp.__writing__");
    let _ = std::fs::remove_file(&tmp_path);
    let file = File::create(&tmp_path).map_err(|e| format!("创建 EPUB 文件失败: {}", e))?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let options_deflate =
        FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // 写入 mimetype (必须是第一个文件，且不压缩)
    zip.start_file("mimetype", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(b"application/epub+zip")
        .map_err(|e| e.to_string())?;

    // 创建 META-INF/container.xml
    zip.start_file("META-INF/container.xml", options_deflate)
        .map_err(|e| e.to_string())?;
    zip.write_all(EPUB_CONTAINER_XML.as_bytes())
        .map_err(|e| e.to_string())?;

    // 并行处理图片（CPU 密集型操作）
    let processed_results: Vec<_> = files
        .par_iter()
        .enumerate()
        .map(|(idx, file_entry)| {
            emit_progress(
                &app,
                "epub_convert",
                idx + 1,
                total,
                file_entry.name.as_str(),
                id,
            );

            let image_id = format!("img{:04}", idx + 1);
            let xhtml_id = format!("page{:04}", idx + 1);
            let image_filename = format!("images/{}.jpg", image_id);
            let xhtml_filename = format!("text/{}.xhtml", xhtml_id);

            match process_image_for_epub(&file_entry.path, page_size) {
                Ok(jpeg_data) => {
                    let xhtml_content = generate_epub_page_xhtml(&image_filename, page_size);
                    Ok((
                        idx,
                        image_id,
                        xhtml_id,
                        image_filename,
                        xhtml_filename,
                        jpeg_data,
                        xhtml_content,
                    ))
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

    for (_idx, image_id, xhtml_id, image_filename, xhtml_filename, jpeg_data, xhtml_content) in
        successful_results
    {
        // 写入图片
        zip.start_file(format!("OEBPS/{}", image_filename), options_deflate)
            .map_err(|e| e.to_string())?;
        zip.write_all(&jpeg_data).map_err(|e| e.to_string())?;

        // 写入 XHTML 页面
        zip.start_file(format!("OEBPS/{}", xhtml_filename), options_deflate)
            .map_err(|e| e.to_string())?;
        zip.write_all(xhtml_content.as_bytes())
            .map_err(|e| e.to_string())?;

        image_items.push((image_id, image_filename));
        spine_items.push((xhtml_id, xhtml_filename));
    }

    // 生成并写入 content.opf
    let content_opf = generate_content_opf(input_name, &image_items, &spine_items);
    zip.start_file("OEBPS/content.opf", options_deflate)
        .map_err(|e| e.to_string())?;
    zip.write_all(content_opf.as_bytes())
        .map_err(|e| e.to_string())?;

    // 生成并写入 toc.ncx
    let toc_ncx = generate_toc_ncx(input_name, &spine_items);
    zip.start_file("OEBPS/toc.ncx", options_deflate)
        .map_err(|e| e.to_string())?;
    zip.write_all(toc_ncx.as_bytes())
        .map_err(|e| e.to_string())?;

    // 生成并写入 nav.xhtml
    let nav_xhtml = generate_nav_xhtml(&spine_items);
    zip.start_file("OEBPS/nav.xhtml", options_deflate)
        .map_err(|e| e.to_string())?;
    zip.write_all(nav_xhtml.as_bytes())
        .map_err(|e| e.to_string())?;

    // 写入 CSS
    zip.start_file("OEBPS/styles/style.css", options_deflate)
        .map_err(|e| e.to_string())?;
    zip.write_all(EPUB_STYLE_CSS.as_bytes())
        .map_err(|e| e.to_string())?;

    zip.finish().map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        e.to_string()
    })?;
    // 写入完成，原子重命名到目标路径。若中途 write_all 出错提前返回，
    // 函数会跳过此处；临时文件会在下次运行时被开头的 remove_file 清理。
    if std::fs::rename(&tmp_path, &output_path).is_err() {
        // 跨卷或目标占用时的回退：复制后删除临时文件。
        std::fs::copy(&tmp_path, &output_path).map_err(|e| format!("写出 EPUB 失败: {e}"))?;
        let _ = std::fs::remove_file(&tmp_path);
    }

    let output_bytes = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);

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
        (
            page_size.height.min(page_size.width),
            page_size.height.max(page_size.width),
        )
    } else {
        // 横屏图片：目标尺寸保持横屏 (宽矮)
        (
            page_size.height.max(page_size.width),
            page_size.height.min(page_size.width),
        )
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
fn generate_content_opf(
    title: &str,
    images: &[(String, String)],
    pages: &[(String, String)],
) -> String {
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
