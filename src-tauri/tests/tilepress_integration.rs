//! Tilepress 集成回归：用真实图片跑通「扫描 → 排版 → 预览 → PDF/WebP 导出」。
//!
//! 测试目录固定为 `D:\Desktop\Tilepress\测试专用`（本机验收用）；目录不存在时跳过，
//! 这样在别的机器上 `cargo test` 不会误报失败。

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tilepress::encode::Quality;
use tilepress::imgout::{write_page_with_resolution, OutFmt};
use tilepress::pdf::Mode;
use tilepress::{
    build_layout, export_pdf, make_preview_cache, scan_dir, LayoutKind, LayoutOptions, PageKind,
};

const TEST_DIR: &str = r"D:\Desktop\Tilepress\测试专用";

fn test_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(TEST_DIR);
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("跳过：测试目录不存在 -> {TEST_DIR}");
        None
    }
}

#[test]
fn real_images_scan_layout_preview_and_export() {
    let Some(dir) = test_dir() else { return };

    let cancel = Arc::new(AtomicBool::new(false));

    // 1) 扫描：只读文件头拿尺寸，不解码像素。
    let scan = scan_dir(&dir, cancel.clone(), &|_| {}).expect("扫描测试图片失败");
    let info = scan.info();
    assert!(info.count >= 50, "测试目录图片数量异常：{}", info.count);
    assert!(info.total_bytes > 0);

    // 2) 排版：A4 竖版、经典网格、10mm 页边距，保持完整内容。
    let opts = LayoutOptions {
        page: PageKind::A4P,
        layout: LayoutKind::Grid,
        margin_mm: 10.0,
        gutter_mm: 1.0,
        per_page: 64,
        mode: Mode::Fit,
        quality: Quality::Lossless,
        iters: 40_000,
        seed: 42,
        ..LayoutOptions::default()
    };
    let layout = build_layout(&scan, &opts, cancel.clone(), &|_| {}).expect("排版失败");
    assert_eq!(layout.images.len(), info.count);
    assert!(!layout.pages.is_empty());
    assert!(layout.fill_avg > 0.80, "填充率过低：{:.3}", layout.fill_avg);

    let out_dir = std::env::temp_dir().join("toolsbag-tilepress-e2e");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("创建测试输出目录失败");

    // 3) 预览：与最终导出共用同一份 LayoutDoc。
    let layout = Arc::new(layout);
    let cache = make_preview_cache(&out_dir.join("preview"), &layout);
    let (preview_path, _) = cache.render(&layout, 0).expect("生成首页预览失败");
    assert!(preview_path.is_file());
    assert!(
        std::fs::metadata(&preview_path)
            .map(|m| m.len())
            .unwrap_or(0)
            > 1_000,
        "预览图异常"
    );

    // 4) PDF 导出：原图无损直塞，再用 lopdf 校验页数。
    let pdf_path = out_dir.join("tilepress-test.pdf");
    let result =
        export_pdf(layout.clone(), &pdf_path, cancel.clone(), &|_| {}).expect("导出 PDF 失败");
    assert!(result.bytes > 10_000, "PDF 体积异常：{}", result.bytes);
    assert_eq!(result.pages, layout.pages.len());

    let doc = lopdf::Document::load(&pdf_path).expect("读取生成的 PDF 失败");
    assert_eq!(doc.get_pages().len(), layout.pages.len(), "PDF 页数不一致");

    // 5) 图片导出：WebP 整页输出，验证第二条导出链路。
    let webp_path = out_dir.join("page-1.webp");
    let (written, bytes) =
        write_page_with_resolution(&layout, 0, Some(150), OutFmt::Webp, 100, &webp_path)
            .expect("导出 WebP 失败");
    assert_eq!(written.as_path(), webp_path.as_path());
    assert!(bytes > 1_000, "WebP 体积异常：{bytes}");
    assert!(webp_path.is_file());

    // 6) 图片过滤：只保留横图，验证 filter_images 与 UI 统计口径一致。
    let landscape = tilepress::filter_images(&scan.images, None, None, Some(1.01), None);
    assert!(landscape.len() <= info.count);
    assert!(landscape
        .iter()
        .all(|img| img.width as f32 / img.height as f32 >= 1.01));

    eprintln!(
        "Tilepress E2E OK: {} 张图 → {} 页，填充率 {:.1}%，PDF {} bytes，输出目录 {}",
        info.count,
        layout.pages.len(),
        layout.fill_avg * 100.0,
        result.bytes,
        out_dir.display()
    );
}

#[test]
fn preview_cache_reuses_rendered_page() {
    let Some(dir) = test_dir() else { return };
    let cancel = Arc::new(AtomicBool::new(false));
    let scan = scan_dir(&dir, cancel.clone(), &|_| {}).expect("扫描失败");
    let opts = LayoutOptions {
        page: PageKind::A4P,
        per_page: 64,
        layout: LayoutKind::Grid,
        mode: Mode::Fit,
        ..LayoutOptions::default()
    };
    let layout = Arc::new(build_layout(&scan, &opts, cancel, &|_| {}).expect("排版失败"));
    let root = std::env::temp_dir().join("toolsbag-tilepress-cache-test");
    let _ = std::fs::remove_dir_all(&root);
    let cache = make_preview_cache(&root, &layout);

    let (first, cached_first) = cache.render(&layout, 0).expect("首次渲染失败");
    assert!(!cached_first);
    let (second, cached_second) = cache.render(&layout, 0).expect("二次渲染失败");
    assert_eq!(first, second);
    assert!(cached_second, "第二次渲染应命中磁盘缓存");

    // 路径辅助函数仅用于确认缓存根目录可写。
    assert!(cache.root.starts_with(&root));
    assert!(cache.root.is_dir());
}
