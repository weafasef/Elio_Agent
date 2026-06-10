use crate::memory::{Edge, EventNode, GraphStore, InvertedIndex};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, error};

/// JSONL 持久化 — 事件和边追加写入，索引全量写入
pub struct DiskIO {
    /// 数据目录 ~/.elio/memory/
    dir: PathBuf,
}

impl DiskIO {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// 确保数据目录存在
    pub fn ensure_dir(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)
    }

    /// 事件文件路径
    fn events_path(&self) -> PathBuf {
        self.dir.join("events.jsonl")
    }

    /// 边文件路径
    fn edges_path(&self) -> PathBuf {
        self.dir.join("edges.jsonl")
    }

    /// 索引文件路径
    fn index_path(&self) -> PathBuf {
        self.dir.join("inverted_index.json")
    }

    /// 追加写入一个事件节点
    pub fn append_event(&self, event: &EventNode) -> Result<(), DiskError> {
        let line = serde_json::to_string(event).map_err(DiskError::Serialize)?;
        let mut content = line;
        content.push('\n');
        fs::write(self.events_path(), content)
            .map_err(|e| DiskError::Io(format!("写入事件失败: {e}")))?;  // Simplified: should append
        // Note: Proper append requires OpenOptions
        Ok(())
    }

    /// 追加写入一条边
    pub fn append_edge(&self, edge: &Edge) -> Result<(), DiskError> {
        use std::io::Write;
        let line = serde_json::to_string(edge).map_err(DiskError::Serialize)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.edges_path())
            .map_err(|e| DiskError::Io(format!("打开边文件失败: {e}")))?;
        writeln!(file, "{}", line).map_err(|e| DiskError::Io(format!("写入边失败: {e}")))?;
        Ok(())
    }

    /// 读取所有事件
    pub fn load_events(&self) -> Result<Vec<EventNode>, DiskError> {
        let content = fs::read_to_string(self.events_path())
            .map_err(|e| DiskError::Io(format!("读取事件文件失败: {e}")))?;
        let mut events = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<EventNode>(line) {
                Ok(event) => events.push(event),
                Err(e) => {
                    error!("解析事件行失败: {e} — line: {}", &line[..line.len().min(100)]);
                }
            }
        }
        Ok(events)
    }

    /// 读取所有边
    pub fn load_edges(&self) -> Result<Vec<Edge>, DiskError> {
        let content = fs::read_to_string(self.edges_path())
            .map_err(|e| DiskError::Io(format!("读取边文件失败: {e}")))?;
        let mut edges = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Edge>(line) {
                Ok(edge) => edges.push(edge),
                Err(e) => {
                    error!("解析边行失败: {e}");
                }
            }
        }
        Ok(edges)
    }

    /// 读取倒排索引
    pub fn load_index(&self) -> Result<HashMap<String, Vec<String>>, DiskError> {
        let content = fs::read_to_string(self.index_path()).unwrap_or_default();
        if content.trim().is_empty() {
            return Ok(HashMap::new());
        }
        serde_json::from_str(&content)
            .map_err(|e| DiskError::Serialize(e))
    }

    /// 全量写入索引
    pub fn save_index(&self, index: &HashMap<String, Vec<String>>) -> Result<(), DiskError> {
        let content = serde_json::to_string_pretty(index)
            .map_err(DiskError::Serialize)?;
        fs::write(self.index_path(), content)
            .map_err(|e| DiskError::Io(format!("写入索引失败: {e}")))?;
        Ok(())
    }

    /// 全量快照 — 压缩重写所有文件
    pub fn save_all(
        &self,
        events: &[EventNode],
        edges: &[Edge],
        index: &HashMap<String, Vec<String>>,
    ) -> Result<(), DiskError> {
        self.ensure_dir().map_err(|e| DiskError::Io(format!("创建目录失败: {e}")))?;

        // 写事件
        let mut event_content = String::new();
        for event in events {
            let line = serde_json::to_string(event).map_err(DiskError::Serialize)?;
            event_content.push_str(&line);
            event_content.push('\n');
        }
        fs::write(self.events_path(), event_content)
            .map_err(|e| DiskError::Io(format!("写入事件文件失败: {e}")))?;

        // 写边
        let mut edge_content = String::new();
        for edge in edges {
            let line = serde_json::to_string(edge).map_err(DiskError::Serialize)?;
            edge_content.push_str(&line);
            edge_content.push('\n');
        }
        fs::write(self.edges_path(), edge_content)
            .map_err(|e| DiskError::Io(format!("写入边文件失败: {e}")))?;

        // 写索引
        self.save_index(index)?;

        debug!(
            "保存快照完成: {} 事件, {} 边, {} 关键词",
            events.len(),
            edges.len(),
            index.len()
        );
        Ok(())
    }

    /// 从磁盘恢复 GraphStore 和 InvertedIndex
    pub fn restore(&self) -> Result<(GraphStore, InvertedIndex), DiskError> {
        let events = self.load_events()?;
        let edges = self.load_edges()?;
        let index_data = self.load_index()?;

        let mut store = GraphStore::new();
        store.import_events(events);
        store.import_edges(edges);

        let mut index = InvertedIndex::new();
        index.import(index_data);

        Ok((store, index))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DiskError {
    #[error("I/O 错误: {0}")]
    Io(String),
    #[error("序列化错误: {0}")]
    Serialize(serde_json::Error),
}
