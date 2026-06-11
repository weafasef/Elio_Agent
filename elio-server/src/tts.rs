//! TTS 服务 — 连接 GPT-SoVITS 进行流式语音合成
//!
//! 参考 Elio_Agent v1 (TypeScript) 的 ttsService.ts 实现:
//! - 情感 → 参考音频映射
//! - 流式合成：每句话一个 WAV 片段
//! - 参考音频目录按 emotion 扫描
//!
//! GPT-SoVITS api_v2.py 需要在 9880 端口运行:
//!   cd D:\VS_python\TTS\GPT-SoVITS-1007-cu124
//!   runtime\python.exe api_v2.py -a 127.0.0.1 -p 9880 -c GPT_SoVITS/configs/tts_infer.yaml

use crate::config::TtsConfig;
use reqwest::Client;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, warn};

// ── Emotion label mapping ─────────────────────────────────────────────────

/// 中文情感标签 → 英文 key
const CN_EMOTION_MAP: &[(&str, &str)] = &[
    ("开心", "happy"),
    ("难过", "sad"),
    ("吃惊", "surprise"),
    ("恐惧", "fear"),
    ("厌恶", "disgust"),
    ("生气", "angry"),
    ("中立", "neutral"),
    ("默认", "default"),
];

/// 情感回退链
const EMOTION_FALLBACKS: &[&str] = &["default", "neutral", "happy"];

// ── Types ─────────────────────────────────────────────────────────────────

/// 参考音频
#[derive(Debug, Clone)]
struct RefAudio {
    path: PathBuf,
    text: String,
}

/// 语音合成结果（非流式）
#[derive(Debug)]
pub struct TtsResult {
    pub audio_bytes: Vec<u8>,
}

/// 语音标签解析结果
#[derive(Debug)]
pub struct SpeechBlocks {
    pub ja: String,
    pub zh: String,
    pub emotion: String,
}

// ── TTS Service ───────────────────────────────────────────────────────────

/// TTS 语音合成服务
pub struct TtsService {
    client: Client,
    config: TtsConfig,
    /// 情感 → 参考音频 映射（初始化时扫描）
    ref_audios: Arc<Mutex<HashMap<String, RefAudio>>>,
}

