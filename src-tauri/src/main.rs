// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cmd;
mod converter;
mod crypto;
mod detector;
mod codebook;
mod error;
mod models;
mod repairer;
mod utils;
mod tools;
mod webdav;

use crate::tools::media_batch;
use crate::tools::image_to_pdf;
use std::{path::PathBuf, process::Command};

// Windows 下用于隐藏子进程窗口
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[tauri::command]
fn open_parent_dir(path: String) -> Result<(), String> {
  let pb = PathBuf::from(path);
  #[cfg(target_os = "windows")]
  {
    if pb.exists() {
      let mut args: Vec<String> = Vec::new();
      if pb.is_file() {
        args.push("/select,".into());
        args.push(pb.to_string_lossy().to_string());
      } else {
        args.push(pb.to_string_lossy().to_string());
      }
      let mut cmd = Command::new("explorer");
      for arg in args {
        cmd.arg(arg);
      }

      // Windows 下也避免额外控制台（虽然 explorer 是 GUI，一般不会弹黑框，这里加上更保险）
      cmd.creation_flags(CREATE_NO_WINDOW);

      cmd.spawn().map_err(|e| e.to_string())?;
      return Ok(());
    }
  }
  #[cfg(target_os = "macos")]
  {
    if pb.exists() {
      Command::new("open")
        .arg("-R")
        .arg(&pb)
        .spawn()
        .map_err(|e| e.to_string())?;
      return Ok(());
    }
  }
  #[cfg(target_os = "linux")]
  {
    if pb.exists() {
      use std::process::Stdio;
      if let Some(dir) = pb.parent() {
        Command::new("xdg-open")
          .arg(dir)
          .stdout(Stdio::null())
          .stderr(Stdio::null())
          .spawn()
          .map_err(|e| e.to_string())?;
        return Ok(());
      }
    }
  }
  Err("无法打开目录".into())
}

#[tauri::command]
fn read_tool3_note() -> Result<String, String> {
  let mut candidates = vec![
    PathBuf::from("关于这个小工具的一些说明.md"),
    PathBuf::from("../关于这个小工具的一些说明.md"),
  ];
  if let Ok(exe) = std::env::current_exe() {
    if let Some(dir) = exe.parent() {
      candidates.push(dir.join("关于这个小工具的一些说明.md"));
      if let Some(parent) = dir.parent() {
        candidates.push(parent.join("关于这个小工具的一些说明.md"));
      }
    }
  }
  for path in candidates.iter() {
    if path.exists() {
      let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
      return Ok(String::from_utf8_lossy(&bytes).to_string());
    }
  }
  Err("未找到说明文件，请确认文件与应用同级".into())
}

fn main() {
  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_fs::init())
    .plugin(tauri_plugin_clipboard_manager::init())
    .plugin(tauri_plugin_shell::init())
    .invoke_handler(tauri::generate_handler![
      media_batch::import_media,
      media_batch::select_media_paths,
      media_batch::read_clipboard_paths,
      media_batch::get_mediainfo_status,
      media_batch::check_ffprobe_status,
      media_batch::check_exiftool_status,
      media_batch::import_detailed_video_info,
      open_parent_dir,
      read_tool3_note,
      cmd::analyze_folder,
      cmd::repair_images,
      cmd::convert_to_pdf,
      cmd::list_images,
      cmd::list_images_tool3,
      cmd::convert_tool3,
      cmd::convert_to_epub,
      cmd::traverse_copy,
      cmd::preview_media,
      cmd::check_mediainfo,
      cmd::codebook_status,
      cmd::codebook_init,
      cmd::codebook_unlock,
      cmd::codebook_lock,
      cmd::codebook_list,
      cmd::codebook_list_devices,
      cmd::codebook_save,
      cmd::codebook_delete,
      cmd::codebook_restore,
      cmd::codebook_save_config,
      cmd::codebook_change_password,
      cmd::codebook_revoke_device,
      cmd::codebook_export_vaultx,
      cmd::codebook_import_vaultx,
      cmd::codebook_preview_vaultx,
      // WebDAV Sync
      cmd::webdav_save_config,
      cmd::webdav_load_config,
      cmd::webdav_test_connection,
      cmd::webdav_get_status,
      cmd::webdav_start_sync,
      cmd::webdav_stop_sync,
      cmd::webdav_sync_entry,
      cmd::webdav_pull_changes,
      cmd::webdav_get_changelog,
      cmd::webdav_start_polling,
      cmd::webdav_stop_polling,
      // Tool5: Image to PDF
      image_to_pdf::analyze_folder_for_pdf,
      image_to_pdf::calculate_preview_layout,
      image_to_pdf::generate_pdf,
      image_to_pdf::get_image_thumbnail
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
