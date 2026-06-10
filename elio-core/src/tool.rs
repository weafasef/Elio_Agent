//! Tool trait — 所有内置工具实现此接口

use serde::Serialize;
use std::fmt;
use std::path::PathBuf;

/// 工具执行上下文
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// 工作目录
    pub cwd: PathBuf,
    /// 会话 ID
    pub session_id: String,
    /// 用户回复（通过 ask_user 收集）
    pub user_message: Option<String>,
}

/// 内容块
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ToolContentBlock {
    Text { text: String },
    Image { source: ImageSource },
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageSource {
    pub data: String,
    pub media_type: String,
}

/// 工具执行结果
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: Vec<ToolContentBlock>,
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContentBlock::Text { text: text.into() }],
            is_error: false,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContentBlock::Text { text: text.into() }],
            is_error: true,
        }
    }
}

/// JSON Schema 构建器
pub fn json_schema(
    type_: &str,
    properties: Vec<(&str, serde_json::Value)>,
    required: Vec<&str>,
) -> serde_json::Value {
    let props: serde_json::Map<String, serde_json::Value> = properties
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();

    serde_json::json!({
        "type": type_,
        "properties": props,
        "required": required
    })
}

pub fn string_schema(description: &str) -> serde_json::Value {
    serde_json::json!({"type": "string", "description": description})
}

pub fn number_schema(description: &str) -> serde_json::Value {
    serde_json::json!({"type": "number", "description": description})
}

pub fn boolean_schema(description: &str) -> serde_json::Value {
    serde_json::json!({"type": "boolean", "description": description})
}

pub fn array_schema(items: serde_json::Value, description: &str) -> serde_json::Value {
    serde_json::json!({"type": "array", "items": items, "description": description})
}

/// 工具接口
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// 工具名称
    fn name(&self) -> &str;
    /// 工具描述
    fn description(&self) -> &str;
    /// JSON Schema 格式的输入定义
    fn input_schema(&self) -> serde_json::Value;
    /// 执行工具
    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: ToolContext,
    ) -> ToolResult;
}

/// 工具调试包装
pub struct ToolDebug<T: Tool>(pub T);

impl<T: Tool> fmt::Debug for ToolDebug<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tool")
            .field("name", &self.0.name())
            .field("description", &self.0.description())
            .finish()
    }
}
