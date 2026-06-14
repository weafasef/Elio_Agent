//! MemorySystem trait — 记忆系统的抽象接口
//!
//! MainLoop 只依赖这个 trait，不关心具体实现。
//! 只有两个连接点：
//!   - 输入: record_event() / tick()
//!   - 输出: get_context() → 注入系统提示词
//! 可以轻松替换为不同的记忆后端（图记忆、向量数据库、外部服务等）。

use super::{
    bridge::ContextBridge,
    disk::DiskError,
    fast::FastPath,
    graph::GraphStore,
    index::InvertedIndex,
    slow::{SlowPath, SlowPathResult},
    types::*,
};
use std::path::PathBuf;

/// 记忆事件 — 喂给记忆系统的唯一输入格式
#[derive(Debug, Clone)]
pub struct MemoryEvent {
    pub text: String,
    pub event_type: EventType,
    pub session_id: Option<String>,
}

/// 记忆统计信息
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    pub event_count: usize,
    pub edge_count: usize,
    pub keyword_count: usize,
}

/// 记忆系统 trait
///
/// 设计原则：
/// - 输入: record_event()（每次消息）+ tick()（定时维护）
/// - 输出: get_context() → 被 PromptManager 读取注入系统提示词
/// - save/load 用于持久化，与上下文无关
#[async_trait::async_trait]
pub trait MemorySystem: Send + Sync {
    /// 记录一条新事件
    fn record_event(&mut self, event: MemoryEvent);

    /// 获取用于系统提示词注入的记忆上下文
    fn get_context(&self) -> String;

    /// 定时维护 tick（触发 SlowPath 等后台任务）
    async fn tick(&mut self);

    /// 持久化到磁盘
    fn save(&self) -> Result<(), DiskError>;

    /// 从磁盘恢复
    fn load(&mut self) -> Result<(), DiskError>;

    /// 统计信息
    fn stats(&self) -> MemoryStats;

    /// 清除所有记忆
    fn clear(&mut self);
}

/// 图记忆系统 — 默认实现
///
/// 内部组合 GraphStore + InvertedIndex + FastPath + SlowPath + ContextBridge
/// 所有内部细节对外部隐藏，外部只通过 MemorySystem trait 交互。
pub struct GraphMemorySystem {
    store: GraphStore,
    index: InvertedIndex,
    disk: Option<super::disk::DiskIO>,
    slow_path: SlowPath,
    context_bridge: ContextBridge,
    fast_config: FastPathConfig,
}

impl GraphMemorySystem {
    /// 创建新的图记忆系统
    ///
    /// * `memory_dir` — 持久化目录，指向 `~/.elio/memory/`
    ///   传 None 则不持久化（仅内存运行）
    /// * `llm_callback` — SlowPath 使用的 LLM 回调
    ///   传 None 则跳过 LLM 推理（仅 FastPath）
    pub fn new(
        memory_dir: Option<PathBuf>,
        llm_callback: Option<Box<dyn Fn(&str) -> String + Send + Sync>>,
    ) -> Self {
        let disk = memory_dir.map(super::disk::DiskIO::new);
        let mut slow_path = SlowPath::new(SlowPathConfig::default());
        if let Some(cb) = llm_callback {
            slow_path.set_llm_callback(cb);
        }

        Self {
            store: GraphStore::new(),
            index: InvertedIndex::new(),
            disk,
            slow_path,
            context_bridge: ContextBridge::new(),
            fast_config: FastPathConfig::default(),
        }
    }

    /// 获取 ContextBridge 引用（供 PromptManager 读取）
    pub fn bridge(&self) -> &ContextBridge {
        &self.context_bridge
    }

    /// 获取 GraphStore 引用（供外部遍历/查询）
    pub fn graph(&self) -> &GraphStore {
        &self.store
    }

    /// 获取倒排索引引用
    pub fn index(&self) -> &InvertedIndex {
        &self.index
    }
}

#[async_trait::async_trait]
impl MemorySystem for GraphMemorySystem {
    fn record_event(&mut self, event: MemoryEvent) {
        let _node = FastPath::process(
            &event.text,
            event.event_type,
            event.session_id,
            &mut self.store,
            &mut self.index,
            &self.fast_config,
        );

        // 更新 ContextBridge
        if let Some(narrative) = FastPath::synthesize_narrative(&self.store, &self.fast_config) {
            self.context_bridge.set(narrative);
        }
    }

    fn get_context(&self) -> String {
        self.context_bridge.get()
    }

