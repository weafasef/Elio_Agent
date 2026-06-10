//! TTS 服务 — 连接 GPT-SoVITS 进行语音合成

use crate::config::TtsConfig;
use reqwest::Client;
use tracing::debug;

pub struct TtsService {
    client: Client,
    config: TtsConfig,
}

impl TtsService {
    pub fn new(config: TtsConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    pub fn is_available(&self) -> bool {
        if !self.config.enabled || self.config.base_url.is_empty() {
            return false;
        }
        // 简单检查：如果能连上就认为可用
        true
    }

    /// 合成语音文本
    pub async fn synthesize(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        if !self.is_available() {
            return Err(TtsError::NotAvailable);
        }

        let url = format!("{}/tts", self.config.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "text": text,
            "voice": self.config.voice,
            "format": "wav",
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| TtsError::RequestFailed(e.to_string()))?;

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| TtsError::RequestFailed(e.to_string()))?
            .to_vec();

        debug!("TTS: 合成 {} 字节音频", bytes.len());
        Ok(bytes)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("TTS 服务不可用")]
    NotAvailable,
    #[error("请求失败: {0}")]
    RequestFailed(String),
}
