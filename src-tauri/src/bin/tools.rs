//! ToolsBag 命令行工具。
//!
//! 用法：
//!   tools v <文件或目录>... [--no-recursive] [--xml] [--quiet] [--json] [--db <路径>]
//!
//! 批量提取视频元数据（与桌面端「视频体检」同一条 MediaInfo 解析链路），
//! 结果写入桌面端共用的 `video_records.db`，打开应用即可在「历史记录 / 数据库统计」中查看。

use app::tools::{media_cache, mediainfo, video_probe, video_store};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

const APP_IDENTIFIER: &str = "com.toolsbag.app";

struct Args {
    paths: Vec<String>,
    recursive: bool,
    xml: bool,
    quiet: bool,
    json: bool,
    db: Option<PathBuf>,
}

fn usage() -> String {
    format!(
        "ToolsBag CLI {}\n\n\
用法:\n  \
tools v <文件或目录>... [选项]\n\n\
命令:\n  \
v, video            批量提取视频元数据并入库（目录默认递归）\n\n\
选项:\n  \
--no-recursive      只扫描目录第一层\n  \
--xml               同时保存 MediaInfo 原始 XML（更慢、库更大）\n  \
--quiet             只输出最终汇总\n  \
--json              以 JSON 输出汇总（隐含 --quiet）\n  \
--db <路径>         指定 video_records.db（默认 {})\n  \
-h, --help          显示帮助\n  \
-V, --version       显示版本\n\n\
环境变量:\n  \
TOOLSBAG_DB         同 --db\n  \
TOOLSBAG_HOME       覆盖应用数据目录（默认 {}）",
        env!("CARGO_PKG_VERSION"),
        default_db_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<不可用>".into()),
        app_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<不可用>".into()),
    )
}

fn parse_args() -> Result<Args, String> {
    let mut raw = std::env::args().skip(1);
    let Some(command) = raw.next() else {
        return Err(usage());
    };

    match command.as_str() {
        "-h" | "--help" => {
            print!("{}", usage());
            std::process::exit(0);
        }
        "-V" | "--version" => {
            println!("tools {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }
        "v" | "video" => {}
        other => return Err(format!("未知命令：{}\n\n{}", other, usage())),
    }

    let mut args = Args {
        paths: Vec::new(),
        recursive: true,
        xml: false,
        quiet: false,
        json: false,
        db: std::env::var_os("TOOLSBAG_DB").map(PathBuf::from),
    };

    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--no-recursive" => args.recursive = false,
            "--recursive" => args.recursive = true,
            "--xml" => args.xml = true,
            "--quiet" => args.quiet = true,
            "--json" => {
                args.json = true;
                args.quiet = true;
            }
            "--db" => {
                let Some(path) = raw.next() else {
                    return Err("--db 需要一个路径参数".into());
                };
                args.db = Some(PathBuf::from(path));
            }
            "-h" | "--help" => {
                print!("{}", usage());
                std::process::exit(0);
            }
            other if other.starts_with('-') => {
                return Err(format!("未知选项：{}\n\n{}", other, usage()))
            }
            other => args.paths.push(other.to_string()),
        }
    }

    if args.paths.is_empty() {
        return Err(format!("请至少指定一个文件或目录\n\n{}", usage()));
    }

    Ok(args)
}

fn app_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("TOOLSBAG_HOME") {
        return Some(PathBuf::from(dir));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return Some(PathBuf::from(appdata).join(APP_IDENTIFIER));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join(APP_IDENTIFIER),
            );
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
            return Some(PathBuf::from(data).join(APP_IDENTIFIER));
        }
        if let Some(home) = std::env::var_os("HOME") {
            return Some(
                PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join(APP_IDENTIFIER),
            );
        }
    }

    None
}

fn default_db_path() -> Option<PathBuf> {
    app_dir().map(|dir| dir.join("video_records.db"))
}

