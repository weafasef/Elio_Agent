//! Elio 自主记忆系统
//!
//! 架构：
//!   MemorySystem trait ← MainLoop 只依赖这个
//!       └── GraphMemorySystem (默认实现)
//!              ├── GraphStore (内存图)
//!              ├── InvertedIndex (倒排索引)
//!              ├── FastPath (<100ms 无 LLM)
//!              ├── SlowPath (LLM 叙事推演)
//!              ├── DiskIO (JSONL 持久化)
//!              └── ContextBridge (唯一输出 → 系统提示词)

mod types;
mod graph;
mod index;
mod disk;
mod traversal;
mod fast;
mod slow;
mod bridge;
pub mod system;
pub mod prompts;

pub use types::*;
pub use graph::GraphStore;
pub use index::InvertedIndex;
pub use disk::DiskIO;
pub use disk::DiskError;
pub use traversal::traverse;
pub use fast::FastPath;
pub use slow::SlowPath;
pub use bridge::ContextBridge;
pub use system::{MemorySystem, GraphMemorySystem, MemoryEvent, MemoryStats};
