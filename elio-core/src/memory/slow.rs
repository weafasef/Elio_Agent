use crate::memory::{
    Edge, EventNode, GraphStore, InvertedIndex, SlowPathConfig,
};
use tracing::{debug, error, info};

/// LLM 回调类型 — SlowPath 通过依赖注入调用 LLM
pub type LlmCallback = Box<dyn Fn(&str) -> String + Send + Sync>;

/// SlowPath — DeepSeek 驱动的推理性记忆路径
///
/// 每 30 秒运行一次:
/// 1. 批量处理未处理事件
/// 2. 叙事补全 (Narrative Enrichment)
/// 3. 因果推断 (Causal Inference)
/// 4. 语义链接 (Semantic Linking)
/// 5. 高置信度边加入图
pub struct SlowPath {
    config: SlowPathConfig,
    call_llm: Option<LlmCallback>,
    /// 已处理的事件 ID 集合
    processed: std::collections::HashSet<String>,
}

impl SlowPath {
    pub fn new(config: SlowPathConfig) -> Self {
        Self {
            config,
            call_llm: None,
            processed: std::collections::HashSet::new(),
        }
    }

    /// 注册 LLM 回调
    pub fn set_llm_callback(&mut self, cb: LlmCallback) {
        self.call_llm = Some(cb);
    }

