//! LLM API 客户端
//!
//! 定义统一的 LLM 调用接口，提供 DeepSeek（兼容 Anthropic 协议）实现。

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// LLM 消息角色
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
}

/// 消息内容块
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentBlock {
    Text { text: String },
    ToolUse {
        name: String,
        input: serde_json::Value,
        id: String,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

/// LLM 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
}

/// 工具定义（用于 LLM tool_use）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// LLM 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub max_tokens: u32,
}

/// LLM 响应用法统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// LLM 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: Vec<ContentBlock>,
    pub usage: Option<Usage>,
    pub stop_reason: Option<String>,
    pub model: String,
}

/// LLM 流式事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: u32,
        delta: ContentDelta,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: u32 },
    #[serde(rename = "message_delta")]
    MessageDelta { delta: MessageDelta },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDelta {
    pub stop_reason: Option<String>,
}

/// LLM API 客户端 trait
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    /// 非流式调用
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;
    /// 流式调用
    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_event: Box<dyn Fn(StreamEvent) + Send>,
    ) -> Result<ChatResponse, LlmError>;
}

/// DeepSeek LLM 客户端（兼容 Anthropic Messages API）
pub struct DeepSeekClient {
    api_key: String,
    base_url: String,
    #[allow(dead_code)]
    model: String,
    http_client: reqwest::Client,
}

impl DeepSeekClient {
    pub fn new(api_key: String, base_url: String, model: String) -> Self {
        Self {
            api_key,
            base_url,
            model,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("创建 HTTP 客户端失败"),
        }
    }

    /// 构建 Anthropic Messages API 格式的请求体
    fn build_anthropic_body(&self, request: &ChatRequest) -> serde_json::Value {
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "user",
                };
                let content: Vec<serde_json::Value> = msg
                    .content
                    .iter()
                    .map(|block| match block {
                        ContentBlock::Text { text } => {
                            serde_json::json!({"type": "text", "text": text})
                        }
                        ContentBlock::ToolUse { name, input, id } => {
                            serde_json::json!({
                                "type": "tool_use",
                                "name": name,
                                "input": input,
                                "id": id
                            })
                        }
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } => {
                            serde_json::json!({
                                "type": "tool_result",
                                "tool_use_id": tool_use_id,
                                "content": content,
                                "is_error": is_error
                            })
                        }
                    })
                    .collect();

                serde_json::json!({"role": role, "content": content})
            })
            .collect();

        let tools: Vec<serde_json::Value> = request
            .tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": request.model,
            "system": request.system,
            "messages": messages,
            "max_tokens": request.max_tokens,
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::Value::Array(tools);
        }

        body
    }
}

#[async_trait::async_trait]
impl LlmClient for DeepSeekClient {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let body = self.build_anthropic_body(&request);
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));

        let resp = self
            .http_client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::HttpError(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(LlmError::ApiError(status.as_u16(), body_text));
        }

        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LlmError::ParseError(e.to_string()))?;

        Self::parse_response(value)
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
        _on_event: Box<dyn Fn(StreamEvent) + Send>,
    ) -> Result<ChatResponse, LlmError> {
        // 简化实现：目前直接调用非流式 API
        self.chat(request).await
    }
}

impl DeepSeekClient {
    fn parse_response(value: serde_json::Value) -> Result<ChatResponse, LlmError> {
        let content_blocks = value
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| LlmError::ParseError("缺少 content".into()))?;

        let mut content = Vec::new();
        for block in content_blocks {
            let block_type = block
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("text");

            match block_type {
                "text" => {
                    let text = block
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    content.push(ContentBlock::Text { text });
                }
                "tool_use" => {
                    let name = block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let input = block.get("input").cloned().unwrap_or(serde_json::Value::Null);
                    let id = block
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or("")
                        .to_string();
                    content.push(ContentBlock::ToolUse { name, input, id });
                }
                _ => {}
            }
        }

        let usage = value.get("usage").map(|u| Usage {
            input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        });

        let stop_reason = value
            .get("stop_reason")
            .and_then(|s| s.as_str())
            .map(String::from);

        let model = value
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(ChatResponse {
            content,
            usage,
            stop_reason,
            model,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP 错误: {0}")]
    HttpError(String),
    #[error("API 错误 ({0}): {1}")]
    ApiError(u16, String),
    #[error("解析错误: {0}")]
    ParseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_anthropic_body() {
        let client = DeepSeekClient::new(
            "sk-test".into(),
            "https://api.deepseek.com".into(),
            "deepseek-v4-flash".into(),
        );

        let request = ChatRequest {
            model: "deepseek-v4-flash".into(),
            system: "你是 Elio".into(),
            messages: vec![Message {
                role: MessageRole::User,
                content: vec![ContentBlock::Text {
                    text: "你好".into(),
                }],
            }],
            tools: vec![],
            max_tokens: 4096,
        };

        let body = client.build_anthropic_body(&request);
        assert_eq!(body["model"], "deepseek-v4-flash");
        assert_eq!(body["system"], "你是 Elio");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"][0]["text"], "你好");
    }

    #[test]
    fn test_parse_response() {
        let json = serde_json::json!({
            "content": [
                {"type": "text", "text": "你好！我是 Elio。"},
                {"type": "tool_use", "name": "Bash", "input": {"command": "ls"}, "id": "tu_123"}
            ],
            "usage": {"input_tokens": 100, "output_tokens": 50},
            "stop_reason": "end_turn",
            "model": "deepseek-v4-flash"
        });

        let resp = DeepSeekClient::parse_response(json).unwrap();
        assert_eq!(resp.content.len(), 2);
        assert_eq!(resp.usage.unwrap().input_tokens, 100);
        assert_eq!(resp.stop_reason.unwrap(), "end_turn");
    }
}
