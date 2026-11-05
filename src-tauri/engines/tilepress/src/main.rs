//! tilepress 命令行入口：参数解析后调用 `tilepress::run_job`，行为与旧版一致。

use tilepress::encode::Quality;
use tilepress::pdf::Mode;
use tilepress::{recommend, run_job, JobOptions, LayoutKind, PageKind, Progress, Stage};
use std::path::PathBuf;

struct Args {
    dir: String,
    page: String,
    n: usize,
    margin: f32,
    gutter: f32,
    iters: usize,
    out: Option<String>,
    pdf: Option<String>,
    limit: Option<usize>,
    seed: u64,
    layout: LayoutKind,
    mode: Mode,
    even: bool,
    cluster: bool,
    min_per: Option<usize>,
    max_per: Option<usize>,
    min_w: Option<u32>,
    min_h: Option<u32>,
    ar_min: Option<f32>,
    ar_max: Option<f32>,
    img_out: Option<String>,
    img_fmt: Option<String>,
    img_dpi: u32,
    img_q: u8,
    pages: Option<usize>,
    quality: Quality,
    dpi: Option<u32>,
    plan: Option<Vec<usize>>,
    rec: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        dir: ".".into(),
        page: "a4p".into(),
        n: 16,
        margin: 10.0,
        gutter: 2.0,
        iters: 40_000,
        out: None,
        pdf: None,
        limit: None,
        seed: 42,
        layout: LayoutKind::Grid,
        mode: Mode::Cover,
        even: true,
        cluster: true,
        min_per: None,
        max_per: None,
        min_w: None,
        min_h: None,
        ar_min: None,
        ar_max: None,
        img_out: None,
        img_fmt: None,
        img_dpi: 0,
        img_q: 100,
        pages: None,
        quality: Quality::Lossless,
        dpi: None,
        plan: None,
        rec: false,
    };
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let k = v[i].clone();
        let next = |i: &mut usize| -> String {
            *i += 1;
            v.get(*i).cloned().unwrap_or_default()
        };
        match k.as_str() {
            "--dir" => a.dir = next(&mut i),
            "--page" => a.page = next(&mut i),
            "--n" => a.n = next(&mut i).parse().unwrap_or(16),
            "--margin" => a.margin = next(&mut i).parse().unwrap_or(10.0),
            "--gutter" => a.gutter = next(&mut i).parse().unwrap_or(2.0),
            "--iters" => a.iters = next(&mut i).parse().unwrap_or(40_000),
            "--out" => a.out = Some(next(&mut i)),
            "--pdf" => a.pdf = Some(next(&mut i)),
            "--limit" => a.limit = next(&mut i).parse().ok(),
            "--seed" => a.seed = next(&mut i).parse().unwrap_or(42),
            "--layout" => {
                a.layout = LayoutKind::from_key(&next(&mut i)).unwrap_or(LayoutKind::Grid)
            }
            "--mode" => {
                a.mode = match next(&mut i).as_str() {
                    "fit" => Mode::Fit,
                    _ => Mode::Cover,
                }
            }
            "--no-even" => a.even = false,
            "--no-cluster" => a.cluster = false,
            "--min-per-page" => {
                a.min_per = next(&mut i).parse::<usize>().ok().filter(|&v| v > 0)
            }
            "--max-per-page" => {
                a.max_per = next(&mut i).parse::<usize>().ok().filter(|&v| v > 0)
            }
            "--min-w" => a.min_w = next(&mut i).parse::<u32>().ok().filter(|&v| v > 0),
            "--min-h" => a.min_h = next(&mut i).parse::<u32>().ok().filter(|&v| v > 0),
            "--ar-min" => a.ar_min = next(&mut i).parse::<f32>().ok().filter(|&v| v > 0.0),
            "--ar-max" => a.ar_max = next(&mut i).parse::<f32>().ok().filter(|&v| v > 0.0),
            "--img-out" => a.img_out = Some(next(&mut i)),
            "--img-fmt" => a.img_fmt = Some(next(&mut i)),
            "--img-dpi" => {
                let dpi = next(&mut i).parse::<u32>().unwrap_or(0);
                a.img_dpi = if dpi == 0 { 0 } else { dpi.clamp(36, 600) };
            }
            "--img-q" => a.img_q = next(&mut i).parse::<u8>().unwrap_or(88).clamp(10, 100),
            "--pages" => a.pages = next(&mut i).parse().ok(),
            "--quality" => {
                a.quality = Quality::from_key(&next(&mut i)).unwrap_or(Quality::Lossless)
            }
            "--rec" => a.rec = true,
            "--plan" => {
                a.plan = Some(
                    next(&mut i)
                        .split(',')
                        .filter_map(|x| x.trim().parse::<usize>().ok())
                        .collect(),
                )
            }
            "--dpi" => {
                a.dpi = next(&mut i).parse::<u32>().ok().filter(|&v| v > 0)
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }
    a
}