    /// 获取尚未处理的事件（排重）
    fn pending_events<'a>(&self, store: &'a GraphStore) -> Vec<&'a EventNode> {
        store
            .all_events()
            .into_iter()
            .filter(|e| !self.processed.contains(&e.id))
            .take(self.config.batch_size)
            .collect()
    }

    /// 执行一次 SlowPath tick
    pub fn tick(&mut self, store: &mut GraphStore, _index: &mut InvertedIndex) -> SlowPathResult {
        let pending = self.pending_events(store);
        if pending.is_empty() {
            return SlowPathResult::NoWork;
        }

        debug!("SlowPath: 处理 {} 个事件", pending.len());

        if self.call_llm.is_none() {
            // 无 LLM 回调时直接标记处理过
            for event in &pending {
                self.processed.insert(event.id.clone());
            }
            return SlowPathResult::SkippedNoLlm;
        }

        let llm = self.call_llm.as_ref().unwrap();
        let mut new_edges = Vec::new();
        let mut processed_count = 0;

        for event in &pending {
            // 1. 叙事补全
            let narrative_prompt = Self::build_narrative_prompt(event);
            let narrative = self.call_with_retry(llm, &narrative_prompt);

            if let Some(ref text) = narrative {
                let entities = Self::extract_entities(text);
                if !entities.is_empty() {
                    // 创建实体引用边
                    for entity in entities {
                        new_edges.push(Edge {
                            source: event.id.clone(),
                            target: format!("entity:{}", entity),
                            relation: crate::memory::RelationType::References,
                            confidence: 0.8,
                            timestamp: event.timestamp,
                            reason: Some(format!("叙事提取实体: {entity}")),
                        });
                    }
                }
            }

            // 2. 因果推断
            let causality_prompt = Self::build_causality_prompt(event, store);
            let causality = self.call_with_retry(llm, &causality_prompt);

            if let Some(ref text) = causality {
                // 解析 LLM 返回的因果边
                let inferred = Self::parse_causal_edges(event, text);
                for mut edge in inferred {
                    if edge.confidence >= self.config.confidence_threshold {
                        edge.timestamp = event.timestamp;
                        new_edges.push(edge);
                    }
                }
            }

            self.processed.insert(event.id.clone());
            processed_count += 1;

            // 避免单次 tick 处理太多
            if processed_count >= self.config.batch_size {
                break;
            }
        }

        // 加入新边
        for edge in &new_edges {
            store.add_edge(edge.clone());
        }

        info!(
            "SlowPath: 处理 {} 事件, 添加 {} 边",
            processed_count,
            new_edges.len()
        );

        SlowPathResult::Done {
            events_processed: processed_count,
            edges_added: new_edges.len(),
        }
    }

    /// 带重试的 LLM 调用
    fn call_with_retry(&self, llm: &LlmCallback, prompt: &str) -> Option<String> {
        let mut last_error = None;
        for attempt in 0..self.config.max_retries {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                llm(prompt)
            })) {
                Ok(result) => {
                    let trimmed = result.trim();
                    if !trimmed.is_empty() && trimmed != "null" {
                        return Some(trimmed.to_string());
                    }
                }
                Err(e) => {
                    last_error = Some(format!("panic: {:?}", e));
                    error!("LLM 调用 panic (第 {} 次): {:?}", attempt + 1, e);
                }
            }
            if attempt + 1 < self.config.max_retries {
                std::thread::sleep(std::time::Duration::from_millis(1000));
            }
        }
        error!("LLM 调用全部失败: {:?}", last_error);
        None
    }

    /// 构建叙事补全提示词
    fn build_narrative_prompt(event: &EventNode) -> String {
        let event_type_str = format!("{:?}", event.event_type);
        format!(
            r#"你是一个记忆叙事助手。请为以下事件撰写一段简洁的叙事描述（中文，50字以内），
并提取其中的关键实体（人物、地点、事物等）。

事件类型: {event_type_str}
事件文本: {text}
时间: {ts}"#,
            event_type_str = event_type_str,
            text = event.text,
            ts = event.timestamp
        )
    }

    /// 构建因果推断提示词
    fn build_causality_prompt(event: &EventNode, store: &GraphStore) -> String {
        // 找到最近的相关事件
        let recent: Vec<&EventNode> = store.latest_events(5);
        let recent_text: Vec<String> = recent
            .iter()
            .take(3)
            .map(|e| format!("[{}] {}", e.id, e.text))
            .collect();

        format!(
            r#"分析以下事件之间是否存在因果关系。
请以 JSON 格式输出：{{"edges": [{{"target": "事件ID", "relation": "leads_to|because_of|enables|prevents|response_to", "confidence": 0.0-1.0, "reason": "原因"}}]}}

当前事件: [{id}] {text}

最近的相关事件:
{recent}"#,
            id = event.id,
            text = event.text,
            recent = recent_text.join("\n")
        )
    }

    /// 解析 LLM 返回的因果边（简单 JSON 解析）
    fn parse_causal_edges(source: &EventNode, llm_response: &str) -> Vec<Edge> {
        let mut edges = Vec::new();
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(llm_response) {
            if let Some(edge_list) = val.get("edges").and_then(|v| v.as_array()) {
                for item in edge_list {
                    let target = item.get("target").and_then(|v| v.as_str()).unwrap_or("");
                    let relation = item.get("relation").and_then(|v| v.as_str()).unwrap_or("related_to");
                    let confidence = item.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);
                    let reason = item.get("reason").and_then(|v| v.as_str()).map(String::from);

                    let relation_type = match relation {
                        "leads_to" => crate::memory::RelationType::LeadsTo,
                        "because_of" => crate::memory::RelationType::BecauseOf,
                        "enables" => crate::memory::RelationType::Enables,
                        "prevents" => crate::memory::RelationType::Prevents,
                        "response_to" => crate::memory::RelationType::ResponseTo,
                        _ => crate::memory::RelationType::RelatedTo,
                    };

                    if !target.is_empty() && target != source.id {
                        edges.push(Edge {
                            source: source.id.clone(),
                            target: target.to_string(),
                            relation: relation_type,
                            confidence,
                            timestamp: source.timestamp,
                            reason,
                        });
                    }
                }
            }
        }
        edges
    }

    /// 从文本提取实体名（简单规则）
    fn extract_entities(text: &str) -> Vec<String> {
        // 简单实现：找引号包裹或特殊标记的词
        let mut entities = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("- ") || line.starts_with("* ") {
                let entity = line.trim_start_matches("- ")
                    .trim_start_matches("* ")
                    .trim();
                if entity.len() >= 2 && entity.len() <= 20 {
                    entities.push(entity.to_string());
                }
            }
        }
        entities
    }

    /// 重置已处理集合（用于测试或恢复）
    pub fn reset(&mut self) {
        self.processed.clear();
    }
}

/// SlowPath 单次 tick 结果
pub enum SlowPathResult {
    NoWork,
    SkippedNoLlm,
    Done {
        events_processed: usize,
        edges_added: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{EventType, FastPathConfig, FastPath};

    #[test]
    fn test_no_work_when_empty() {
        let mut store = GraphStore::new();
        let mut index = InvertedIndex::new();
        let config = SlowPathConfig::default();
        let mut sp = SlowPath::new(config);

        match sp.tick(&mut store, &mut index) {
            SlowPathResult::NoWork => {}
            _ => panic!("expected NoWork"),
        }
    }

    #[test]
    fn test_skipped_no_llm() {
        let mut store = GraphStore::new();
        let mut index = InvertedIndex::new();
        let fp_config = FastPathConfig::default();

        FastPath::process("test event", EventType::UserMessage, None, &mut store, &mut index, &fp_config);

        let sp_config = SlowPathConfig::default();
        let mut sp = SlowPath::new(sp_config);

        match sp.tick(&mut store, &mut index) {
            SlowPathResult::SkippedNoLlm => {}
            _ => panic!("expected SkippedNoLlm"),
        }
    }
}
