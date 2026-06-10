use crate::memory::{Edge, EventId, EventNode, MemoryStats, RelationType};
use std::collections::HashMap;

/// 内存图存储 — 事件节点 + 有向边
#[derive(Debug, Clone)]
pub struct GraphStore {
    events: HashMap<EventId, EventNode>,
    edges: HashMap<EventId, Vec<Edge>>,  // source → outgoing edges
}

impl GraphStore {
    pub fn new() -> Self {
        Self {
            events: HashMap::new(),
            edges: HashMap::new(),
        }
    }

    /// 添加事件节点
    pub fn add_event(&mut self, node: EventNode) {
        self.events.insert(node.id.clone(), node);
    }

    /// 获取事件节点
    pub fn get_event(&self, id: &str) -> Option<&EventNode> {
        self.events.get(id)
    }

    /// 添加有向边
    pub fn add_edge(&mut self, edge: Edge) {
        self.edges
            .entry(edge.source.clone())
            .or_default()
            .push(edge);
    }

    /// 获取某个节点的所有出边
    pub fn get_edges(&self, source: &str) -> Vec<&Edge> {
        self.edges.get(source).map_or(Vec::new(), |v| v.iter().collect())
    }

    /// 获取某个节点的出边（按关系类型过滤）
    pub fn get_edges_by_relation(&self, source: &str, rel: RelationType) -> Vec<&Edge> {
        self.edges
            .get(source)
            .map_or(Vec::new(), |v| {
                v.iter().filter(|e| e.relation == rel).collect()
            })
    }

    /// 最近 N 个事件（按时间戳降序）
    pub fn latest_events(&self, n: usize) -> Vec<&EventNode> {
        let mut events: Vec<&EventNode> = self.events.values().collect();
        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        events.truncate(n);
        events
    }

    /// 按时间范围查询事件 [start, end)
    pub fn events_in_range(&self, start: i64, end: i64) -> Vec<&EventNode> {
        self.events
            .values()
            .filter(|e| e.timestamp >= start && e.timestamp < end)
            .collect()
    }

    /// 按类型查询事件
    pub fn events_by_type(&self, event_type: &str) -> Vec<&EventNode> {
        self.events
            .values()
            .filter(|e| {
                use crate::memory::EventType;
                matches!(&e.event_type, EventType::Other(t) if t == event_type)
                    || std::mem::discriminant(&e.event_type)
                        == std::mem::discriminant(&EventType::Other(event_type.into()))
            })
            .collect()
    }

    /// 统计
    pub fn stats(&self) -> MemoryStats {
        MemoryStats {
            event_count: self.events.len(),
            edge_count: self.edges.values().map(|v| v.len()).sum(),
            keyword_count: 0,
        }
    }

    /// 批量导入事件（启动恢复用）
    pub fn import_events(&mut self, events: Vec<EventNode>) {
        for e in events {
            self.events.insert(e.id.clone(), e);
        }
    }

    /// 批量导入边（启动恢复用）
    pub fn import_edges(&mut self, edges: Vec<Edge>) {
        for e in edges {
            self.edges.entry(e.source.clone()).or_default().push(e);
        }
    }

    /// 获取所有事件
    pub fn all_events(&self) -> Vec<&EventNode> {
        self.events.values().collect()
    }

    /// 获取所有边
    pub fn all_edges(&self) -> Vec<&Edge> {
        self.edges.values().flatten().collect()
    }

    /// 清除所有数据
    pub fn clear(&mut self) {
        self.events.clear();
        self.edges.clear();
    }
}

impl Default for GraphStore {
    fn default() -> Self {
        Self::new()
    }
}