fn print_help() {
    println!(
        "tilepress —— 图片批量拼贴成 PDF\n\n\
         用法：\n  \
         tilepress --dir <图片文件夹> --page a4p|a4l|a3p|a3l --n 64 --margin 10 --gutter 2 \\\n      \
         --layout mosaic|grid --mode cover|fit --quality lossless|high|standard|small --dpi 300 \\\n      \
         --pdf out.pdf --out layout.json [--pages K] [--plan 62,64,60] [--no-even] [--no-cluster] [--min-per-page M] [--max-per-page N] [--limit N]"
    );
    println!("图片输出：--img-out <文件夹> --img-fmt webp|avif|png [--img-dpi 0|150|300|600] [--img-q 1..100]\n默认保留原图像素（DPI=0），质量=100：WebP 无损 / AVIF 最高质量。");
}

fn main() {
    let a = parse_args();
    let page = match PageKind::from_key(&a.page) {
        Some(p) => p,
        None => {
            eprintln!("未知幅面：{}（可选 a4p/a4l/a3p/a3l）", a.page);
            std::process::exit(2);
        }
    };

    let page_kind = match a.page.as_str() {
        "a4l" => PageKind::A4L,
        "a3p" => PageKind::A3P,
        "a3l" => PageKind::A3L,
        _ => PageKind::A4P,
    };

    if a.rec {
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;
        let cancel = Arc::new(AtomicBool::new(false));
        let scan = match tilepress::scan_dir(std::path::Path::new(&a.dir), cancel, &|_| {}) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("错误：{:#}", e);
                std::process::exit(1);
            }
        };
        let rec = tilepress::recommend_bounded(
            &scan.images,
            page_kind,
            a.margin,
            a.gutter,
            a.min_per,
            a.max_per,
        );
        println!("图片总数   {}", scan.images.len());
        println!(
            "幅面       {} 边距 {} mm 间距 {} mm",
            a.page, a.margin, a.gutter
        );
        if a.min_per.is_some() || a.max_per.is_some() {
            println!(
                "每页张数限制 {} – {}",
                a.min_per.map(|v| v.to_string()).unwrap_or_else(|| "不限".into()),
                a.max_per.map(|v| v.to_string()).unwrap_or_else(|| "不限".into())
            );
        }
        println!("候选方案   {} 个", rec.options.len());
        for (i, o) in rec.options.iter().enumerate() {
            println!(
                "  [{}] {} 页 · {} · 填充 {:.2}% · 面积差 {:.2} 倍 · --plan {}",
                i + 1,
                o.pages,
                o.plan
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join("/"),
                o.fill * 100.0,
                o.spread,
                o.plan
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        return;
    }

    let opts = JobOptions {
        dir: PathBuf::from(&a.dir),
        page,
        margin_mm: a.margin,
        gutter_mm: a.gutter,
        per_page: a.n.max(1),
        per_page_plan: a.plan.clone(),
        pages: a.pages,
        mode: a.mode,
        quality: a.quality,
        dpi: a.dpi,
        out_pdf: a.pdf.as_ref().map(PathBuf::from),
        out_json: a.out.as_ref().map(PathBuf::from),
        seed: a.seed,
        layout: a.layout,
        iters: a.iters,
        even: a.even,
        cluster: a.cluster,
        min_w: a.min_w,
        min_h: a.min_h,
        ar_min: a.ar_min,
        ar_max: a.ar_max,
        limit: a.limit,
    };

    let rec = recommend(page, a.margin, 0);
    let result = run_job(opts, |p: Progress| match p.stage {
        Stage::Layout => eprintln!("  {}", p.message),
        Stage::WritingPdf => eprintln!("  {}", p.message),
        _ => {}
    });

    let r = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("错误：{:#}", e);
            std::process::exit(1);
        }
    };

    println!("图片总数   {}", r.images);
    println!(
        "幅面       {} ({:.0}x{:.0} mm)",
        page.key(),
        r.page_w_mm,
        r.page_h_mm
    );
    println!(
        "每页张数   {} ({})",
        r.per_page,
        if a.even { "均摊" } else { "顺序" }
    );
    println!("页数       {}", r.pages);
    println!("边距/间隙  {} mm / {} mm", a.margin, a.gutter);
    println!("推荐张数   {} 张/页 (建议 {}-{})", rec.mid, rec.lo, rec.hi);
    println!(
        "推荐页数   {} 页 (按 {} 张/页)",
        (r.images + rec.mid - 1) / rec.mid,
        rec.mid
    );
    println!("扫描耗时   {:.1} ms", r.ms_scan as f64);
    println!("排版耗时   {:.1} ms", r.ms_layout as f64);
    println!("平均填充   {:.2}%", r.fill_avg * 100.0);

    if let Some(out) = &a.out {
        println!("版面数据   {}", out);
    }
    // 导出图片（WebP / AVIF / PNG）：独立跑一遍扫描 + 排版
    if let Some(dir) = &a.img_out {
        let run = || -> anyhow::Result<()> {
        let fmt = match a.img_fmt.as_deref().unwrap_or("webp") {
            "avif" => tilepress::imgout::OutFmt::Avif,
            "png" => tilepress::imgout::OutFmt::Png,
            _ => tilepress::imgout::OutFmt::Webp,
        };
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let scan = tilepress::scan_dir(
            std::path::Path::new(&a.dir),
            cancel.clone(),
            &|_| {},
        )?;
        let lo = tilepress::LayoutOptions {
            page,
            margin_mm: a.margin,
            gutter_mm: a.gutter,
            per_page: a.n.max(1),
            per_page_plan: a.plan.clone(),
            pages: a.pages,
            layout: a.layout,
            mode: a.mode,
            quality: a.quality,
            dpi: a.dpi,
            seed: a.seed,
            iters: a.iters,
            even: a.even,
            cluster: a.cluster,
            min_w: a.min_w,
            min_h: a.min_h,
            ar_min: a.ar_min,
            ar_max: a.ar_max,
        };
        let layout = tilepress::build_layout(&scan, &lo, cancel.clone(), &|_| {})?;
        let out_dir = PathBuf::from(dir);
        std::fs::create_dir_all(&out_dir).ok();
        let base: String = std::path::Path::new(&a.dir)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "tilepress".into());
        let base: String = base.chars().take(10).collect();
        let t = std::time::Instant::now();
        let n_pages = layout.pages.len();
        let pool = tilepress::build_pool(if fmt == tilepress::imgout::OutFmt::Avif {
            1
        } else {
            tilepress::worker_count().min(4).max(1)
        });
        let sizes: Vec<u64> = {
            use rayon::prelude::*;
            pool.install(|| -> anyhow::Result<Vec<u64>> {
                (0..n_pages)
                    .into_par_iter()
                    .map(|page_i| -> anyhow::Result<u64> {
                        let path = if n_pages == 1 {
                            out_dir.join(format!("{}.{}", base, fmt.ext()))
                        } else {
                            out_dir.join(format!("{}-p{}.{}", base, page_i + 1, fmt.ext()))
                        };
                        let (_, n) = tilepress::imgout::write_page_with_resolution(
                            &layout,
                            page_i,
                            if a.img_dpi == 0 { None } else { Some(a.img_dpi) },
                            fmt,
                            a.img_q,
                            &path,
                        )?;
                        Ok(n)
                    })
                    .collect::<anyhow::Result<Vec<u64>>>()
            })?
        };
        let total: u64 = sizes.iter().sum();
        println!(
            "图片输出   {} 页 {}  {} ({:.1} MB, {:.0} ms)",
            layout.pages.len(),
            fmt.label(),
            out_dir.display(),
            total as f64 / 1048576.0,
            t.elapsed().as_millis() as f64
        );
        Ok(())
        };
        if let Err(e) = run() {
            eprintln!("错误：{:#}", e);
            std::process::exit(1);
        }
    }

    if let Some(p) = &r.pdf_path {
        println!(
            "PDF 输出   {}  ({:.1} MB, {:.0} ms)",
            p,
            r.pdf_bytes as f64 / 1048576.0,
            r.ms_pdf as f64
        );
    }
}
