use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

/// 外部感知体 — WorldviewBuffer
///
/// 收集外部事件，在每个 MainLoop tick 中被消费并注入系统提示词。
/// 自动注入当前时间、运行时长、时段上下文。
pub struct WorldviewBuffer {
    pending: VecDeque<Percept>,
    recent_slices: VecDeque<PerceptionSlice>,
    max_slices: usize,
    /// 启动时间
    start_time: SystemTime,
    /// 👁 Sight — 视觉感知（截屏描述），每心跳更新
    sight: Option<String>,
}

/// 单个感知
#[derive(Debug, Clone)]
pub struct Percept {
    pub text: String,
    pub source: PerceptSource,
    pub timestamp: i64,
}

/// 感知来源
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerceptSource {
    User,
    System,
    ToolResult,
    Timer,
}

/// 感知切片
#[derive(Debug, Clone)]
pub struct PerceptionSlice {
    pub percepts: Vec<Percept>,
    pub committed_at: i64,
}

impl WorldviewBuffer {
    pub fn new(max_slices: usize) -> Self {
        Self {
            pending: VecDeque::new(),
            recent_slices: VecDeque::with_capacity(max_slices + 1),
            max_slices,
            start_time: SystemTime::now(),
            sight: None,
        }
    }

    /// 推送一条感知
    pub fn push(&mut self, text: impl Into<String>, source: PerceptSource) {
        let now = now_millis();
        self.pending.push_back(Percept {
            text: text.into(),
            source,
            timestamp: now,
        });
    }

    /// 提交当前所有未处理感知为一个切片
    pub fn commit_slice(&mut self) -> Option<PerceptionSlice> {
        if self.pending.is_empty() {
            return None;
        }
        let now = now_millis();
        let percepts: Vec<Percept> = self.pending.drain(..).collect();
        let slice = PerceptionSlice { committed_at: now, percepts };
        self.recent_slices.push_back(slice.clone());
        while self.recent_slices.len() > self.max_slices {
            self.recent_slices.pop_front();
        }
        Some(slice)
    }

    /// 获取最近 N 个切片
    pub fn recent_slices(&self, n: usize) -> Vec<&PerceptionSlice> {
        self.recent_slices.iter().rev().take(n).collect()
    }

    /// 是否有未消费的感知
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// 👁 设置视觉感知（截屏描述）
    pub fn set_sight(&mut self, text: String) {
        self.sight = Some(text);
    }

    /// 清除视觉感知
    pub fn clear_sight(&mut self) {
        self.sight = None;
    }

    /// 构建完整世界观文本（含时间 + 运行时长 + 外部感知）
    pub fn build_worldview(&self) -> String {
        // 1. 当前时间
        let time_str = format_current_time();

        // 2. 运行时长
        let uptime_str = format_uptime(self.start_time);

        // 3. 👁 Sight（视觉感知）
        let sight_str = self.sight.as_ref()
            .map(|s| format!("👁 Sight: {s}"))
            .unwrap_or_default();

        // 4. 近期感知摘要
        let percepts_str = self.format_recent_percepts(3);

        let mut parts = vec![time_str, uptime_str];
        if !sight_str.is_empty() {
            parts.push(sight_str);
        }
        parts.push(percepts_str);

        format!(
            "<worldview>\n{}\n</worldview>",
            parts.join("\n")
        )
    }

    /// 格式化近期感知
    fn format_recent_percepts(&self, n: usize) -> String {
        let slices = self.recent_slices(n);
        if slices.is_empty() {
            return "本周期内无外部事件。".to_string();
        }
        let mut parts = Vec::new();
        for slice in slices {
            for percept in &slice.percepts {
                let source_str = match percept.source {
                    PerceptSource::User => "💬 用户",
                    PerceptSource::System => "⚙️ 系统",
                    PerceptSource::ToolResult => "🔧 工具",
                    PerceptSource::Timer => "⏰ 定时",
                };
                parts.push(format!("[{source_str}] {}", percept.text));
            }
        }
        parts.join("\n")
    }

