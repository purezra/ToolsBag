//! ToolsBag 共享核心。
//!
//! Tauri 主程序（`src/main.rs`）和命令行工具（`src/bin/tools.rs`）都从这里
//! 引用媒体探测、数据库等模块，避免两套实现出现行为差异。

pub mod cmd;
pub mod codebook;
pub mod crypto;
pub mod detector;
pub mod error;
pub mod models;
pub mod repairer;
pub mod tools;
pub mod update;
pub mod utils;
pub mod webdav;
