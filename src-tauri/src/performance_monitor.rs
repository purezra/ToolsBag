use crate::models_performance::{SystemResourceInfo, PerformanceConfig, BatchOptimizationResult};
use std::sync::{Arc, Mutex};
use sysinfo::{System, SystemExt, ProcessExt, CpuExt};
use rayon::prelude::*;

/// 内存监控结构体
pub struct MemoryMonitor {
    system: System,
    config: PerformanceConfig,
    max_memory_mb: Option<u64>,
}

impl MemoryMonitor {
    /// 创建新的内存监控器
    pub fn new(config: PerformanceConfig) -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        
        let max_memory_mb = config.memory_limit_mb;
        
        Self {
            system,
            config,
            max_memory_mb,
        }
    }
    
    /// 获取当前系统资源信息
    pub fn get_system_info(&mut self) -> SystemResourceInfo {
        self.system.refresh_all();
        
        let total_memory = self.system.total_memory();
        let available_memory = self.system.available_memory();
        let used_memory = total_memory - available_memory;
        
        let memory_usage_percent = if total_memory > 0 {
            (used_memory as f64 / total_memory as f64) * 100.0
        } else {
            0.0
        };
        
        let cpu_usage = self.system.global_cpu_info().cpu_usage();
        
        SystemResourceInfo {
            total_memory_mb: total_memory / (1024 * 1024),
            available_memory_mb: available_memory / (1024 * 1024),
            used_memory_mb: used_memory / (1024 * 1024),
            memory_usage_percent,
            cpu_usage_percent: cpu_usage,
            is_under_pressure: self.is_system_under_pressure(),
        }
    }
    
    /// 判断系统是否处于压力状态
    fn is_system_under_pressure(&self) -> bool {
        self.system.refresh_memory();
        let memory_percent = self.get_current_memory_percent();
        let cpu_percent = self.get_current_cpu_percent();
        
        memory_percent > self.config.memory_pressure_threshold || 
        cpu_percent > self.config.cpu_pressure_threshold
    }
    
    /// 获取当前内存使用百分比
    fn get_current_memory_percent(&self) -> f64 {
        self.system.refresh_memory();
        let total = self.system.total_memory();
        let available = self.system.available_memory();
        let used = total - available;
        
        if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }
    
    /// 获取当前CPU使用百分比
    fn get_current_cpu_percent(&self) -> f64 {
        self.system.refresh_cpu();
        self.system.global_cpu_info().cpu_usage()
    }
    
    /// 根据当前内存状态计算最优批大小
    pub fn calculate_optimal_batch_size(&mut self, file_count: usize, estimated_memory_per_file_mb: u64) -> u32 {
        let system_info = self.get_system_info();
        
        // 计算理论最大批大小
        let max_batch_theoretical = match self.max_memory_mb {
            Some(limit) => {
                let safe_memory = limit as f64 * self.config.safety_margin;
                let memory_per_batch = estimated_memory_per_file_mb as f64 * file_count as f64;
                (safe_memory / memory_per_batch).floor() as u32
            },
            None => {
                // 使用可用内存的70%作为安全限制
                let safe_memory = system_info.available_memory_mb as f64 * 0.7;
                let memory_per_batch = estimated_memory_per_file_mb as f64 * file_count as f64;
                (safe_memory / memory_per_batch).floor() as u32
            }
        };
        
        // 根据系统压力调整
        let mut optimal_batch = if system_info.is_under_pressure {
            // 压力状态下，减少批大小
            (max_batch_theoretical as f64 * 0.5) as u32
        } else {
            // 正常状态下，使用75%的理论最大值
            (max_batch_theoretical as f64 * 0.75) as u32
        };
        
        // 限制在配置的范围内
        let min_batch = self.config.min_batch_size.max(1);
        let max_batch = self.config.max_batch_size.min(file_count as u32);
        
        optimal_batch = optimal_batch.clamp(min_batch, max_batch);
        
        // 如果仍然太大，再次检查内存压力
        if self.estimate_memory_usage(optimal_batch, estimated_memory_per_file_mb) > system_info.available_memory_mb {
            optimal_batch = (optimal_batch as f64 * 0.5) as u32;
            optimal_batch = optimal_batch.clamp(min_batch, max_batch);
        }
        
        optimal_batch
    }
    
    /// 估算指定批大小的内存使用量
    pub fn estimate_memory_usage(&self, batch_size: u32, estimated_memory_per_file_mb: u64) -> u64 {
        batch_size as u64 * estimated_memory_per_file_mb
    }
    
    /// 监控处理过程中的资源使用
    pub fn monitor_processing(&mut self, batch_size: u32, processing_time_ms: u64) -> bool {
        self.system.refresh_all();
        let system_info = self.get_system_info();
        
        // 如果系统压力过大，建议减少批大小
        let needs_adjustment = system_info.is_under_pressure || 
                               system_info.memory_usage_percent > 90.0 ||
                               system_info.cpu_usage_percent > 95.0;
        
        if needs_adjustment {
            // 记录调整建议
            log::warn!(
                "检测到系统压力过大: 内存使用 {:.1}%, CPU使用 {:.1}%",
                system_info.memory_usage_percent,
                system_info.cpu_usage_percent
            );
        }
        
        !needs_adjustment
    }
}

