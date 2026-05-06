use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeRequest {
  pub input_dir: PathBuf,
  pub recursive: bool,
  pub force_preprocess: bool,
  pub cache_max: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeResult {
  pub total_files: usize,
  pub problem_files: usize,
  pub compatible_files: usize,
  pub log_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImageIssue {
  pub path: PathBuf,
  pub reasons: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RepairRequest {
  pub input_dir: PathBuf,
  pub output_dir: Option<PathBuf>,
  pub only_problem: bool,
  pub cache_max: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RepairResult {
  pub repaired: usize,
  pub copied: usize,
  pub failed: usize,
  pub output_dir: PathBuf,
  pub log_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConvertRequest {
  pub input_dir: PathBuf,
  #[serde(default)]
  pub output_dir: Option<PathBuf>,
  #[serde(default)]
  pub batch_size: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConvertResult {
  pub batches: usize,
  pub output_path: PathBuf,
  pub log_path: PathBuf,
  pub problems: Vec<ImageIssue>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ProgressPayload {
  pub id: Uuid,
  pub stage: String,
  pub current: usize,
  pub total: usize,
  pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
  pub name: String,
  pub path: PathBuf,
  pub format: String,
  pub size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Convert3Request {
  pub input_dir: PathBuf,
  #[serde(default)]
  pub output_dir: Option<PathBuf>,
  #[serde(default)]
  pub batch_size: Option<usize>,
  #[serde(default = "default_lossless_merge")]
  pub lossless_merge: bool,
}

fn default_lossless_merge() -> bool {
  true
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Convert3Result {
  pub batches: usize,
  pub output_path: PathBuf,
  pub original_bytes: u64,
  pub output_bytes: u64,
  pub problems: Vec<ImageIssue>,
  pub log_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EpubConvertRequest {
  pub input_dir: PathBuf,
  #[serde(default)]
  pub output_dir: Option<PathBuf>,
  #[serde(default)]
  pub page_size: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EpubConvertResult {
  pub output_path: PathBuf,
  pub original_bytes: u64,
  pub output_bytes: u64,
  pub page_count: usize,
  pub page_size: String,
  pub problems: Vec<ImageIssue>,
}
