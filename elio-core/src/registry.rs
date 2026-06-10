use crate::tool::{Tool, ToolContext, ToolResult};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

/// 工具注册表 — 集中注册、查找和执行工具
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// 注册一个工具
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.name().to_string();
        info!("注册工具: {}", name);
        self.tools.insert(name, Arc::new(tool));
    }

    /// 通过名称获取工具
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    /// 执行工具
    pub async fn execute(
        &self,
        name: &str,
        input: serde_json::Value,
        ctx: ToolContext,
    ) -> ToolResult {
        match self.tools.get(name) {
            Some(tool) => {
                info!("执行工具: {} 输入: {}", name, serde_json::to_string(&input).unwrap_or_default());
                tool.execute(input, ctx).await
            }
            None => ToolResult::error(format!("未知工具: {name}")),
        }
    }

    /// 转换为 LLM tool_use 格式
    pub fn to_llm_tools(&self) -> Vec<crate::llm::ToolDef> {
        self.tools
            .values()
            .map(|tool| crate::llm::ToolDef {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
            })
            .collect()
    }

    /// 注册多个工具
    pub fn register_all(&mut self, tools: Vec<Box<dyn Tool>>) {
        for tool in tools {
            let name = tool.name().to_string();
            self.tools.insert(name, tool.into());
        }
    }

    /// 已注册的工具数量
    pub fn count(&self) -> usize {
        self.tools.len()
    }

    /// 列出所有工具名
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{json_schema, string_schema, Tool, ToolContext, ToolResult};

    struct EchoTool;

    #[async_trait::async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "回显输入"
        }
        fn input_schema(&self) -> serde_json::Value {
            json_schema(
                "object",
                vec![("text", string_schema("要回显的文本"))],
                vec!["text"],
            )
        }
        async fn execute(&self, input: serde_json::Value, _ctx: ToolContext) -> ToolResult {
            let text = input.get("text").and_then(|v| v.as_str()).unwrap_or("");
            ToolResult::text(text)
        }
    }

    #[tokio::test]
    async fn test_register_and_execute() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);

        assert_eq!(registry.count(), 1);
        assert_eq!(registry.list(), vec!["echo"]);

        let result = registry
            .execute(
                "echo",
                serde_json::json!({"text": "Hello"}),
                ToolContext {
                    cwd: std::path::PathBuf::from("/"),
                    session_id: "test".into(),
                    user_message: None,
                },
            )
            .await;

        assert!(!result.is_error);
        assert!(format!("{:?}", result.content).contains("Hello"));
    }

    #[test]
    fn test_unknown_tool() {
        let registry = ToolRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }
}
