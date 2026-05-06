use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::media::MediaFileInfo;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TraverseRequest {
  pub input_dir: PathBuf,
  #[serde(default)]
  pub output_dir: Option<PathBuf>,
  #[serde(default)]
  pub organize_by_format: bool,
  #[serde(default)]
  pub min_size_mb: Option<u64>,
  #[serde(default)]
  pub max_size_mb: Option<u64>,
  #[serde(default)]
  pub include_patterns: Vec<String>,
  #[serde(default)]
  pub exclude_patterns: Vec<String>,
  #[serde(default)]
  pub media_only_mode: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FormatStat {
  pub ext: String,
  pub scanned: usize,
  pub count: usize,
  pub filtered: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TraverseProblem {
  pub path: PathBuf,
  pub error: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TraverseResult {
  pub total_files: usize,
  pub copied: usize,
  pub skipped: usize,
  pub failed: usize,
  pub output_dir: PathBuf,
  pub elapsed_ms: u128,
  pub throughput_bytes: u64,
  pub format_stats: Vec<FormatStat>,
  pub problems: Vec<TraverseProblem>,
  pub log_path: PathBuf,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub media_files: Vec<MediaFileInfo>,
}
