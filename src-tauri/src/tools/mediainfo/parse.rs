//! MediaInfo 文本解析工具函数
//! 格式化/解析 码率、时长、帧率、文件大小等

/// 格式化码率显示
pub fn format_bitrate(bps: u64) -> String {
    if bps >= 1_000_000 {
        format!("{:.3} Mbps", bps as f64 / 1_000_000.0)
    } else if bps > 0 {
        format!("{:.3} kbps", bps as f64 / 1_000.0)
    } else {
        "-".to_string()
    }
}

/// 格式化时长 (毫秒 -> HH:MM:SS)
pub fn format_duration(ms: u64) -> String {
    if ms == 0 {
        return "-".to_string();
    }
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{:02}:{:02}", mins, secs)
    }
}

/// 格式化帧率（保留2位小数，整数时不显示小数，不显示FPS）
pub fn format_frame_rate(value: &str) -> String {
    // 提取数字部分
    let num_str: String = value
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();

    if let Ok(fps) = num_str.parse::<f64>() {
        if fps == fps.floor() {
            // 整数（安全截断）
            format!("{}", fps.clamp(0.0, u32::MAX as f64) as u32)
        } else {
            // 保留2位小数，去掉末尾的0
            let formatted = format!("{:.2}", fps);
            let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
            trimmed.to_string()
        }
    } else {
        value.to_string()
    }
}

/// 解析数字（去除单位）
pub fn parse_number(s: &str) -> u32 {
    let s = s.replace(" ", "").replace(",", "");
    // 提取数字部分
    let num_str: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    num_str.parse().unwrap_or(0)
}

/// 解析码率
pub fn parse_bitrate(s: &str) -> u64 {
    let s_lower = s.to_lowercase();
    let s_clean = s.replace(" ", "").replace(",", "");

    // 提取数字
    let num_str: String = s_clean
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let num: f64 = num_str.parse().unwrap_or(0.0);

    if s_lower.contains("mb/s") || s_lower.contains("mbps") {
        (num * 1_000_000.0) as u64
    } else if s_lower.contains("kb/s") || s_lower.contains("kbps") {
        (num * 1_000.0) as u64
    } else {
        num as u64
    }
}

/// 解析时长到毫秒（防溢出）
/// 支持格式：
/// - MediaInfo 标准格式："2 h 30 min 45 s 123 ms"（空格分隔）
/// - MediaInfo 连写格式："2h 30min 45s 123ms"（数字和单位连写）
/// - 纯毫秒："1502456"
/// - HH:MM:SS 或 HH:MM:SS.mmm
/// - 小数秒："22.500 s"、"22.5s"
pub fn parse_duration_to_ms(s: &str) -> u64 {
    let s = s.trim();
    if s.is_empty() {
        return 0;
    }

    // 尝试 HH:MM:SS 或 HH:MM:SS.mmm 格式
    if let Some(ms) = try_parse_hhmmss(s) {
        return ms;
    }

    let mut total_ms: u64 = 0;
    let parts: Vec<&str> = s.split_whitespace().collect();

    let mut i = 0;
    while i < parts.len() {
        let part = parts[i];

        // 尝试解析数字（支持小数，如 "22.500"）
        if let Some(ms) = parse_decimal_token_to_ms(part) {
            // 看下一个 token 是否是时间单位
            if i + 1 < parts.len() {
                let unit = parts[i + 1].to_lowercase();
                if is_time_unit(&unit) {
                    let unit_ms = apply_time_unit_ms(ms, &unit);
                    total_ms = total_ms.saturating_add(unit_ms);
                    i += 2;
                    continue;
                }
            }
            // 无单位：最后一个 token 视为毫秒
            total_ms = total_ms.saturating_add(ms);
            i += 1;
        } else if let Some((ms, unit)) = split_decimal_and_unit(part) {
            // 连写格式："2h"、"30min"、"45.5s"、"123ms"
            let unit_ms = apply_time_unit_ms(ms, &unit);
            total_ms = total_ms.saturating_add(unit_ms);
            i += 1;
        } else {
            i += 1;
        }
    }

    total_ms
}

