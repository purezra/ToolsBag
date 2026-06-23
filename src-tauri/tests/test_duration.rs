use std::os::windows::ffi::OsStrExt;
use std::ffi::OsStr;
use std::iter::once;
use libloading::Library;

type FnNew = unsafe extern "system" fn() -> *mut std::ffi::c_void;
type FnDelete = unsafe extern "system" fn(*mut std::ffi::c_void);
type FnOpenW = unsafe extern "system" fn(*mut std::ffi::c_void, *const u16) -> usize;
type FnClose = unsafe extern "system" fn(*mut std::ffi::c_void);
type FnInform = unsafe extern "system" fn(*mut std::ffi::c_void, usize) -> *const u16;
type FnOption = unsafe extern "system" fn(*mut std::ffi::c_void, *const u16, *const u16) -> *const u16;
type FnGetW = unsafe extern "system" fn(*mut std::ffi::c_void, usize, usize, *const u16, usize, usize) -> *const u16;

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

unsafe fn from_wide(ptr: *const u16) -> String {
    if ptr.is_null() { return String::new(); }
    let mut len = 0;
    while *ptr.add(len) != 0 && len < 1_000_000 { len += 1; }
    String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
}

#[test]
fn test_exact_code_path() {
    let path = r"E:\20260607135915\20260621162531\02_2h37m0s_gqn-012.mp4";
    let dll = std::env::temp_dir().join("toolsbag_mediainfo_0.2.0.dll");

    unsafe {
        let lib = Library::new(&dll).expect("load DLL");
        let new_fn: FnNew = *lib.get(b"MediaInfo_New\0").unwrap();
        let delete_fn: FnDelete = *lib.get(b"MediaInfo_Delete\0").unwrap();
        let open_fn: FnOpenW = *lib.get(b"MediaInfo_Open\0").unwrap();
        let close_fn: FnClose = *lib.get(b"MediaInfo_Close\0").unwrap();
        let inform_fn: FnInform = *lib.get(b"MediaInfo_Inform\0").unwrap();
        let option_fn: FnOption = *lib.get(b"MediaInfo_Option\0").unwrap();
        let get_fn: FnGetW = *lib.get(b"MediaInfo_Get\0").unwrap();

        let handle = new_fn();
        let path_wide = to_wide(path);
        open_fn(handle, path_wide.as_ptr());

        // 和 get_video_meta 一样：先 Option 清空 Inform 模板，再 Inform，再 Get
        let opt_key = to_wide("Inform");
        let opt_val = to_wide("");
        option_fn(handle, opt_key.as_ptr(), opt_val.as_ptr());

        let info_ptr = inform_fn(handle, 0);
        let _info = from_wide(info_ptr);

        // 现在 Get
        let param = to_wide("Duration");
        let ptr = get_fn(handle, 0, 0, param.as_ptr(), 1, 0);
        let raw = from_wide(ptr);
        let ms: u64 = raw.trim().parse().unwrap_or(0);

        println!("Exact code path: Get Duration raw='{}', parsed={}ms", raw.trim(), ms);
        if ms > 0 {
            println!("HMS: {}h{}m{}s", ms / 3600000, (ms % 3600000) / 60000, (ms % 60000) / 1000);
            println!("Minutes: {:.4}min", ms as f64 / 60000.0);
        } else {
            // 尝试不用 Option 直接 Inform
            println!("FAILED with Option+Inform path. Trying without Option...");
            let info_ptr2 = inform_fn(handle, 0);
            let _info2 = from_wide(info_ptr2);
            let ptr2 = get_fn(handle, 0, 0, param.as_ptr(), 1, 0);
            let raw2 = from_wide(ptr2);
            println!("Without re-Option: raw='{}'", raw2.trim());
        }

        close_fn(handle);
        delete_fn(handle);
    }
}
