use std::collections::HashMap;

/// 倒排索引 — 关键词 → 事件 ID 列表
#[derive(Debug, Clone)]
pub struct InvertedIndex {
    /// keyword → list of event IDs
    index: HashMap<String, Vec<String>>,
}

impl InvertedIndex {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
        }
    }

    /// 为事件的所有关键词建立索引
    pub fn index_event(&mut self, event_id: &str, keywords: &[String]) {
        for kw in keywords {
            let kw_lower = kw.to_lowercase();
            self.index
                .entry(kw_lower)
                .or_default()
                .push(event_id.to_string());
        }
    }

    /// 根据关键词搜索事件 ID
    pub fn search(&self, keywords: &[String]) -> Vec<String> {
        let mut results: Vec<String> = Vec::new();
        for kw in keywords {
            let kw_lower = kw.to_lowercase();
            if let Some(ids) = self.index.get(&kw_lower) {
                results.extend(ids.iter().cloned());
            }
        }
        results.sort();
        results.dedup();
        results
    }

    /// 删除事件 ID 的所有索引引用
    pub fn remove_event(&mut self, event_id: &str) {
        self.index.retain(|_, ids| {
            ids.retain(|id| id != event_id);
            !ids.is_empty()
        });
    }

    /// 批量建立索引
    pub fn index_events(&mut self, events: &[(&str, &[String])]) {
        for (event_id, keywords) in events {
            self.index_event(event_id, keywords);
        }
    }

    /// 获取所有关键词
    pub fn all_keywords(&self) -> Vec<&str> {
        self.index.keys().map(|s| s.as_str()).collect()
    }

    /// 关键词数量
    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// 导出完整索引
    pub fn export(&self) -> &HashMap<String, Vec<String>> {
        &self.index
    }

    /// 从导出的 HashMap 恢复
    pub fn import(&mut self, index: HashMap<String, Vec<String>>) {
        self.index = index;
    }
}

impl Default for InvertedIndex {
    fn default() -> Self {
        Self::new()
    }
}
