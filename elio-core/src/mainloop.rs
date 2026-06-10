//! MainLoop — Elio 的自主感知-决策-行动循环

use crate::llm::{ChatRequest, ContentBlock, LlmClient, Message, MessageRole};
use crate::memory::{MemoryEvent, MemorySystem};
use crate::registry::ToolRegistry;
use crate::tool::ToolContext;
use crate::worldview::{PerceptSource, WorldviewBuffer};
use tracing::{debug, info};

/// MainLoop 配置
#[derive(Debug, Clone)]
pub struct MainLoopConfig {
    /// 心跳间隔（秒）
    pub heartbeat_secs: u64,
    /// LLM model name
    pub model: String,
    /// LLM API base URL
    pub llm_base_url: String,
    /// Max tokens per response
    pub max_tokens: u32,
    /// System prompt (完整文本，由 PromptManager 组装)
    pub system_prompt: String,
}

impl Default for MainLoopConfig {
    fn default() -> Self {
        Self {
            heartbeat_secs: 30,
            model: "deepseek-v4-flash".into(),
            llm_base_url: "https://api.deepseek.com/anthropic".into(),
            max_tokens: 4096,
            system_prompt: String::new(),
        }
    }
}

/// MainLoop 状态
#[derive(Debug, Clone, PartialEq)]
pub enum LoopState {
    /// 空闲，等待感知
    Idle,
    /// 正在 LLM 调用
    Thinking,
    /// LLM 正在执行工具
    ExecutingTool,
    /// 等待用户输入
    WaitingForUser,
}

/// 对话消息历史
#[derive(Debug, Clone)]
pub struct Conversation {
    pub messages: Vec<Message>,
    pub max_turns: usize,
}

impl Conversation {
    pub fn new(max_turns: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_turns,
        }
    }

    pub fn add_user_message(&mut self, text: &str) {
        self.messages.push(Message {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: text.into() }],
        });
        self.trim();
    }

    pub fn add_assistant_text(&mut self, text: &str) {
        self.messages.push(Message {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
        });
    }

    pub fn add_tool_result(&mut self, tool_use_id: String, content: String, is_error: bool) {
        self.messages.push(Message {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult { tool_use_id, content, is_error }],
        });
    }

    fn trim(&mut self) {
        while self.messages.len() > self.max_turns * 2 {
            self.messages.remove(0);
        }
    }
}

/// MainLoop 每步的结果
pub enum StepResult {
    /// 无工作可做
    Idle,
    /// LLM 回复文本
    Response(String),
    /// LLM 调用工具 (name, input, tool_call_id)
    ToolCall(String, serde_json::Value, String),
    /// 发生错误
    Error(String),
}

/// Elio 自主 MainLoop
pub struct MainLoop {
    /// 当前状态
    pub state: LoopState,
    config: MainLoopConfig,
    llm: Box<dyn LlmClient>,
    /// 世界感知缓冲
    pub worldview: WorldviewBuffer,
    /// 记忆系统（只依赖 MemorySystem trait）
    pub memory: Box<dyn MemorySystem>,
    /// 工具注册表
    pub tools: ToolRegistry,
    /// 对话历史
    pub conversation: Conversation,
}

impl MainLoop {
    pub fn new(config: MainLoopConfig, llm: Box<dyn LlmClient>, memory: Box<dyn MemorySystem>) -> Self {
        Self {
            state: LoopState::Idle,
            config,
            llm,
            worldview: WorldviewBuffer::default(),
            memory,
            tools: ToolRegistry::new(),
            conversation: Conversation::new(50),
        }
    }

    /// 处理用户消息
    pub fn on_user_message(&mut self, text: &str) {
        self.conversation.add_user_message(text);
        self.memory.record_event(MemoryEvent {
            text: text.into(),
            event_type: crate::memory::EventType::UserMessage,
            session_id: Some(self.config.model.clone()),
        });
        self.worldview.push(text, PerceptSource::User);
        self.state = LoopState::Thinking;
    }

    /// 执行一步 MainLoop tick
    pub async fn step(&mut self) -> StepResult {
        if self.conversation.messages.is_empty() {
            self.state = LoopState::Idle;
            return StepResult::Idle;
        }

        self.worldview.commit_slice();

        // 构建系统提示词（世界观注入 — 时间/运行时长/外部感知 + 记忆上下文）
        let worldview_text = self.worldview.build_worldview();
        let mut system_prompt = self.config.system_prompt.clone();
        if !worldview_text.is_empty() {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&worldview_text);
        }
        let mem_ctx = self.memory.get_context();
        if !mem_ctx.is_empty() {
            system_prompt.push_str("\n\n## 记忆上下文\n");
            system_prompt.push_str(&mem_ctx);
        }

