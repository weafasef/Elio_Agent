use crate::memory::{
    Edge, EventNode, EventType, FastPathConfig, GraphStore, InvertedIndex,
    RelationType,
};
use std::collections::HashMap;
use tracing::debug;

/// FastPath — 无 LLM 快速记忆路径
///
/// 目标 <100ms，纯规则驱动:
/// 1. 创建事件节点
/// 2. 建立时间边 (PRECEDES)
/// 3. 提取关键词
/// 4. 搜索倒排索引，查找关联事件
/// 5. 4D 图遍历
/// 6. 合成叙事摘要 → 写入 ContextBridge
pub struct FastPath;

impl FastPath {
    /// 处理一条新输入，执行快速记忆路径
    pub fn process(
        text: &str,
        event_type: EventType,
        session_id: Option<String>,
        store: &mut GraphStore,
        index: &mut InvertedIndex,
        config: &FastPathConfig,
    ) -> EventNode {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        // 1. 提取关键词
        let keywords = Self::extract_keywords(text, config.max_keywords);

        // 2. 创建事件节点
        let event_id = format!("evt_{}_{}", now, fastrand::u64(..));
        let event = EventNode {
            id: event_id.clone(),
            text: text.to_string(),
            event_type,
            timestamp: now,
            keywords: keywords.clone(),
            session_id,
            entities: Vec::new(),
            metadata: HashMap::new(),
        };

        // 3. 添加到图
        let maybe_prev = store.latest_events(1).first().cloned().map(|e| e.id.clone());
        store.add_event(event.clone());
        index.index_event(&event_id, &keywords);

        // 4. 建立时间边
        if let Some(prev_id) = maybe_prev {
            store.add_edge(Edge {
                source: prev_id,
                target: event_id.clone(),
                relation: RelationType::Precedes,
                confidence: 1.0,
                timestamp: now,
                reason: Some("时间先后".into()),
            });
        }

        // 5. 搜索相关事件
        if !keywords.is_empty() {
            let related = index.search(&keywords);
            for rel_id in related {
                if rel_id != event_id {
                    store.add_edge(Edge {
                        source: event_id.clone(),
                        target: rel_id,
                        relation: RelationType::RelatedTo,
                        confidence: 0.9,
                        timestamp: now,
                        reason: Some("关键词匹配".into()),
                    });
                }
            }
        }

        debug!(
            "FastPath: 事件 {} 已记录，关键词: {:?}",
            event_id, keywords
        );

        event
    }

    /// 简单关键词提取 — 按分隔符切分 + 去停用词
    pub fn extract_keywords(text: &str, max: usize) -> Vec<String> {
        let stop_words = [
            "的", "了", "在", "是", "我", "有", "和", "就", "不", "人", "都", "一",
            "一个", "上", "也", "很", "到", "说", "要", "去", "你", "会", "着",
            "没有", "看", "好", "自己", "这",
            "a", "an", "the", "is", "are", "was", "were", "be", "been",
            "i", "you", "he", "she", "it", "we", "they",
            "in", "on", "at", "to", "for", "of", "with", "by",
            "and", "or", "but", "not", "so", "if", "as",
        ];

        let mut words: Vec<String> = text
            .split(|c: char| !c.is_alphanumeric() && c != '-')
            .filter(|w| w.len() >= 2)
            .map(|w| w.to_lowercase())
            .filter(|w| !stop_words.contains(&w.as_str()))
            .collect();

        words.sort_by(|a, b| {
            b.len().cmp(&a.len()) // 长词优先
        });
        words.dedup();
        words.truncate(max);
        words
    }

    /// 合成叙事摘要
    pub fn synthesize_narrative(
        store: &GraphStore,
        _config: &FastPathConfig,
    ) -> Option<String> {
        let recent = store.latest_events(5);
        if recent.is_empty() {
            return None;
        }

        let mut parts = Vec::new();
        for event in &recent {
            let text_preview = if event.text.len() > 60 {
                format!("{}...", &event.text[..60])
            } else {
                event.text.clone()
            };
            parts.push(format!("[{}] {}", event.id, text_preview));
        }

        Some(format!(
            "最近的 {} 个事件:\n{}",
            recent.len(),
            parts.join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_keywords() {
        let text = "今天天气很好，我想去公园散步和跑步";
        let keywords = FastPath::extract_keywords(text, 5);
        assert!(!keywords.is_empty());
        assert!(keywords.len() <= 5);
        for kw in &keywords {
            assert!(kw.len() >= 2);
        }
    }

    #[test]
    fn test_extract_keywords_english() {
        let text = "I want to write a Rust program for file processing";
        let keywords = FastPath::extract_keywords(text, 5);
        assert!(!keywords.is_empty());
        // "Rust" and "program" and "processing" should be extracted
        assert!(keywords.iter().any(|k| k == "rust" || k == "program" || k == "processing"));
    }

    #[test]
    fn test_process_creates_event() {
        let mut store = GraphStore::new();
        let mut index = InvertedIndex::new();
        let config = FastPathConfig::default();

        let event = FastPath::process(
            "Hello world test",
            EventType::UserMessage,
            None,
            &mut store,
            &mut index,
            &config,
        );

        assert_eq!(event.text, "Hello world test");
        assert!(store.get_event(&event.id).is_some());
        assert!(store.stats().event_count >= 1);
    }

    #[test]
    fn test_synthesize_narrative_empty() {
        let store = GraphStore::new();
        let config = FastPathConfig::default();
        assert!(FastPath::synthesize_narrative(&store, &config).is_none());
    }
}