    async fn tick(&mut self) {
        match self.slow_path.tick(&mut self.store, &mut self.index) {
            SlowPathResult::Done { events_processed: n, edges_added: m } => {
                tracing::debug!("[记忆] SlowPath tick: {n} events, {m} edges");
                if let Some(narrative) = FastPath::synthesize_narrative(&self.store, &self.fast_config) {
                    self.context_bridge.set(narrative);
                }
            }
            SlowPathResult::NoWork | SlowPathResult::SkippedNoLlm => {}
        }
    }

    fn save(&self) -> Result<(), DiskError> {
        match &self.disk {
            Some(disk) => {
                let events: Vec<EventNode> = self.store.all_events().into_iter().cloned().collect();
                let edges: Vec<Edge> = self.store.all_edges().into_iter().cloned().collect();
                disk.save_all(&events, &edges, self.index.export())
            }
            None => Ok(()),
        }
    }

    fn load(&mut self) -> Result<(), DiskError> {
        match &self.disk {
            Some(disk) => {
                disk.ensure_dir().map_err(|e| DiskError::Io(format!("创建记忆目录失败: {e}")))?;

                // 尝试恢复旧数据
                match disk.restore() {
                    Ok((store, index)) => {
                        self.store = store;
                        self.index = index;
                        if let Some(narrative) = FastPath::synthesize_narrative(&self.store, &self.fast_config) {
                            self.context_bridge.set(narrative);
                        }
                        let stats = self.store.stats();
                        tracing::info!(
                            "[记忆] 已加载: {} 事件, {} 边, {} 关键词",
                            stats.event_count, stats.edge_count, self.index.len()
                        );
                    }
                    Err(DiskError::Io(ref msg)) if msg.contains("events.jsonl") || msg.contains("No such file") => {
                        tracing::info!("[记忆] 无现有数据，从空白开始");
                    }
                    Err(e) => return Err(e),
                }
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn stats(&self) -> MemoryStats {
        let s = self.store.stats();
        MemoryStats {
            event_count: s.event_count,
            edge_count: s.edge_count,
            keyword_count: self.index.len(),
        }
    }

    fn clear(&mut self) {
        self.store.clear();
        self.index = InvertedIndex::new();
        self.context_bridge.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_stats() {
        let mut ms = GraphMemorySystem::new(None, None);
        assert_eq!(ms.stats().event_count, 0);

        ms.record_event(MemoryEvent {
            text: "今天天气很好".into(),
            event_type: EventType::UserMessage,
            session_id: None,
        });

        let stats = ms.stats();
        assert_eq!(stats.event_count, 1);
        assert!(stats.keyword_count > 0);
    }

    #[test]
    fn test_context_bridge_updated() {
        let mut ms = GraphMemorySystem::new(None, None);
        ms.record_event(MemoryEvent {
            text: "测试消息".into(),
            event_type: EventType::UserMessage,
            session_id: None,
        });

        let ctx = ms.get_context();
        // 应该包含叙事摘要
        assert!(ctx.contains("最近的") || ctx.contains("测试"));
    }

    #[tokio::test]
    async fn test_tick_no_crash() {
        let mut ms = GraphMemorySystem::new(None, None);
        ms.record_event(MemoryEvent {
            text: "事件1".into(),
            event_type: EventType::UserMessage,
            session_id: None,
        });
        ms.tick().await; // 即使没有 LLM 回调也不应崩溃
    }

    #[test]
    fn test_clear() {
        let mut ms = GraphMemorySystem::new(None, None);
        ms.record_event(MemoryEvent {
            text: "数据".into(),
            event_type: EventType::UserMessage,
            session_id: None,
        });
        assert_eq!(ms.stats().event_count, 1);
        ms.clear();
        assert_eq!(ms.stats().event_count, 0);
    }

    #[test]
    fn test_event_type_serde_roundtrip() {
        let cases = [
            (EventType::UserMessage, "\"user_message\""),
            (EventType::AssistantMessage, "\"assistant_message\""),
            (EventType::ToolUse, "\"tool_use\""),
            (EventType::Other("custom".into()), "\"custom\""),
        ];
        for (val, expected_json) in &cases {
            let json = serde_json::to_string(val).unwrap();
            assert_eq!(&json, expected_json);
            let back: EventType = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, val);
        }
    }

    #[test]
    fn test_relation_type_serde_roundtrip() {
        let cases = [
            (RelationType::Precedes, "\"precedes\""),
            (RelationType::LeadsTo, "\"leads_to\""),
            (RelationType::BecauseOf, "\"because_of\""),
        ];
        for (val, expected_json) in &cases {
            let json = serde_json::to_string(val).unwrap();
            assert_eq!(&json, expected_json);
            let back: RelationType = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, val);
        }
    }
}
