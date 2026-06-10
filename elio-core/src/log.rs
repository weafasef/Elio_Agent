//! 审计日志系统 — 记录提示词/回复/时间到 JSONL
//!
//! 写入 logs/YYYY-MM-DD.jsonl，与现有 logview_gui.py 兼容。

use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 日志事件
#[derive(Debug, Clone, Serialize)]
pub struct LogEvent {
    /// ISO 8601 时间戳
    pub timestamp: String,
    /// 事件类型
    #[serde(rename = "type")]
    pub event_type: String,
    /// 数据内容
    pub data: String,
    /// 来源（system/user/elio）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 会话 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// 审计日志记录器
pub struct AuditLogger {
    dir: PathBuf,
    buffer: Mutex<Vec<LogEvent>>,
    flush_interval: std::time::Duration,
}

impl AuditLogger {
    /// 创建审计日志记录器
    ///
    /// `dir`: 日志目录（默认为 `./logs/`）
    pub fn new(dir: PathBuf) -> Self {
        fs::create_dir_all(&dir).ok();
        Self {
            dir,
            buffer: Mutex::new(Vec::with_capacity(100)),
            flush_interval: std::time::Duration::from_secs(5),
        }
    }

    /// 记录事件
    pub fn log(&self, event_type: &str, data: &str, source: Option<&str>) {
        let now = format_timestamp();
        let event = LogEvent {
            timestamp: now,
            event_type: event_type.to_string(),
            data: data.to_string(),
            source: source.map(String::from),
            session_id: None,
        };

        if let Ok(mut buf) = self.buffer.lock() {
            buf.push(event);
        }
    }

    /// 记录事件（带 session_id）
    pub fn log_with_session(
        &self,
        event_type: &str,
        data: &str,
        source: Option<&str>,
        session_id: Option<&str>,
    ) {
        let now = format_timestamp();
        let event = LogEvent {
            timestamp: now,
            event_type: event_type.to_string(),
            data: data.to_string(),
            source: source.map(String::from),
            session_id: session_id.map(String::from),
        };

        if let Ok(mut buf) = self.buffer.lock() {
            buf.push(event);
        }
    }

    /// 立即刷新到磁盘
    pub fn flush(&self) {
        let events: Vec<LogEvent> = {
            let mut buf = self.buffer.lock().unwrap();
            let mut drained = Vec::new();
            std::mem::swap(&mut drained, &mut *buf);
            drained
        };

        if events.is_empty() {
            return;
        }

        let path = self.current_log_path();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap_or_else(|_| {
                fs::create_dir_all(&self.dir).ok();
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .expect("无法创建日志文件")
            });

        for event in &events {
            if let Ok(line) = serde_json::to_string(event) {
                writeln!(file, "{}", line).ok();
            }
        }
    }

    /// 获取当前日志文件路径 logs/YYYY-MM-DD.jsonl
    fn current_log_path(&self) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs() + 8 * 3600; // UTC+8
        let days = secs / 86400;
        let remaining = secs % 86400;
        let hour = remaining / 3600;
        let min = (remaining % 3600) / 60;
        let sec = remaining % 60;

        // 简单日期计算
        let mut y = 1970i64;
        let mut d = days as i64;
        loop {
            let days_in_year = if is_leap(y) { 366 } else { 365 };
            if d < days_in_year { break; }
            d -= days_in_year;
            y += 1;
        }
        let month_days = if is_leap(y) {
            [31,29,31,30,31,30,31,31,30,31,30,31]
        } else {
            [31,28,31,30,31,30,31,31,30,31,30,31]
        };
        let mut m = 0;
        for &md in &month_days {
            if d < md { break; }
            d -= md;
            m += 1;
        }
        let day = d + 1;

        let filename = format!("{:04}-{:02}-{:02}.jsonl", y, m + 1, day);
        self.dir.join(filename)
    }
}

impl Drop for AuditLogger {
    fn drop(&mut self) {
        self.flush();
    }
}

/// 格式化 ISO 8601 时间戳（UTC+8）
fn format_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = now.as_secs() + 8 * 3600; // UTC+8
    let nanos = now.subsec_nanos();
    let days = total_secs / 86400;
    let remaining = total_secs % 86400;
    let hour = remaining / 3600;
    let min = (remaining % 3600) / 60;
    let sec = remaining % 60;
    let millis = nanos / 1_000_000;

    let mut y = 1970i64;
    let mut d = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if d < days_in_year { break; }
        d -= days_in_year;
        y += 1;
    }
    let month_days = if is_leap(y) {
        [31,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        [31,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut m = 0;
    for &md in &month_days {
        if d < md { break; }
        d -= md;
        m += 1;
    }
    let day = d + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}+08:00",
        y, m + 1, day, hour, min, sec, millis
    )
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// === 便捷的事件类型常量 ===

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
        assert!(ts.len() > 20);
        assert!(ts.ends_with("+08:00"));
        // 应该是当前年份
        assert!(ts.starts_with("2026"));
    }

    #[test]
    fn test_log_and_flush() {
        let dir = std::env::temp_dir().join("elio_test_audit");
        let _ = fs::remove_dir_all(&dir);

        let logger = AuditLogger::new(dir.clone());
        logger.log(EVENT_USER_MESSAGE, "你好", Some("user"));
        logger.log(EVENT_ELIO_RESPONSE, "こんにちは", Some("elio"));
        logger.flush();

        // 验证文件存在
        let path = logger.current_log_path();
        assert!(path.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_event_format() {
        let event = LogEvent {
            timestamp: "2026-06-10T15:09:43.770+08:00".into(),
            event_type: "user.message".into(),
            data: "你好".into(),
            source: Some("user".into()),
            session_id: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"#));
        assert!(json.contains(r#""user.message"#));
        assert!(json.contains(r#""你好"#));
    }
}
