// 性能优化相关结构体添加到主models.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 性能优化配置结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceConfig {
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_concurrent_threads")]
    pub concurrent_threads: usize,
    #[serde(default = "default_png_quality")]
    pub png_quality: u8,
    #[serde(default = "default_memory_threshold")]
    pub memory_threshold_mb: u64,
    #[serde(default = "default_enable_adaptive_batch")]
    pub enable_adaptive_batch: bool,
    #[serde(default = "default_enable_time_estimation")]
    pub enable_time_estimation: bool,
    #[serde(default = "default_enable_image_preprocessing")]
    pub enable_image_preprocessing: bool,
    #[serde(default = "default_max_image_size")]
    pub max_image_size_mb: u64,
}

fn default_batch_size() -> usize { 50 }
fn default_concurrent_threads() -> usize { 4 }
fn default_png_quality() -> u8 { 80 }
fn default_memory_threshold() -> u64 { 512 }
fn default_enable_adaptive_batch() -> bool { true }
fn default_enable_time_estimation() -> bool { true }
fn default_enable_image_preprocessing() -> bool { true }
fn default_max_image_size() -> u64 { 100 }

/// 时间预估结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimeEstimation {
    pub estimated_seconds: u64,
    pub confidence_level: f32,
    pub factors: Vec<String>,
    pub total_files: usize,
    pub total_size_bytes: u64,
}

/// 图片质量信息结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImageQualityInfo {
    pub path: PathBuf,
    pub format: String,
    pub resolution: (u32, u32),
    pub file_size: u64,
    pub quality_score: f32,
    pub has_issues: bool,
    pub issues: Vec<String>,
    pub recommended_actions: Vec<String>,
}

/// 处理性能指标结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingMetrics {
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub files_processed: usize,
    pub files_failed: usize,
    pub total_input_bytes: u64,
    pub total_output_bytes: u64,
    pub peak_memory_mb: u64,
    pub average_batch_size: f32,
    pub processing_speed_mbps: f32,
    pub compression_ratio: f32,
}

/// 批处理优化结果结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BatchOptimizationResult {
    pub recommended_batch_size: usize,
    pub estimated_improvement: f32,
    pub memory_usage_estimate: u64,
    pub processing_time_estimate: u64,
    pub current_performance_score: f32,
    pub optimized_performance_score: f32,
}

/// 系统资源监控结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SystemResourceInfo {
    pub total_memory_mb: u64,
    pub available_memory_mb: u64,
    pub cpu_cores: usize,
    pub disk_free_space_mb: u64,
    pub memory_usage_percent: f32,
}

/// 内存监控器结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMonitor {
    pub total_processed: usize,
    pub current_memory_usage_mb: u64,
    pub peak_memory_usage_mb: u64,
    pub low_memory_mode: bool,
    pub adaptive_batch_size: usize,
}

/// 性能分析结果结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceAnalysis {
    pub system_info: SystemResourceInfo,
    pub file_analysis: FileAnalysisResult,
    pub time_estimation: Option<TimeEstimation>,
    pub optimization_suggestions: Vec<String>,
    pub batch_recommendation: Option<BatchOptimizationResult>,
}

/// 文件分析结果结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileAnalysisResult {
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub format_distribution: std::collections::HashMap<String, usize>,
    pub size_distribution: SizeDistribution,
    pub complexity_score: f32,
    pub estimated_processing_time: u64,
}

/// 文件大小分布结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SizeDistribution {
    pub small_files: usize,    // < 1MB
    pub medium_files: usize,   // 1-10MB
    pub large_files: usize,    // 10-100MB
    pub huge_files: usize,     // > 100MB
    pub average_size_mb: f32,
    pub max_size_mb: u64,
}

/// 图片预处理选项结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImagePreprocessingOptions {
    pub auto_rotate: bool,
    pub auto_enhance: bool,
    pub noise_reduction: bool,
    pub color_correction: bool,
    pub quality_improvement: bool,
}

/// 渐进式预览配置结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PreviewConfig {
    pub enable_progressive_loading: bool,
    pub thumbnail_size: (u32, u32),
    pub preview_quality: u8,
    pub cache_enabled: bool,
    pub cache_size_mb: u64,
}

/// 实时处理状态结构体
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingStatus {
    pub current_file: Option<PathBuf>,
    pub files_completed: usize,
    pub files_total: usize,
    pub current_batch: usize,
    pub batches_total: usize,
    pub processing_speed_mbps: f32,
    pub estimated_time_remaining: Option<u64>,
    pub current_memory_usage_mb: u64,
    pub quality_improvements: usize,
    pub optimizations_applied: Vec<String>,
}
