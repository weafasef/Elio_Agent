/// ContextBridge — 模块级上下文传递
///
/// 里代理 (MemoryAgent/SlowPath) 写入合成记忆上下文，
/// 表代理 (Elio MainLoop) 读取并注入系统提示词。
///
/// 零成本：简单的模块级字符串交换。
use std::sync::{Arc, RwLock};

/// 线程安全的上下文桥
#[derive(Debug, Clone)]
pub struct ContextBridge {
    inner: Arc<RwLock<String>>,
}

impl ContextBridge {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(String::new())),
        }
    }

    /// 写入记忆上下文
    pub fn set(&self, ctx: String) {
        if let Ok(mut guard) = self.inner.write() {
            *guard = ctx;
        }
    }

    /// 读取记忆上下文
    pub fn get(&self) -> String {
        self.inner.read().map(|g| g.clone()).unwrap_or_default()
    }

    /// 检查是否有上下文
    pub fn has_context(&self) -> bool {
        self.inner.read().map(|g| !g.is_empty()).unwrap_or(false)
    }

    /// 清除上下文
    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.write() {
            guard.clear();
        }
    }
}

impl Default for ContextBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get() {
        let bridge = ContextBridge::new();
        assert!(!bridge.has_context());
        bridge.set("测试记忆上下文".into());
        assert!(bridge.has_context());
        assert_eq!(bridge.get(), "测试记忆上下文");
    }

    #[test]
    fn test_clear() {
        let bridge = ContextBridge::new();
        bridge.set("some context".into());
        bridge.clear();
        assert_eq!(bridge.get(), "");
    }
}