fn format_duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "-".into();
    }
    let total = seconds.round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn item_line(done: usize, total: usize, item: &app::models::VideoInfoItem) -> String {
    let status = if item.status == "success" { "OK  " } else { "FAIL" };
    let resolution = match (item.width, item.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => format!("{w}x{h}"),
        _ => "-".into(),
    };
    let codec = item.codec.as_deref().unwrap_or("-");
    let duration = item
        .duration_sec
        .map(format_duration)
        .unwrap_or_else(|| "-".into());
    let size = format_size(item.size);
    if item.status == "success" {
        format!(
            "[{done:>4}/{total:<4}] {status} {resolution:<11} {codec:<10} {duration:>9} {size:>9}  {}",
            item.name
        )
    } else {
        format!(
            "[{done:>4}/{total:<4}] {status} {:<47} {}（{}）",
            "无法解析",
            item.name,
            item.reason.as_deref().unwrap_or("未知错误")
        )
    }
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let num_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8);
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build_global()
        .ok();

    if !mediainfo::init_mediainfo() {
        eprintln!("MediaInfo 不可用：找不到内嵌或随程序分发的 MediaInfo.dll");
        std::process::exit(2);
    }

    let db_path = args
        .db
        .clone()
        .or_else(default_db_path)
        .unwrap_or_else(|| PathBuf::from("video_records.db"));

    if let Some(dir) = app_dir() {
        media_cache::init_cache(&dir.join("media_cache.db"));
    }

    let started = Instant::now();
    let paths = match video_probe::collect_video_paths(&args.paths, args.recursive) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("扫描失败：{error}");
            std::process::exit(1);
        }
    };

    if !args.quiet {
        println!("ToolsBag CLI {}", env!("CARGO_PKG_VERSION"));
        println!("MediaInfo: {}", mediainfo::get_mediainfo_path().unwrap_or_default());
        println!("数据库:    {}", db_path.display());
        println!(
            "扫描:      {} 个视频{}",
            paths.len(),
            if args.recursive { "（递归）" } else { "" }
        );
        println!();
    }

    if paths.is_empty() {
        if args.json {
            println!(
                "{}",
                serde_json::json!({
                    "database": db_path.display().to_string(),
                    "total": 0,
                    "success": 0,
                    "failed": 0,
                    "saved": 0,
                    "elapsedMs": started.elapsed().as_millis() as u64,
                })
            );
        } else {
            println!("未找到视频文件。");
        }
        return;
    }

    let total = paths.len();
    let progress = AtomicUsize::new(0);
    let items: Vec<app::models::VideoInfoItem> = paths
        .par_iter()
        .enumerate()
        .map(|(idx, path)| {
            let item = video_probe::probe_video_item(idx as u64, path);
            let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
            if !args.quiet {
                println!("{}", item_line(done, total, &item));
            }
            item
        })
        .collect();

    let success = items.iter().filter(|i| i.status == "success").count();
    let failed = items.len().saturating_sub(success);

    let mut xml_map = HashMap::new();
    if args.xml {
        for item in items.iter().filter(|i| i.status == "success") {
            if let Some(xml) = mediainfo::get_video_xml(Path::new(&item.path)) {
                xml_map.insert(item.path.clone(), xml);
            }
        }
    }

    let saved_items: Vec<_> = items
        .iter()
        .filter(|i| i.status == "success")
        .cloned()
        .collect();

    let saved = match video_store::open(&db_path) {
        Ok(mut conn) => match video_store::upsert_items(&mut conn, &saved_items, &xml_map) {
            Ok(count) => count,
            Err(error) => {
                eprintln!("\n写入数据库失败：{error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("\n无法打开数据库 {}：{error}", db_path.display());
            std::process::exit(1);
        }
    };

    let elapsed = started.elapsed();
    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "database": db_path.display().to_string(),
                "total": total,
                "success": success,
                "failed": failed,
                "saved": saved,
                "elapsedMs": elapsed.as_millis() as u64,
            })
        );
    } else {
        println!();
        println!(
            "完成：成功 {success} / 失败 {failed} / 共 {total}，入库 {saved} 条，耗时 {}",
            format_duration(elapsed.as_secs_f64())
        );
        println!("数据库：{}", db_path.display());
        if failed > 0 {
            println!("失败项未写入数据库，可稍后重试。");
        }
    }

    if failed == total {
        std::process::exit(1);
    }
}
