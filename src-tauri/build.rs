use std::path::{Path, PathBuf};

/// 构建期嵌入的本地二进制。
///
/// 这些文件体积大、许可证各异，不进版本库（见 `.gitignore`），
/// 但 `include_bytes!` 要求它在编译时存在。缺失时给出可执行的修复指引，
/// 而不是让编译器直接抛一个「couldn't read ...」。
struct RequiredBinary {
    path: PathBuf,
    what: &'static str,
    hint: &'static str,
}

fn required_binaries() -> Vec<RequiredBinary> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    vec![
        RequiredBinary {
            path: root.join("..").join("MediaInfo.dll"),
            what: "MediaInfo.dll",
            hint: "下载 MediaInfo 的 Windows 64 位 DLL（https://mediaarea.net/en/MediaInfo/Download/Windows），\
解压后把 MediaInfo.dll 放到仓库根目录（与 package.json 同级）。",
        },
        RequiredBinary {
            path: root.join("bin").join("libjxl").join("cjxl.exe"),
            what: "src-tauri/bin/libjxl/cjxl.exe",
            hint: "从 https://github.com/libjxl/libjxl/releases 下载 jxl-x64-windows-static.zip，\
把其中的 cjxl.exe 放到 src-tauri/bin/libjxl/。",
        },
    ]
}

fn check_binaries() {
    let required = required_binaries();
    let missing: Vec<&RequiredBinary> = required.iter().filter(|b| !b.path.is_file()).collect();

    if missing.is_empty() {
        return;
    }

    let mut message = String::from("缺少构建所需的二进制文件：\n");
    for bin in &missing {
        message.push_str(&format!(
            "\n  ✗ {}\n    需要：{}\n    修复：{}\n",
            bin.path.display(),
            bin.what,
            bin.hint
        ));
    }
    message.push_str("\n这两个文件由 include_bytes! 在编译期嵌入，因此必须先准备好再构建。");
    panic!("{}", message);
}

fn main() {
    // 只有目标平台是 Windows 时才会 include_bytes! 这些文件
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        check_binaries();
        for bin in required_binaries() {
            emit_rerun_if_changed(&bin.path);
        }
    }

    tauri_build::build();
}

/// 文件变化时重新运行 build.rs（include_bytes! 本身已跟踪，这里只是让检查尽早触发）
fn emit_rerun_if_changed(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}
