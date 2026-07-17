//! MediaInfo.dll 动态加载封装
//! 用于获取视频、音频、图片的详细元数据
//!
//! # 模块结构
//! - `parse` — 文本解析工具（码率/时长/帧率等）
//! - `meta` — 基础元数据提取（get_video_meta / get_audio_meta / get_image_meta）
//! - `detailed` — 视频体检详细元数据解析
//! - `xml` — XML 输出与格式转换（JSON / Markdown）

mod detailed;
mod meta;
mod parse;
mod xml;

pub use detailed::get_detailed_video_meta;
pub use meta::{get_audio_meta, get_image_meta, get_video_meta};
pub use xml::{get_complete_info, get_video_xml, get_xml_as_json, get_xml_as_markdown};

use libloading::Library;
use once_cell::sync::OnceCell;
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// MediaInfo 函数类型定义 - 使用 system 调用约定 (Windows stdcall)
type FnNew = unsafe extern "system" fn() -> *mut std::ffi::c_void;
type FnDelete = unsafe extern "system" fn(*mut std::ffi::c_void);
type FnOpenW = unsafe extern "system" fn(*mut std::ffi::c_void, *const u16) -> usize;
type FnClose = unsafe extern "system" fn(*mut std::ffi::c_void);
type FnInform = unsafe extern "system" fn(*mut std::ffi::c_void, usize) -> *const u16;
type FnOption =
    unsafe extern "system" fn(*mut std::ffi::c_void, *const u16, *const u16) -> *const u16;
type FnGetW = unsafe extern "system" fn(
    *mut std::ffi::c_void,
    usize,      // StreamKind (General=0)
    usize,      // StreamNumber (0)
    *const u16, // Parameter ("Duration")
    usize,      // InfoKind (Text=1)
    usize,      // SearchKind (0)
) -> *const u16;

/// 缓存的 MediaInfo 函数指针，避免每次调用重复查找符号
struct MediaInfoSymbols {
    new_fn: FnNew,
    delete_fn: FnDelete,
    open_fn: FnOpenW,
    close_fn: FnClose,
    inform_fn: FnInform,
    option_fn: FnOption,
    get_fn: FnGetW,
}

// Safety: 函数指针来自已加载的 DLL，只要 DLL 不卸载就有效，
// 而 Library 存储在 static OnceCell 中永远不会卸载。
unsafe impl Send for MediaInfoSymbols {}
unsafe impl Sync for MediaInfoSymbols {}

static MEDIAINFO_LIB: OnceCell<Option<Library>> = OnceCell::new();
static MEDIAINFO_SYMS: OnceCell<Option<MediaInfoSymbols>> = OnceCell::new();
static MEDIAINFO_PATH: OnceCell<Mutex<Option<String>>> = OnceCell::new();
static RESOURCE_DIR: OnceCell<PathBuf> = OnceCell::new();

/// 设置 Tauri 资源目录，供 MediaInfo DLL 查找使用
pub fn set_resource_dir(path: PathBuf) {
    RESOURCE_DIR.set(path).ok();
}

/// 获取 MediaInfo.dll 的加载路径
pub fn get_mediainfo_path() -> Option<String> {
    MEDIAINFO_PATH.get()?.lock().ok()?.clone()
}

/// 查找 MediaInfo 动态库的候选路径（纯运行期加载，不编译期嵌入）
///
/// 解析顺序：Tauri 资源目录 → 可执行文件目录 → 工作目录 → 系统 PATH。
/// 打包时由 tauri.conf.json 的 bundle.resources 将 MediaInfo.dll 带入资源目录；
/// 开发时把 DLL 放到 src-tauri/MediaInfo.dll 或系统 PATH 即可（该文件不入库）。
fn mediainfo_library_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // Tauri 资源目录（打包时 bundle.resources 的目标位置）
    if let Some(res_dir) = RESOURCE_DIR.get() {
        candidates.push(res_dir.join("MediaInfo.dll"));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("MediaInfo.dll"));
            candidates.push(dir.join("bin").join("windows").join("MediaInfo.dll"));
            candidates.push(dir.join("resources").join("MediaInfo.dll"));
            candidates.push(
                dir.join("resources")
                    .join("bin")
                    .join("windows")
                    .join("MediaInfo.dll"),
            );
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("MediaInfo.dll"));
                candidates.push(parent.join("resources").join("MediaInfo.dll"));
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("MediaInfo.dll"));
        candidates.push(cwd.join("src-tauri").join("MediaInfo.dll"));
        candidates.push(
            cwd.join("src-tauri")
                .join("bin")
                .join("windows")
                .join("MediaInfo.dll"),
        );
    }
    candidates.push(PathBuf::from("MediaInfo.dll"));

    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|path| seen.insert(path.to_string_lossy().to_ascii_lowercase()))
        .collect()
}

