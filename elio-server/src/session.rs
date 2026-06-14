//! 会话管理 — 持有 MainLoop 实例

use elio_core::mainloop::{MainLoopConfig, MainLoop};
use elio_core::memory::MemorySystem;
use elio_core::llm::DeepSeekClient;
use elio_core::log::AuditLogger;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 单个会话
pub struct Session {
    pub inner: Mutex<MainLoop>,
}

impl Session {
    pub fn new(config: MainLoopConfig, memory: Box<dyn MemorySystem>, logger: Arc<AuditLogger>) -> Self {
        let api_key = std::env::var("ANTHROPIC_AUTH_TOKEN")
            .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
            .unwrap_or_default();

        let llm = Box::new(DeepSeekClient::new(
            api_key,
            config.llm_base_url.clone(),
            config.model.clone(),
        ));

        Session {
            inner: Mutex::new(MainLoop::new(config, llm, memory, logger)),
        }
    }
}

/// 会话管理器 — 目前只维护一个默认会话
pub struct SessionManager {
    sessions: Vec<Arc<Session>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    pub fn create_default(&mut self, config: MainLoopConfig, memory: Box<dyn MemorySystem>, logger: Arc<AuditLogger>) -> Arc<Session> {
        tracing::info!("[系统] 创建默认会话");
        let session = Arc::new(Session::new(config, memory, logger));
        self.sessions.push(session.clone());
        session
    }

    pub fn get_default(&self) -> Option<Arc<Session>> {
        self.sessions.first().cloned()
    }
}
