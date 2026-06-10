//! 服务器配置

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryConfig {
    pub dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TtsConfig {
    pub enabled: bool,
    pub base_url: String,
    pub voice: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub llm: LlmConfig,
    pub memory: MemoryConfig,
    pub tts: TtsConfig,
}

impl Config {
    /// 从默认路径加载配置
    pub fn load() -> Result<Self, ConfigError> {
        let config_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config");
        let default_path = config_dir.join("default.toml");

        let content = std::fs::read_to_string(&default_path)
            .map_err(|e| ConfigError::ReadFailed(default_path.clone(), e))?;

        toml::from_str(&content).map_err(|e| ConfigError::Parse(e))
    }

    /// 从指定路径加载
    pub fn load_from(path: &PathBuf) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadFailed(path.clone(), e))?;
        toml::from_str(&content).map_err(|e| ConfigError::Parse(e))
    }

    /// 获取 API key（优先级：环境变量 > 配置文件）
    pub fn resolve_api_key(&self) -> String {
        if !self.llm.api_key.is_empty() {
            return self.llm.api_key.clone();
        }
        std::env::var("ANTHROPIC_AUTH_TOKEN")
            .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
            .unwrap_or_default()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("配置文件读取失败 {0:?}: {1}")]
    ReadFailed(PathBuf, std::io::Error),
    #[error("配置解析失败: {0}")]
    Parse(toml::de::Error),
}