/// 初始化 MediaInfo 库
pub fn init_mediainfo() -> bool {
    MEDIAINFO_PATH.get_or_init(|| Mutex::new(None));

    MEDIAINFO_LIB.get_or_init(|| {
        for path in mediainfo_library_candidates() {
            match unsafe { Library::new(&path) } {
                Ok(lib) => {
                    println!("[MediaInfo] Loaded from: {:?}", path);
                    if let Some(mutex) = MEDIAINFO_PATH.get() {
                        if let Ok(mut guard) = mutex.lock() {
                            *guard = Some(path.display().to_string());
                        }
                    }
                    unsafe {
                        if lib.get::<FnNew>(b"MediaInfo_New\0").is_ok() {
                            println!("[MediaInfo] Found MediaInfo_New");
                            return Some(lib);
                        }
                    }
                }
                Err(e) => {
                    println!("[MediaInfo] Failed to load {:?}: {}", path, e);
                }
            }
        }

        println!("[MediaInfo] DLL not found in any path");
        None
    });

    // 缓存所有函数指针，避免后续每次调用重复查找符号
    MEDIAINFO_SYMS.get_or_init(|| {
        let lib = MEDIAINFO_LIB.get()?.as_ref()?;
        unsafe {
            let new_fn: FnNew = *lib.get::<FnNew>(b"MediaInfo_New\0").ok()?;
            let delete_fn: FnDelete = *lib.get::<FnDelete>(b"MediaInfo_Delete\0").ok()?;
            let open_fn: FnOpenW = *lib.get::<FnOpenW>(b"MediaInfo_Open\0").ok()?;
            let close_fn: FnClose = *lib.get::<FnClose>(b"MediaInfo_Close\0").ok()?;
            let inform_fn: FnInform = *lib.get::<FnInform>(b"MediaInfo_Inform\0").ok()?;
            let option_fn: FnOption = *lib.get::<FnOption>(b"MediaInfo_Option\0").ok()?;
            let get_fn: FnGetW = *lib.get::<FnGetW>(b"MediaInfo_Get\0").ok()?;
            println!("[MediaInfo] All 7 FFI symbols cached");
            Some(MediaInfoSymbols {
                new_fn,
                delete_fn,
                open_fn,
                close_fn,
                inform_fn,
                option_fn,
                get_fn,
            })
        }
    });

    is_mediainfo_available()
}

/// MediaInfo 句柄包装器（RAII）
pub(crate) struct MediaInfoHandle {
    handle: *mut std::ffi::c_void,
}

impl MediaInfoHandle {
    /// 打开文件，返回句柄（使用缓存的函数指针）
    pub fn open(path: &Path) -> Option<Self> {
        Self::open_inner(path, None)
    }

    /// 以指定 ParseSpeed 打开文件。`parse_speed` 为 "0"~"1"：
    /// 0.0 = 只读容器头部元数据（最快，适合批量轻量提取），1.0 = 完整解析。
    /// ParseSpeed 必须在 Open 之前设置，否则不生效。
    pub fn open_fast(path: &Path) -> Option<Self> {
        Self::open_inner(path, Some("0"))
    }

    fn open_inner(path: &Path, parse_speed: Option<&str>) -> Option<Self> {
        let syms = MEDIAINFO_SYMS.get()?.as_ref()?;
        unsafe {
            let handle = (syms.new_fn)();
            if handle.is_null() {
                return None;
            }

            if let Some(speed) = parse_speed {
                let key = to_wide_string("ParseSpeed");
                let val = to_wide_string(speed);
                (syms.option_fn)(handle, key.as_ptr(), val.as_ptr());
            }

            let path_wide = to_wide_string(&path.to_string_lossy());
            if (syms.open_fn)(handle, path_wide.as_ptr()) == 0 {
                (syms.delete_fn)(handle);
                return None;
            }

            Some(MediaInfoHandle { handle })
        }
    }

    /// 获取 Inform 文本输出
    pub fn get_inform(&self) -> String {
        let syms = match MEDIAINFO_SYMS.get() {
            Some(Some(s)) => s,
            _ => return String::new(),
        };
        unsafe {
            let opt_key = to_wide_string("Inform");
            let opt_val = to_wide_string("");
            (syms.option_fn)(self.handle, opt_key.as_ptr(), opt_val.as_ptr());

            let info_ptr = (syms.inform_fn)(self.handle, 0);
            from_wide_ptr(info_ptr)
        }
    }