        self.state = LoopState::Thinking;

        let request = ChatRequest {
            model: self.config.model.clone(),
            system: system_prompt,
            messages: self.conversation.messages.clone(),
            tools: self.tools.to_llm_tools(),
            max_tokens: self.config.max_tokens,
        };

        let response = match self.llm.chat(request).await {
            Ok(r) => r,
            Err(e) => {
                self.state = LoopState::Idle;
                return StepResult::Error(e.to_string());
            }
        };

        for block in &response.content {
            match block {
                ContentBlock::Text { text } => {
                    self.conversation.add_assistant_text(text);
                    self.memory.record_event(MemoryEvent {
                        text: text.clone(),
                        event_type: crate::memory::EventType::AssistantMessage,
                        session_id: None,
                    });
                    info!("Elio 回复: {:.100}", text);
                    self.state = LoopState::Idle;
                    return StepResult::Response(text.clone());
                }
                ContentBlock::ToolUse { name, input, id } => {
                    self.state = LoopState::ExecutingTool;
                    return StepResult::ToolCall(name.clone(), input.clone(), id.clone());
                }
                _ => {}
            }
        }

        self.state = LoopState::Idle;
        StepResult::Idle
    }

    /// 执行工具并将结果送回 LLM
    pub async fn execute_tool(&mut self, name: &str, input: serde_json::Value, tool_call_id: &str) -> StepResult {
        let ctx = ToolContext {
            cwd: std::env::current_dir().unwrap_or_default(),
            session_id: "elio".into(),
            user_message: None,
        };

        let result = self.tools.execute(name, input, ctx).await;
        let result_text = result
            .content
            .iter()
            .map(|b| match b {
                crate::tool::ToolContentBlock::Text { text } => text.clone(),
                crate::tool::ToolContentBlock::Image { .. } => "[图片]".into(),
            })
            .collect::<Vec<_>>()
            .join("\n");

        self.conversation.add_tool_result(tool_call_id.to_string(), result_text.clone(), result.is_error);

        let status = if result.is_error { "失败" } else { "成功" };
        self.memory.record_event(MemoryEvent {
            text: format!("工具 {name} 执行{status}: {result_text}"),
            event_type: crate::memory::EventType::ToolResult,
            session_id: None,
        });

        Box::pin(self.step()).await
    }

    /// 定时记忆维护 tick
    pub async fn memory_tick(&mut self) {
        debug!("执行记忆 tick");
        self.memory.tick().await;
    }

    /// 获取记忆统计
    pub fn memory_stats(&self) -> crate::memory::MemoryStats {
        self.memory.stats()
    }

    /// 加载系统提示词
    pub fn set_system_prompt(&mut self, prompt: String) {
        self.config.system_prompt = prompt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{ChatResponse, LlmClient, LlmError, Usage};
    use crate::memory::{GraphMemorySystem, MemorySystem};
    use std::sync::{Arc, Mutex};

    struct MockLlm;
    #[async_trait::async_trait]
    impl LlmClient for MockLlm {
        async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, LlmError> {
            Ok(ChatResponse {
                content: vec![ContentBlock::Text { text: "Mock response".into() }],
                usage: Some(Usage { input_tokens: 10, output_tokens: 5 }),
                stop_reason: Some("end_turn".into()),
                model: "mock".into(),
            })
        }
        async fn chat_stream(
            &self, _request: ChatRequest, _on_event: Box<dyn Fn(crate::llm::StreamEvent) + Send>,
        ) -> Result<ChatResponse, LlmError> { self.chat(_request).await }
    }

    #[tokio::test]
    async fn test_on_user_message() {
        let config = MainLoopConfig::default();
        let llm = Box::new(MockLlm);
        let memory = Box::new(GraphMemorySystem::new(None, None));
        let mut loop_ = MainLoop::new(config, llm, memory);

        loop_.on_user_message("你好 Elio");
        assert_eq!(loop_.state, LoopState::Thinking);

        match loop_.step().await {
            StepResult::Response(text) => assert_eq!(text, "Mock response"),
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn test_conversation_trim() {
        let mut conv = Conversation::new(2);
        for i in 0..5 { conv.add_user_message(&format!("msg {i}")); }
        assert!(conv.messages.len() <= 4);
    }
}