/// 将小数 token 解析为毫秒级数值（无单位）
/// "22" → 22, "22.500" → 22, "9422500" → 9422500
fn parse_decimal_token_to_ms(s: &str) -> Option<u64> {
    let num: f64 = s.parse().ok()?;
    if num < 0.0 || !num.is_finite() {
        return None;
    }
    // 四舍五入到整数毫秒
    Some((num + 0.5) as u64)
}

/// 判断 token 是否是时间单位
fn is_time_unit(unit: &str) -> bool {
    let u = unit.to_lowercase();
    u.starts_with("h")
        || u.starts_with("min")
        || u == "mn"
        || (u.starts_with("s") && !u.starts_with("ms"))
        || u.starts_with("ms")
}

/// 将毫秒级数值按时间单位换算
fn apply_time_unit_ms(base_ms: u64, unit: &str) -> u64 {
    if unit.starts_with("h") {
        base_ms.saturating_mul(3600).saturating_mul(1000)
    } else if unit.starts_with("min") || unit == "mn" {
        base_ms.saturating_mul(60).saturating_mul(1000)
    } else if unit.starts_with("s") && !unit.starts_with("ms") {
        base_ms.saturating_mul(1000)
    } else if unit.starts_with("ms") {
        base_ms
    } else {
        0
    }
}

/// 尝试从 "2h"、"30min"、"45.5s"、"123ms" 中拆分小数数字和单位
fn split_decimal_and_unit(s: &str) -> Option<(u64, String)> {
    // 数字部分可以包含小数点
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(s.len());
    if num_end == 0 {
        return None;
    }
    let num_str = &s[..num_end];
    let unit = &s[num_end..];
    if num_str.is_empty() || unit.is_empty() {
        return None;
    }
    let num: f64 = num_str.parse().ok()?;
    if num < 0.0 || !num.is_finite() {
        return None;
    }
    let base_ms = (num + 0.5) as u64;
    Some((base_ms, unit.to_lowercase()))
}

/// 尝试解析 HH:MM:SS 或 HH:MM:SS.mmm 格式
fn try_parse_hhmmss(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let h: u64 = parts[0].trim().parse().ok()?;
        let m: u64 = parts[1].trim().parse().ok()?;
        let s_part = parts[2].trim();
        // 处理可能的小数秒 "SS.mmm"
        if let Some(dot_pos) = s_part.find('.') {
            let sec: u64 = s_part[..dot_pos].parse().unwrap_or(0);
            let frac_str = &s_part[dot_pos + 1..];
            // 补齐到3位毫秒
            let frac_ms: u64 = match frac_str.len() {
                1 => frac_str.parse::<u64>().unwrap_or(0) * 100,
                2 => frac_str.parse::<u64>().unwrap_or(0) * 10,
                3 => frac_str.parse::<u64>().unwrap_or(0),
                _ => frac_str.parse::<u64>().unwrap_or(0) / 10_u64.pow(frac_str.len() as u32 - 3),
            };
            let total_ms = h * 3_600_000 + m * 60_000 + sec * 1000 + frac_ms;
            Some(total_ms)
        } else {
            let sec: u64 = s_part.parse().unwrap_or(0);
            Some(h * 3_600_000 + m * 60_000 + sec * 1000)
        }
    } else {
        None
    }
}

/// 解析文件大小字符串为字节数
pub fn parse_file_size(s: &str) -> u64 {
    let s_clean = s.replace([' ', ','], "");
    let num_str: String = s_clean
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let num: f64 = num_str.parse().unwrap_or(0.0);
    let s_lower = s.to_lowercase();
    if s_lower.contains("gib") || s_lower.contains("gb") {
        (num * 1_073_741_824.0) as u64
    } else if s_lower.contains("mib") || s_lower.contains("mb") {
        (num * 1_048_576.0) as u64
    } else if s_lower.contains("kib") || s_lower.contains("kb") {
        (num * 1024.0) as u64
    } else {
        num as u64
    }
}