    /// 获取完整信息输出（Complete 模式，包含所有字段）
    pub fn get_complete(&self) -> String {
        let syms = match MEDIAINFO_SYMS.get() {
            Some(Some(s)) => s,
            _ => return String::new(),
        };
        unsafe {
            // 先清除 Inform 模板，避免残留的自定义模板覆盖 Complete 输出
            let clear_key = to_wide_string("Inform");
            let clear_val = to_wide_string("");
            (syms.option_fn)(self.handle, clear_key.as_ptr(), clear_val.as_ptr());

            // 启用 Complete 模式（等同于 CLI 的 --Full）
            let opt_key = to_wide_string("Complete");
            let opt_val = to_wide_string("1");
            (syms.option_fn)(self.handle, opt_key.as_ptr(), opt_val.as_ptr());

            let info_ptr = (syms.inform_fn)(self.handle, 0);
            from_wide_ptr(info_ptr)
        }
    }

    /// 获取 XML 输出
    pub fn get_xml(&self) -> String {
        let syms = match MEDIAINFO_SYMS.get() {
            Some(Some(s)) => s,
            _ => return String::new(),
        };
        unsafe {
            let opt_key = to_wide_string("Inform");
            let opt_val = to_wide_string("XML");
            (syms.option_fn)(self.handle, opt_key.as_ptr(), opt_val.as_ptr());

            let info_ptr = (syms.inform_fn)(self.handle, 0);
            from_wide_ptr(info_ptr)
        }
    }

    /// 通过 MediaInfo_Get 获取指定流的 Duration 毫秒值
    /// StreamKind: General=0, Video=1, Audio=2, Text=3
    pub fn get_stream_duration_ms(&self, stream_kind: usize, stream_number: usize) -> u64 {
        self.get_stream_value(stream_kind, stream_number, "Duration")
            .trim()
            .parse()
            .unwrap_or(0)
    }

    /// 直接读取指定流字段，避免并发批量导入时 Inform 文本偶发缺列。
    pub fn get_stream_value(
        &self,
        stream_kind: usize,
        stream_number: usize,
        parameter: &str,
    ) -> String {
        const INFO_TEXT: usize = 1;
        let syms = match MEDIAINFO_SYMS.get() {
            Some(Some(s)) => s,
            _ => return String::new(),
        };
        unsafe {
            let param = to_wide_string(parameter);
            let ptr = (syms.get_fn)(
                self.handle,
                stream_kind,
                stream_number,
                param.as_ptr(),
                INFO_TEXT,
                0,
            );
            from_wide_ptr(ptr)
        }
    }

    /// 通过 MediaInfo_Get 直接获取 General/Duration 毫秒值
    /// 绕过默认 Inform 文本输出（超过1小时时常省略秒数）
    pub fn get_duration_ms(&self) -> u64 {
        self.get_stream_duration_ms(0, 0)
    }
}

impl Drop for MediaInfoHandle {
    fn drop(&mut self) {
        if let Some(Some(syms)) = MEDIAINFO_SYMS.get() {
            unsafe {
                (syms.close_fn)(self.handle);
                (syms.delete_fn)(self.handle);
            }
        }
    }
}

/// 检查 MediaInfo 是否可用
pub fn is_mediainfo_available() -> bool {
    mediainfo_is_available(
        MEDIAINFO_LIB.get().is_some_and(Option::is_some),
        MEDIAINFO_SYMS.get().is_some_and(Option::is_some),
    )
}

fn mediainfo_is_available(library_loaded: bool, symbols_loaded: bool) -> bool {
    library_loaded && symbols_loaded
}

/// 转换为宽字符串 (Windows UTF-16)
fn to_wide_string(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

/// 从宽字符指针读取字符串（带边界保护）
unsafe fn from_wide_ptr(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    const MAX_LEN: usize = 1_000_000; // 防止无终止符时越界
    let mut len = 0;
    while len < MAX_LEN && *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    String::from_utf16_lossy(slice)
}

#[cfg(test)]
mod tests {
    use super::mediainfo_is_available;

    #[test]
    fn availability_requires_library_and_all_symbols() {
        assert!(!mediainfo_is_available(true, false));
        assert!(!mediainfo_is_available(false, true));
        assert!(mediainfo_is_available(true, true));
    }
}
