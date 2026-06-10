use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 时间戳（毫秒级 Unix 时间戳）
pub type Timestamp = i64;

/// 事件节点 ID
pub type EventId = String;

/// 事件类型分类
///
/// 序列化为 snake_case 字符串，兼容 TS .elio/memory/ 的 JSONL 格式
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventType {
    /// 用户消息
    UserMessage,
    /// Elio 回复
    AssistantMessage,
    /// 工具调用
    ToolUse,
    /// 工具结果
    ToolResult,
    /// 系统事件
    System,
    /// 世界观注入
    Worldview,
    /// 情感快照
    Emotion,
    /// 梦境
    Dream,
    /// 记忆操作
    Memory,
    /// 其他（兼容未知类型）
    Other(String),
}

impl Default for EventType {
    fn default() -> Self {
        EventType::System
    }
}

impl Serialize for EventType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let str = match self {
            EventType::UserMessage => "user_message",
            EventType::AssistantMessage => "assistant_message",
            EventType::ToolUse => "tool_use",
            EventType::ToolResult => "tool_result",
            EventType::System => "system",
            EventType::Worldview => "worldview",
            EventType::Emotion => "emotion",
            EventType::Dream => "dream",
            EventType::Memory => "memory",
            EventType::Other(t) => t.as_str(),
        };
        s.serialize_str(str)
    }
}

impl<'de> Deserialize<'de> for EventType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "user_message" => EventType::UserMessage,
            "assistant_message" => EventType::AssistantMessage,
            "tool_use" => EventType::ToolUse,
            "tool_result" => EventType::ToolResult,
            "system" => EventType::System,
            "worldview" => EventType::Worldview,
            "emotion" => EventType::Emotion,
            "dream" => EventType::Dream,
            "memory" => EventType::Memory,
            other => EventType::Other(other.to_string()),
        })
    }
}

/// 关系类型（边类型）
///
/// 序列化为 snake_case 字符串，兼容 TS 格式
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelationType {
    /// 时间先后
    Precedes,
    /// 因果 — A 导致 B
    LeadsTo,
    /// 因果 — B 因为 A
    BecauseOf,
    /// 因果 — A 使 B 成为可能
    Enables,
    /// 因果 — A 阻止了 B
    Prevents,
    /// 因果 — A 是对 B 的回应
    ResponseTo,
    /// 语义相关
    RelatedTo,
    /// 语义相似
    SimilarTo,
    /// 语义 — A 是 B 的一部分
    PartOf,
    /// 实体 — A 引用 B
    References,
}

impl Default for RelationType {
    fn default() -> Self {
        RelationType::RelatedTo
    }
}

impl Serialize for RelationType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let str = match self {
            RelationType::Precedes => "precedes",
            RelationType::LeadsTo => "leads_to",
            RelationType::BecauseOf => "because_of",
            RelationType::Enables => "enables",
            RelationType::Prevents => "prevents",
            RelationType::ResponseTo => "response_to",
            RelationType::RelatedTo => "related_to",
            RelationType::SimilarTo => "similar_to",
            RelationType::PartOf => "part_of",
            RelationType::References => "references",
        };
        s.serialize_str(str)
    }
}

impl<'de> Deserialize<'de> for RelationType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let lower = s.to_lowercase();
        Ok(match lower.as_str() {
            "precedes" | "temporal" => RelationType::Precedes,
            "leads_to" => RelationType::LeadsTo,
            "because_of" => RelationType::BecauseOf,
            "enables" => RelationType::Enables,
            "prevents" => RelationType::Prevents,
            "response_to" => RelationType::ResponseTo,
            "related_to" | "semantic" => RelationType::RelatedTo,
            "similar_to" => RelationType::SimilarTo,
            "part_of" => RelationType::PartOf,
            "references" | "entity" => RelationType::References,
            "mentioned_in" | "contains" => {
                // 旧数据中的非标准关系类型，忽略
                return Ok(RelationType::RelatedTo);
            }
            other => {
                tracing::warn!("未知关系类型: {other}，视为 related_to");
                RelationType::RelatedTo
            }
        })
    }
}

/// 事件节点 — 记忆的基本单位
///
/// 兼容 TS 旧数据格式（rawText, eventType, sessionId）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventNode {
    /// 唯一 ID
    pub id: EventId,
    /// 事件文本内容（兼容旧名 rawText）
    #[serde(alias = "rawText")]
    pub text: String,
    /// 事件类型（兼容旧名 eventType，旧数据可能没有此字段）
    #[serde(default, alias = "eventType")]
    pub event_type: EventType,
    /// 时间戳
    pub timestamp: Timestamp,
    /// 提取的关键词列表
    #[serde(default)]
    pub keywords: Vec<String>,
    /// 关联的会话 ID（兼容旧名 sessionId）
    #[serde(default, alias = "sessionId")]
    pub session_id: Option<String>,
    /// 关联的实体名
    #[serde(default)]
    pub entities: Vec<String>,
    /// 额外元数据
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// 图边 — 连接两个事件节点
///
/// 兼容 TS 旧数据格式（sourceId, targetId, subtype, weight）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// 源节点 ID（兼容旧名 sourceId）
    #[serde(alias = "sourceId")]
    pub source: EventId,
    /// 目标节点 ID（兼容旧名 targetId）
    #[serde(alias = "targetId")]
    pub target: EventId,
    /// 关系类型（旧数据用 subtype 字段）
    #[serde(default, alias = "subtype")]
    pub relation: RelationType,
    /// 置信度 [0.0, 1.0]（旧数据用 weight）
    #[serde(default = "default_confidence", alias = "weight")]
    pub confidence: f64,
    /// 边创建时间
    #[serde(default)]
    pub timestamp: Timestamp,
    /// 推理依据文本
    #[serde(default)]
    pub reason: Option<String>,
}

fn default_confidence() -> f64 { 1.0 }

/// FastPath 配置
#[derive(Debug, Clone)]
pub struct FastPathConfig {
    /// 关键词提取词数上限
    pub max_keywords: usize,
    /// 最近切片保留数
    pub recent_slices: usize,
    /// 遍历跳数
    pub traversal_hops: usize,
    /// 最大返回事件数
    pub max_events: usize,
}

impl Default for FastPathConfig {
    fn default() -> Self {
        Self {
            max_keywords: 10,
            recent_slices: 7,
            traversal_hops: 2,
            max_events: 20,
        }
    }
}

/// SlowPath 配置
#[derive(Debug, Clone)]
pub struct SlowPathConfig {
    /// 批处理间隔（秒）
    pub interval_secs: u64,
    /// 每批最大事件数
    pub batch_size: usize,
    /// 推理置信度阈值
    pub confidence_threshold: f64,
    /// 最大重试次数
    pub max_retries: u32,
}

impl Default for SlowPathConfig {
    fn default() -> Self {
        Self {
            interval_secs: 30,
            batch_size: 10,
            confidence_threshold: 0.7,
            max_retries: 3,
        }
    }
}