impl TtsService {
    /// 创建 TTS 服务，扫描参考音频目录
    pub fn new(config: TtsConfig) -> Self {
        let ref_audios = Arc::new(Mutex::new(Self::scan_ref_audios(&config)));

        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .no_proxy()
                .build()
                .expect("构建 HTTP 客户端失败"),
            config,
            ref_audios,
        }
    }

    /// 扫描参考音频目录，构建 emotion → RefAudio 映射
    fn scan_ref_audios(config: &TtsConfig) -> HashMap<String, RefAudio> {
        let mut map = HashMap::new();

        let dir = match &config.ref_audio_dir {
            Some(d) if !d.is_empty() => PathBuf::from(d),
            _ => {
                debug!("[TTS] 未设置 ref_audio_dir，跳过参考音频扫描");
                return map;
            }
        };

        if !dir.is_dir() {
            warn!("[TTS] 参考音频目录不存在: {:?}", dir);
            return map;
        }

        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("[TTS] 读取参考音频目录失败: {e}");
                return map;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "wav") {
                continue;
            }

            let filename = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            // 解析 【情绪】文本.wav
            // 注意：【】都是多字节 UTF-8 字符，不能用 [1..] 按字节切片
            let (emotion, text) = if let (Some(start), Some(end)) =
                (filename.find('【'), filename.find('】'))
            {
                // 【 3 字节，】 3 字节
                let emo_part = &filename[start + 3..end];
                let text_part = &filename[end + 3..];
                let eng = CN_EMOTION_MAP.iter()
                    .find(|(cn, _)| *cn == emo_part)
                    .map(|(_, eng)| *eng)
                    .unwrap_or(emo_part);
                (eng.to_string(), text_part.to_string())
            } else {
                // 文件名不含情绪标签，用 default
                ("default".to_string(), filename.to_string())
            };

            debug!("[TTS] 参考音频: {emotion} → {:?} ({})", path, text);
            map.insert(emotion, RefAudio { path, text });
        }

        debug!("[TTS] 扫描到 {} 个参考音频", map.len());
        map
    }

    /// 检查 TTS 服务是否可用
    pub async fn is_available(&self) -> bool {
        if !self.config.enabled || self.config.base_url.is_empty() {
            return false;
        }
        // 尝试连接基础 URL
        let url = format!("{}/", self.config.base_url.trim_end_matches('/'));
        match self.client.get(&url).send().await {
            Ok(resp) => resp.status().is_success() || resp.status().is_client_error(),
            Err(e) => {
                warn!("[TTS] 服务不可用: {e}");
                false
            }
        }
    }

    /// 非流式合成（简单场景，返回完整 WAV）
    pub async fn synthesize(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let url = format!("{}/tts", self.config.base_url.trim_end_matches('/'));

        // 获取参考音频
        let ref_audio = self.get_ref_audio("default").await;

        let mut body = serde_json::json!({
            "text": text,
            "text_lang": self.config.lang,
            "media_type": "wav",
            "streaming_mode": false,
        });

        if let Some(ref ra) = ref_audio {
            body["ref_audio_path"] = serde_json::json!(ra.path.to_string_lossy().replace('\\', "/"));
            body["prompt_text"] = serde_json::json!(ra.text);
            body["prompt_lang"] = serde_json::json!(self.config.lang);
        }

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| TtsError::RequestFailed(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            return Err(TtsError::RequestFailed(format!("HTTP {status}: {err_text}")));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| TtsError::RequestFailed(e.to_string()))?
            .to_vec();

        debug!("[TTS] 非流式合成: {} 字节音频", bytes.len());
        Ok(bytes)
    }

    /// 流式合成 — 每句话通过回调返回独立 WAV
    ///
    /// GPT-SoVITS 流式响应格式：
    ///   1. 先发 44 字节 WAV header
    ///   2. 每句话一个 raw PCM chunk
    /// 每段 PCM 需要拼上 WAV header 才形成有效 WAV 文件
    pub async fn synthesize_stream<F>(
        &self,
        text: &str,
        emotion: &str,
        on_chunk: F,
    ) -> Result<usize, TtsError>
    where
        F: Fn(Vec<u8>, usize) + Send + 'static,
    {
        let url = format!("{}/tts", self.config.base_url.trim_end_matches('/'));

        let ref_audio = self.get_ref_audio(emotion).await;

        let mut body = serde_json::json!({
            "text": text,
            "text_lang": self.config.lang,
            "media_type": "wav",
            "streaming_mode": self.config.streaming,
        });

        if let Some(ref ra) = ref_audio {
            body["ref_audio_path"] = serde_json::json!(ra.path.to_string_lossy().replace('\\', "/"));
            body["prompt_text"] = serde_json::json!(ra.text);
            body["prompt_lang"] = serde_json::json!(self.config.lang);
        }

        let mut resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| TtsError::RequestFailed(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            return Err(TtsError::RequestFailed(format!("HTTP {status}: {err_text}")));
        }

        // ── 流式读取 ──────────────────────────────────────────────────────
        let mut wav_header: Option<Vec<u8>> = None;
        let mut chunk_index = 0;

        // 非流式模式：直接读取完整 body，作为单个 chunk 回调
        if !self.config.streaming {
            let all_bytes = resp
                .bytes()
                .await
                .map_err(|e| TtsError::RequestFailed(e.to_string()))?;
            debug!("[TTS] 非流式响应: {} 字节", all_bytes.len());
            on_chunk(all_bytes.to_vec(), 0);
            return Ok(1);
        }

        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| TtsError::RequestFailed(e.to_string()))?
        {

            // 第一块：包含 WAV header（44 字节） + 可能的首句 PCM
            if wav_header.is_none() {
                if chunk.len() < 44 {
                    continue; // 等待足够数据
                }
                let header = chunk[..44].to_vec();
                let pcm = chunk[44..].to_vec();
                wav_header = Some(header);

                if !pcm.is_empty() {
                    let wav = Self::build_wav(&wav_header.as_ref().unwrap(), &pcm);
                    on_chunk(wav, chunk_index);
                    chunk_index += 1;
                }
                continue;
            }

            // 后续块：纯 PCM
            if !chunk.is_empty() {
                let wav = Self::build_wav(wav_header.as_ref().unwrap(), &chunk);
                on_chunk(wav, chunk_index);
                chunk_index += 1;
            }
        }

        debug!("[TTS] 流式合成完成: {chunk_index} 个分片");
        Ok(chunk_index)
    }

    /// 用共享 WAV header 和 PCM 构建完整 WAV 文件字节
    fn build_wav(header: &[u8], pcm: &[u8]) -> Vec<u8> {
        let mut wav = header.to_vec();
        // 更新 RIFF chunk size (offset 4): 36 + pcm.len()
        let riff_size = 36u32 + pcm.len() as u32;
        wav[4..8].copy_from_slice(&riff_size.to_le_bytes());
        // 更新 data chunk size (offset 40)
        let data_size = pcm.len() as u32;
        wav[40..44].copy_from_slice(&data_size.to_le_bytes());
        wav.extend_from_slice(pcm);
        wav
    }

    /// 获取情感对应的参考音频（带 fallback 链）
    async fn get_ref_audio(&self, emotion: &str) -> Option<RefAudio> {
        let map = self.ref_audios.lock().await;

        // 直接匹配
        if let Some(ra) = map.get(emotion) {
            return Some(ra.clone());
        }

        // Fallback 链
        for fallback in EMOTION_FALLBACKS {
            if let Some(ra) = map.get(*fallback) {
                return Some(ra.clone());
            }
        }

        None
    }
}

