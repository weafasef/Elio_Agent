use crate::memory::{EventNode, GraphStore, RelationType};
use std::collections::HashSet;

/// 4D 图遍历结果
#[derive(Debug, Clone)]
pub struct TraversalResult {
    /// 起始事件 ID
    pub root_id: String,
    /// 遍历深度跳数
    pub hops: usize,
    /// 发现的路径
    pub paths: Vec<TraversalPath>,
}

/// 一条遍历路径
#[derive(Debug, Clone)]
pub struct TraversalPath {
    pub dimension: TraversalDimension,
    pub events: Vec<String>,  // event IDs in path
    pub total_confidence: f64,
}

/// 遍历维度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalDimension {
    /// 时间维度 — preceds/succeeds
    Temporal,
    /// 语义维度 — related_to/similar_to/part_of
    Semantic,
    /// 因果维度 — leads_to/because_of/enables/prevents/response_to
    Causal,
    /// 实体维度 — references
    Entity,
}

/// 4D 图遍历 — 从根节点出发，沿各维度探索
pub fn traverse(
    store: &GraphStore,
    root_id: &str,
    max_hops: usize,
) -> Vec<TraversalResult> {
    let _root = match store.get_event(root_id) {
        Some(e) => e,
        None => return Vec::new(),
    };

    // 各维度独立遍历
    let dimensions = [
        TraversalDimension::Temporal,
        TraversalDimension::Semantic,
        TraversalDimension::Causal,
        TraversalDimension::Entity,
    ];

    let mut results = Vec::new();

    for &dim in &dimensions {
        let mut visited = HashSet::new();
        let mut paths = Vec::new();

        visited.insert(root_id.to_string());
        dfs_dimension(
            store,
            root_id,
            dim,
            &mut visited,
            &mut vec![root_id.to_string()],
            &mut paths,
            0,
            max_hops,
        );

        if !paths.is_empty() {
            results.push(TraversalResult {
                root_id: root_id.to_string(),
                hops: max_hops,
                paths,
            });
        }
    }

    results
}

/// 按维度深度遍历
fn dfs_dimension(
    store: &GraphStore,
    current_id: &str,
    dim: TraversalDimension,
    visited: &mut HashSet<String>,
    current_path: &mut Vec<String>,
    paths: &mut Vec<TraversalPath>,
    depth: usize,
    max_hops: usize,
) {
    if depth >= max_hops {
        // 记录路径
        if current_path.len() > 1 {
            let conf = compute_path_confidence(store, current_path, dim);
            paths.push(TraversalPath {
                dimension: dim,
                events: current_path.clone(),
                total_confidence: conf,
            });
        }
        return;
    }

    let edges = store.get_edges(current_id);
    for edge in edges {
        if !matches_dimension(&edge.relation, dim) {
            continue;
        }
        if visited.contains(&edge.target) {
            continue;
        }

        visited.insert(edge.target.clone());
        current_path.push(edge.target.clone());

        dfs_dimension(
            store,
            &edge.target,
            dim,
            visited,
            current_path,
            paths,
            depth + 1,
            max_hops,
        );

        current_path.pop();
        visited.remove(&edge.target);
    }

    // 路径终点
    if current_path.len() > 1 && depth > 0 {
        let conf = compute_path_confidence(store, current_path, dim);
        paths.push(TraversalPath {
            dimension: dim,
            events: current_path.clone(),
            total_confidence: conf,
        });
    }
}

/// 判断边类型是否属于指定维度
fn matches_dimension(rel: &RelationType, dim: TraversalDimension) -> bool {
    match dim {
        TraversalDimension::Temporal => matches!(rel, RelationType::Precedes),
        TraversalDimension::Semantic => matches!(
            rel,
            RelationType::RelatedTo | RelationType::SimilarTo | RelationType::PartOf
        ),
        TraversalDimension::Causal => matches!(
            rel,
            RelationType::LeadsTo
                | RelationType::BecauseOf
                | RelationType::Enables
                | RelationType::Prevents
                | RelationType::ResponseTo
        ),
        TraversalDimension::Entity => matches!(rel, RelationType::References),
    }
}

/// 计算路径置信度（路径上各边置信度的乘积）
fn compute_path_confidence(
    store: &GraphStore,
    path: &[String],
    _dim: TraversalDimension,
) -> f64 {
    let mut conf = 1.0;
    for window in path.windows(2) {
        if let Some(edges) = store.get_edges_by_relation(
            &window[0],
            RelationType::RelatedTo, // simplified — check all relation types
        ).first() {
            conf *= edges.confidence;
        }
    }
    conf.max(0.0).min(1.0)
}

/// 按时间戳排序事件列表（降序）
#[allow(dead_code)]
pub fn sort_by_timestamp(events: &mut [&EventNode]) {
    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{EventNode, EventType};

    fn make_event(id: &str, ts: i64) -> EventNode {
        EventNode {
            id: id.to_string(),
            text: format!("event {id}"),
            event_type: EventType::UserMessage,
            timestamp: ts,
            keywords: Vec::new(),
            session_id: None,
            entities: Vec::new(),
            metadata: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_traversal_no_results_on_empty() {
        let store = GraphStore::new();
        let results = traverse(&store, "nonexistent", 2);
        assert!(results.is_empty());
    }

    #[test]
    fn test_sort_by_timestamp() {
        let e1 = make_event("a", 100);
        let e2 = make_event("b", 200);
        let e3 = make_event("c", 150);
        let mut events = vec![&e1, &e2, &e3];
        sort_by_timestamp(&mut events);
        assert_eq!(events[0].id, "b");
        assert_eq!(events[1].id, "c");
        assert_eq!(events[2].id, "a");
    }
}