/// 时间预估模型
pub struct TimeEstimationModel {
    historical_data: Arc<Mutex<Vec<ProcessingRecord>>>,
    config: PerformanceConfig,
}

#[derive(Clone, Debug)]
pub struct ProcessingRecord {
    pub file_size_mb: f64,
    pub processing_time_ms: u64,
    pub file_type: String,
    pub timestamp: std::time::Instant,
}

impl TimeEstimationModel {
    /// 创建新的时间预估模型
    pub fn new(config: PerformanceConfig) -> Self {
        Self {
            historical_data: Arc::new(Mutex::new(Vec::new())),
            config,
        }
    }
    
    /// 记录处理数据
    pub fn record_processing(&self, file_size_mb: f64, processing_time_ms: u64, file_type: String) {
        if let Ok(mut data) = self.historical_data.lock() {
            let record = ProcessingRecord {
                file_size_mb,
                processing_time_ms,
                file_type,
                timestamp: std::time::Instant::now(),
            };
            
            data.push(record);
            
            // 保持历史数据在合理范围内
            if data.len() > self.config.max_history_records {
                data.remove(0);
            }
        }
    }
    
    /// 预估处理时间
    pub fn estimate_processing_time(&self, files: &[crate::models::FileEntry]) -> (u64, f64) {
        if let Ok(data) = self.historical_data.lock() {
            if data.is_empty() {
                // 没有历史数据，使用默认值
                let default_time_per_mb = 100.0; // 100ms per MB
                let total_mb: f64 = files.iter().map(|f| f.size as f64 / (1024.0 * 1024.0)).sum();
                let estimated_time = (total_mb * default_time_per_mb) as u64;
                return (estimated_time, 0.5); // 低置信度
            }
            
            // 按文件类型分组计算平均速度
            let mut type_speeds = std::collections::HashMap::new();
            
            for record in data.iter() {
                let speed_mb_per_ms = record.file_size_mb / record.processing_time_ms as f64;
                type_speeds
                    .entry(record.file_type.clone())
                    .or_insert_with(Vec::new)
                    .push(speed_mb_per_ms);
            }
            
            let mut total_estimated_time = 0u64;
            let mut total_weight = 0f64;
            let mut confidence_sum = 0f64;
            let mut confidence_count = 0;
            
            for file in files {
                let file_mb = file.size as f64 / (1024.0 * 1024.0);
                let file_type = &file.format;
                
                if let Some(speeds) = type_speeds.get(file_type) {
                    if !speeds.is_empty() {
                        let avg_speed: f64 = speeds.iter().sum::<f64>() / speeds.len() as f64;
                        if avg_speed > 0.0 {
                            let file_time = file_mb / avg_speed;
                            total_estimated_time += file_time as u64;
                            total_weight += 1.0;
                            
                            // 计算置信度（基于数据点的数量）
                            let confidence = (speeds.len() as f64 / self.config.confidence_threshold as f64).min(1.0);
                            confidence_sum += confidence;
                            confidence_count += 1;
                        }
                    }
                }
            }
            
            let overall_confidence = if confidence_count > 0 {
                confidence_sum / confidence_count as f64
            } else {
                0.3 // 低置信度
            };
            
            (total_estimated_time, overall_confidence)
        } else {
            // 锁获取失败，返回保守估计
            let default_time_per_mb = 150.0;
            let total_mb: f64 = files.iter().map(|f| f.size as f64 / (1024.0 * 1024.0)).sum();
            let estimated_time = (total_mb * default_time_per_mb) as u64;
            (estimated_time, 0.2)
        }
    }
    