    /// 旧的 format_for_worldview — 保留兼容
    pub fn format_for_worldview(&self, n: usize) -> String {
        let recent = self.format_recent_percepts(n);
        format!("## 外部感知\n{recent}")
    }

    /// 清除
    pub fn clear(&mut self) {
        self.pending.clear();
        self.recent_slices.clear();
        self.sight = None;
        self.start_time = SystemTime::now();
    }

    /// 重置启动时间
    pub fn reset_uptime(&mut self) {
        self.start_time = SystemTime::now();
    }
}

impl Default for WorldviewBuffer {
    fn default() -> Self {
        Self::new(7)
    }
}

// === 辅助函数 ===

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// 格式化当前时间（含时段上下文）
fn format_current_time() -> String {
    let now = SystemTime::now();
    let since_epoch = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since_epoch.as_secs();

    // 计算本地时间（UTC+8）
    let local_secs = secs + 8 * 3600;
    let hour = (local_secs / 3600) % 24;
    let minute = (local_secs / 60) % 60;
    let second = local_secs % 60;

    let period = match hour {
        5..=8 => "清晨",
        9..=11 => "上午",
        12..=13 => "中午",
        14..=17 => "下午",
        18..=21 => "傍晚",
        _ => "夜间",
    };

    // 年月日
    let days = (local_secs / 86400) as i64;
    let mut y = 1970i64;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let month_days = if is_leap(y) {
        [31,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        [31,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut m = 0;
    for &md in &month_days {
        if remaining < md { break; }
        remaining -= md;
        m += 1;
    }
    let d = remaining + 1;

    format!(
        "当前时间: {}/{:02}/{:02} {:02}:{:02}:{:02}（{period}）",
        y, m + 1, d, hour, minute, second
    )
}

/// 格式化运行时长
fn format_uptime(start: SystemTime) -> String {
    let elapsed = start.elapsed().unwrap_or_default();
    let total_secs = elapsed.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;

    if hours > 0 {
        format!("已持续运行: {hours} 小时 {minutes} 分钟")
    } else {
        format!("已持续运行: {minutes} 分钟")
    }
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_commit() {
        let mut wv = WorldviewBuffer::new(7);
        wv.push("你好", PerceptSource::User);
        assert!(wv.has_pending());
        let slice = wv.commit_slice();
        assert!(slice.is_some());
        assert_eq!(slice.unwrap().percepts.len(), 1);
        assert!(!wv.has_pending());
    }

    #[test]
    fn test_empty_commit() {
        let mut wv = WorldviewBuffer::new(7);
        assert!(wv.commit_slice().is_none());
    }

    #[test]
    fn test_recent_slices_limit() {
        let mut wv = WorldviewBuffer::new(3);
        for i in 0..5 {
            wv.push(format!("percept {i}"), PerceptSource::System);
            wv.commit_slice();
        }
        assert_eq!(wv.recent_slices(10).len(), 3);
    }

    #[test]
    fn test_format_for_worldview() {
        let mut wv = WorldviewBuffer::new(7);
        wv.push("测试消息", PerceptSource::User);
        wv.commit_slice();
        let formatted = wv.format_for_worldview(7);
        assert!(formatted.contains("外部感知"));
        assert!(formatted.contains("测试消息"));
    }

    #[test]
    fn test_build_worldview() {
        let mut wv = WorldviewBuffer::new(7);
        wv.push("你好", PerceptSource::User);
        wv.commit_slice();
        let wv_text = wv.build_worldview();
        assert!(wv_text.starts_with("<worldview>"));
        assert!(wv_text.contains("当前时间"));
        assert!(wv_text.contains("已持续运行"));
        assert!(wv_text.contains("💬 用户"));
        assert!(wv_text.ends_with("</worldview>"));
    }

    #[test]
    fn test_empty_worldview() {
        let wv = WorldviewBuffer::new(7);
        let wv_text = wv.build_worldview();
        assert!(wv_text.contains("当前时间"));
        assert!(wv_text.contains("无外部事件"));
    }
}
