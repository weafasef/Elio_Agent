//! 审计日志 — 每次调用立即写入 logs/YYYY-MM-DD.jsonl
//!
//! 格式兼容 logview_gui.py，每行一个 JSON 事件。

use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// 日志事件
#[derive(Debug, Clone, Serialize)]
pub struct LogEvent {
    pub timestamp: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// 审计日志 — 立刻写入，不缓冲
pub struct AuditLogger {
    dir: PathBuf,
    /// 缓存当前日志文件路径，避免每天重复计算
    current_path: Mutex<Option<PathBuf>>,
}

impl AuditLogger {
    pub fn new(dir: PathBuf) -> Self {
        fs::create_dir_all(&dir).ok();
        Self {
            dir,
            current_path: Mutex::new(None),
        }
    }

    /// 记录事件，立即写入文件
    pub fn log(&self, event_type: &str, data: &str, source: Option<&str>) {
        let event = LogEvent {
            timestamp: format_timestamp(),
            event_type: event_type.to_string(),
            data: data.to_string(),
            source: source.map(String::from),
            session_id: None,
        };
        self.write_event(&event);
    }

    /// 带 session_id 的记录
    pub fn log_with_session(
        &self,
        event_type: &str,
        data: &str,
        source: Option<&str>,
        session_id: Option<&str>,
    ) {
        let event = LogEvent {
            timestamp: format_timestamp(),
            event_type: event_type.to_string(),
            data: data.to_string(),
            source: source.map(String::from),
            session_id: session_id.map(String::from),
        };
        self.write_event(&event);
    }

    fn write_event(&self, event: &LogEvent) {
        let path = self.get_or_create_path();
        if let Ok(line) = serde_json::to_string(event) {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(file, "{line}");
            }
        }
    }

    /// 获取当前日志文件路径（缓存避免重复计算）
    fn get_or_create_path(&self) -> PathBuf {
        if let Ok(mut cache) = self.current_path.lock() {
            if let Some(ref path) = *cache {
                return path.clone();
            }
            let path = daily_log_path(&self.dir);
            *cache = Some(path.clone());
            path
        } else {
            daily_log_path(&self.dir)
        }
    }
}

/// 计算当天日志文件路径
fn daily_log_path(dir: &PathBuf) -> PathBuf {
    let now = now_secs() / 86400; // 转换为天数
    let (y, m, d) = date_from_epoch(now);
    let filename = format!("{y:04}-{m:02}-{d:02}.jsonl");
    dir.join(filename)
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let total = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    total + 8 * 3600 // UTC+8
}

/// 格式化 ISO 8601 时间戳 (UTC+8)
pub fn format_timestamp() -> String {
    let total = now_secs();
    let sec = total % 60;
    let min = (total / 60) % 60;
    let hour = (total / 3600) % 24;
    let days = total / 86400;
    let (y, m, d) = date_from_epoch(days);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis();
    format!(
        "{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}.{millis:03}+08:00"
    )
}

fn date_from_epoch(days: i64) -> (i64, i64, i64) {
    let mut y = 1970i64;
    let mut d = days;
    loop {
        let days_in = if is_leap(y) { 366 } else { 365 };
        if d < days_in { break; }
        d -= days_in;
        y += 1;
    }
    let month_days = if is_leap(y) {
        [31,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        [31,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut m = 0i64;
    for &md in &month_days {
        if d < md { break; }
        d -= md;
        m += 1;
    }
    (y, m + 1, d + 1)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// === 事件类型常量 ===

pub const EVENT_USER_MESSAGE: &str = "user.message";
pub const EVENT_SYSTEM_PROMPT: &str = "system.prompt";
pub const EVENT_ELIO_RESPONSE: &str = "elio.response";
pub const EVENT_SYSTEM_HEARTBEAT: &str = "system.heartbeat";
pub const EVENT_API_REQUEST: &str = "api.request";
pub const EVENT_API_RESPONSE: &str = "api.response";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp() {
        let ts = format_timestamp();
        assert!(ts.len() > 20, "timestamp too short: {ts}");
        assert!(ts.ends_with("+08:00"), "tz missing: {ts}");
        assert!(ts.starts_with("2026"), "year: {ts}");
    }

    #[test]
    fn test_log_writes_file() {
        let dir = std::env::temp_dir().join("elio_test_log_write");
        let _ = fs::remove_dir_all(&dir);

        let logger = AuditLogger::new(dir.clone());
        logger.log(EVENT_USER_MESSAGE, "测试消息", Some("user"));
        logger.log(EVENT_ELIO_RESPONSE, "こんにちは", Some("elio"));

        // 验证文件被写入
        let files: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert!(!files.is_empty(), "没有日志文件被创建");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_event_json_format() {
        let json = serde_json::json!({
            "timestamp": "2026-06-10T15:09:43.770+08:00",
            "type": "user.message",
            "data": "你好",
            "source": "user",
        });
        assert_eq!(json["type"], "user.message");
        assert_eq!(json["data"], "你好");
    }
}
