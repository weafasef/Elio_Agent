use std::collections::VecDeque;

/// 外部感知体 — WorldviewBuffer
///
/// 收集外部事件（用户消息、系统事件），在 MainLoop tick 中被消费。
/// 维护最近 N 个感知切片作为短期记忆。
pub struct WorldviewBuffer {
    /// 未消费的感知
    pending: VecDeque<Percept>,
    /// 最近 N 个已提交的切片
    recent_slices: VecDeque<PerceptionSlice>,
    /// 最大切片保留数
    max_slices: usize,
}

/// 单个感知
#[derive(Debug, Clone)]
pub struct Percept {
    /// 感知内容
    pub text: String,
    /// 来源
    pub source: PerceptSource,
    /// 时间戳
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

/// 已提交的感知切片（一组感知的快照）
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
        }
    }

    /// 推送一条感知
    pub fn push(&mut self, text: impl Into<String>, source: PerceptSource) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        self.pending.push_back(Percept {
            text: text.into(),
            source,
            timestamp: now,
        });
    }

    /// 消费并提交当前所有未处理感知为一个切片
    pub fn commit_slice(&mut self) -> Option<PerceptionSlice> {
        if self.pending.is_empty() {
            return None;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let percepts: Vec<Percept> = self.pending.drain(..).collect();

        let slice = PerceptionSlice {
            committed_at: now,
            percepts,
        };

        self.recent_slices.push_back(slice.clone());

        // 保持切片数量上限
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

    /// 格式化 worldview 文本（供系统提示词注入）
    pub fn format_for_worldview(&self, n: usize) -> String {
        let slices = self.recent_slices(n);
        if slices.is_empty() {
            return "当前无外部感知。".to_string();
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

        format!("## 外部感知\n{}", parts.join("\n"))
    }

    /// 清除所有数据
    pub fn clear(&mut self) {
        self.pending.clear();
        self.recent_slices.clear();
    }
}

impl Default for WorldviewBuffer {
    fn default() -> Self {
        Self::new(7) // 默认保留 7 个切片
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_commit() {
        let mut wv = WorldviewBuffer::new(7);
        assert!(!wv.has_pending());

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
}