// ── Speech parsing ────────────────────────────────────────────────────────

/// 从 LLM 回复文本中解析语音标签
///
/// 支持的标签：
/// - `<ja>...</ja>` — 日文语音文本（合成用）
/// - `<zh>...</zh>` — 中文字幕
/// - `<emotion>...</emotion>` — 情感（happy/sad/neutral...）
///
/// 没有标签时检测是否含日文字符，有则 fallback 整段文本。
pub fn parse_speech_blocks(text: &str) -> Option<SpeechBlocks> {
    // 提取标签内容
    let ja_blocks: Vec<&str> = text
        .split("<ja>")
        .skip(1)
        .filter_map(|s| s.split("</ja>").next())
        .map(|s| s.trim())
        .collect();

    let zh_blocks: Vec<&str> = text
        .split("<zh>")
        .skip(1)
        .filter_map(|s| s.split("</zh>").next())
        .map(|s| s.trim())
        .collect();

    let emotion = text
        .split("<emotion>")
        .nth(1)
        .and_then(|s| s.split("</emotion>").next())
        .map(|s| s.trim().to_lowercase());

    let ja = ja_blocks.join("");
    let zh = zh_blocks.join("");

    if !ja.is_empty() {
        return Some(SpeechBlocks {
            ja,
            zh,
            emotion: emotion.unwrap_or_else(|| "happy".into()),
        });
    }

    // 没有 <ja> 标签：检查是否有日文字符作为 fallback
    let has_japanese = text.chars().any(|c| {
        matches!(c,
            '\u{3040}'..='\u{309F}' | // 平假名
            '\u{30A0}'..='\u{30FF}' | // 片假名
            '\u{FF66}'..='\u{FF9D}'   // 半角片假名
        )
    });

    if has_japanese {
        // 去掉 HTML 标签和工具调用标记
        let cleaned = text
            .replace(&['[', ']', '<', '>', '/', '\''][..], "")
            .trim()
            .to_string();
        // 只保留日文字符、标点和空格
        let cleaned: String = cleaned.chars().filter(|c| {
            c.is_whitespace()
                || matches!(c,
                    '\u{3040}'..='\u{309F}' | // 平假名
                    '\u{30A0}'..='\u{30FF}' | // 片假名
                    '\u{FF66}'..='\u{FF9D}' | // 半角片假名
                    '\u{3000}'..='\u{303F}' | // 日文标点
                    '\u{FF00}'..='\u{FFEF}' | // 全角字母/标点
                    '\u{4E00}'..='\u{9FFF}' | // CJK 汉字（用于混合文本）
                    '.' | ',' | '?' | '!' | '…' | '—' | '~'
                )
        }).collect();
        if cleaned.trim().is_empty() {
            return None;
        }
        return Some(SpeechBlocks {
            ja: cleaned,
            zh: String::new(),
            emotion: emotion.unwrap_or_else(|| "happy".into()),
        });
    }

    None
}

// ── Errors ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("TTS 服务不可用")]
    NotAvailable,
    #[error("请求失败: {0}")]
    RequestFailed(String),
}