    /// 获取历史数据统计
    pub fn get_statistics(&self) -> (usize, f64, u64) {
        if let Ok(data) = self.historical_data.lock() {
            let count = data.len();
            let avg_file_size = if count > 0 {
                data.iter().map(|r| r.file_size_mb).sum::<f64>() / count as f64
            } else {
                0.0
            };
            let avg_processing_time = if count > 0 {
                data.iter().map(|r| r.processing_time_ms).sum::<u64>() / count as u64
            } else {
                0
            };
            (count, avg_file_size, avg_processing_time)
        } else {
            (0, 0.0, 0)
        }
    }
}

/// 性能优化器主类
pub struct PerformanceOptimizer {
    memory_monitor: MemoryMonitor,
    time_estimator: TimeEstimationModel,
    config: PerformanceConfig,
}

impl PerformanceOptimizer {
    /// 创建性能优化器
    pub fn new(config: PerformanceConfig) -> Self {
        let memory_monitor = MemoryMonitor::new(config.clone());
        let time_estimator = TimeEstimationModel::new(config.clone());
        
        Self {
            memory_monitor,
            time_estimator,
            config,
        }
    }
    
    /// 分析并优化批处理
    pub fn analyze_and_optimize(&mut self, files: &[crate::models::FileEntry]) -> BatchOptimizationResult {
        let system_info = self.memory_monitor.get_system_info();
        let estimated_processing_time = self.time_estimator.estimate_processing_time(files);
        
        // 估算平均文件大小
        let avg_file_size_mb = if !files.is_empty() {
            files.iter().map(|f| f.size as f64 / (1024.0 * 1024.0)).sum::<f64>() / files.len() as f64
        } else {
            1.0
        };
        
        // 计算推荐批大小
        let recommended_batch_size = self.memory_monitor.calculate_optimal_batch_size(
            files.len(), 
            (avg_file_size_mb * 1.5) as u64 // 保守估计
        );
        
        let estimated_memory_usage = self.memory_monitor.estimate_memory_usage(
            recommended_batch_size, 
            (avg_file_size_mb * 1.5) as u64
        );
        
        BatchOptimizationResult {
            recommended_batch_size,
            estimated_processing_time_ms: estimated_processing_time.0,
            estimated_memory_usage_mb: estimated_memory_usage,
            confidence_score: estimated_processing_time.1,
            system_info,
            optimization_applied: true,
            notes: vec![
                format!("基于系统内存 ({:.1} MB 可用) 优化", system_info.available_memory_mb),
                format!("预估处理时间: {:.1} 秒", estimated_processing_time.0 as f64 / 1000.0),
                format!("置信度: {:.1}%", estimated_processing_time.1 * 100.0),
            ],
        }
    }
    
    /// 记录单个文件的处理结果
    pub fn record_file_processing(&self, file_size_mb: f64, processing_time_ms: u64, file_type: String) {
        self.time_estimator.record_processing(file_size_mb, processing_time_ms, file_type);
    }
    
    /// 监控批处理性能
    pub fn monitor_batch_processing(&mut self, batch_size: u32, processing_time_ms: u64) -> bool {
        self.memory_monitor.monitor_processing(batch_size, processing_time_ms)
    }
    
    /// 获取性能统计
    pub fn get_performance_stats(&self) -> (usize, f64, u64, SystemResourceInfo) {
        let (history_count, avg_file_size, avg_processing_time) = self.time_estimator.get_statistics();
        let mut system_info = self.memory_monitor.get_system_info();
        (history_count, avg_file_size, avg_processing_time, system_info)
    }
}
