//! Elio Server — HTTP + WebSocket 入口

mod config;
mod routes;
mod session;
mod tts;
mod ws;

use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use config::Config;
use elio_core::memory::{GraphMemorySystem, MemoryEvent, MemorySystem};
use elio_core::prompt::PromptManager;
use elio_core::tool::{ToolContentBlock, ToolContext};
use elio_core::worldview::PerceptSource;
use session::SessionManager;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

use base64::Engine;

/// 共享应用状态
pub struct AppState {
    session_mgr: SessionManager,
    config: Config,
    /// 广播 Elio 回复到所有 WebSocket 连接
    pub response_tx: tokio::sync::broadcast::Sender<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "elio_server=info,elio_core=info".into()),
        )
        .init();

    // 加载配置
    let config = Config::load().map_err(|e| anyhow::anyhow!("配置加载失败: {e}"))?;
    info!(
        "Elio Server v{} 启动中...",
        env!("CARGO_PKG_VERSION")
    );

    // 解析 API key
    let api_key = config.resolve_api_key();
    if api_key.is_empty() {
        anyhow::bail!("未设置 API key，请通过环境变量 ANTHROPIC_AUTH_TOKEN 或配置文件设置");
    }
    // 需要 unsafe 因为 Rust 2024 edition 中 set_var 是 unsafe 的
    unsafe { std::env::set_var("ANTHROPIC_AUTH_TOKEN", &api_key); }

    // 初始化记忆系统
    let memory_dir = resolve_memory_dir(&config.memory.dir);
    info!("记忆目录: {:?}", memory_dir);

    let mut graph_memory = GraphMemorySystem::new(Some(memory_dir.clone()), None);
    if let Err(e) = graph_memory.load() {
        tracing::warn!("加载记忆失败（将使用空白记忆）: {e}");
    }
    let mem_stats = graph_memory.stats();
    info!("记忆状态: {} 事件, {} 边", mem_stats.event_count, mem_stats.edge_count);

    // 加载提示词
    let prompts_dir = resolve_prompts_dir();
    info!("提示词目录: {:?}", prompts_dir);
    let mut prompt_mgr = PromptManager::new(prompts_dir);
    if let Err(e) = prompt_mgr.load_all() {
        anyhow::bail!("提示词加载失败: {e}");
    }
    info!("已加载 {} 个提示词文件", prompt_mgr.loaded_count());
    if let Err(missing) = prompt_mgr.check_required() {
        tracing::warn!("缺少提示词文件: {:?}", missing);
    }
    let system_prompt = prompt_mgr.build_system_prompt(None);

    // 初始化日志
    let log_dir = resolve_logs_dir();
    info!("日志目录: {:?}", log_dir);
    let logger = Arc::new(elio_core::log::AuditLogger::new(log_dir));

    // 创建会话
    let mut session_mgr = SessionManager::new();
    let mainloop_config = elio_core::mainloop::MainLoopConfig {
        model: config.llm.model.clone(),
        llm_base_url: config.llm.base_url.clone(),
        max_tokens: 4096,
        system_prompt,
        ..Default::default()
    };
    session_mgr.create_default(mainloop_config, Box::new(graph_memory), logger);

    // 提取地址信息
    let addr = format!("{}:{}", config.server.host, config.server.port);

    // 创建 broadcast channel 用于推送 Elio 回复到 WS 客户端
    let (response_tx, _) = tokio::sync::broadcast::channel::<String>(64);

    // 构建 axum 路由
    let app_state = Arc::new(AppState {
        session_mgr,
        config,
        response_tx: response_tx.clone(),
    });

    // 初始化 TTS 服务（按配置启用）
    // 注：config 已在 AppState 中，需要从 app_state 读取 tts 配置
    let tts_service: Option<Arc<tts::TtsService>> = {
        let tts_cfg = &app_state.config.tts;
        if tts_cfg.enabled {
            let svc = Arc::new(tts::TtsService::new(tts_cfg.clone()));
            info!("TTS 服务已加载: {}", tts_cfg.base_url);

            // 启动时检查 GPT-SoVITS 连接
            let check = svc.is_available().await;
            info!("GPT-SoVITS 连接检查: {}", if check { "✅ 可用" } else { "❌ 不可用" });

            Some(svc)
        } else {
            info!("TTS 服务已禁用（enabled = false）");
            None
        }
    };

    // 连续步进循环：每 30s 至少调一次 step_stream，启动后立即首次调用
    let heartbeat_state = Arc::clone(&app_state);
    let heartbeat_tx = response_tx.clone();
    let heartbeat_tts = tts_service.clone();
    tokio::spawn(async move {
        // 首次 step 立即触发，后续每次间隔至少 30s
        loop {
            let step_start = std::time::Instant::now();
            tracing::debug!("elio step loop");

            let session = match heartbeat_state.session_mgr.get_default() {
                Some(s) => s,
                None => continue,
            };

            // 1. 定时器 tick（推 Timer 感知 + system tick 到对话）
            let mut guard = session.inner.lock().await;
            guard.on_timer_tick();
            drop(guard);

            // 2. 单次 step（流式 — 逐 delta 实时广播文本到客户端）
            // 用块作用域控制 guard 生命周期，ToolCall 分支提取所需数据后释放锁
            // Phase 2 标记：</en> 提前 TTS 是否已启动（跨 guard 块使用）
            let tts_started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let step_result = {
                let mut guard = session.inner.lock().await;

                // 先发 content_start 标记文本开始
                let _ = heartbeat_tx.send(
                    serde_json::json!({"type": "content_start", "blockType": "text"}).to_string()
                );

                // 流式 step — on_delta 中实时广播每个文本片段
                // 用 ThinkStripper 去掉 <think>...</think> 块内容
                let stripper = std::sync::Arc::new(std::sync::Mutex::new(ThinkStripper::new()));
                let s = stripper.clone();
                let tx = heartbeat_tx.clone();

                // Phase 2: 检测 </en> 提前启动 TTS（不等完整回复）
                let tts_flag = tts_started.clone();
                let full_raw = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
                let raw_buf = full_raw.clone();
                let tts_svc = heartbeat_tts.clone();
                let tx_tts = heartbeat_tx.clone();

                let result = guard.step_stream(move |delta_text| {
                    // Phase 1: 显示文本（剥离 think 块）
                    let clean = s.lock().unwrap().feed(delta_text);
                    if !clean.is_empty() {
                        let _ = tx.send(
                            serde_json::json!({
                                "type": "content_delta",
                                "delta": {"text": clean}
                            }).to_string()
                        );
                    }

                    // Phase 2: 累积原始文本，检测 </en> 提前 TTS
                    let mut raw = raw_buf.lock().unwrap();
                    raw.push_str(delta_text);
                    if !tts_flag.load(std::sync::atomic::Ordering::Relaxed)
                        && raw.contains("</en>")
                    {
                        if let Some(en_text) = raw.split("<en>").nth(1)
                            .and_then(|s| s.split("</en>").next())
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                        {
                            tts_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                            tracing::info!("[TTS] 检测到 </en>，提前合成: 「{:.60}」", en_text);

                            if let Some(tts) = tts_svc.as_ref() {
                                let tts = tts.clone();
                                let tx = tx_tts.clone();
                                let en_for_msg = en_text.clone();
                                // <emotion> 可能还没到，用默认值
                                let emotion = "happy".to_string();
                                // <zh> 可能也没到，用空
                                let zh: String = String::new();
                                tokio::spawn(async move {
                                    let result = tts.synthesize_stream(
                                        &en_text,
                                        &emotion,
                                        move |wav_bytes, idx| {
                                            let b64 = base64::engine::general_purpose::STANDARD
                                                .encode(&wav_bytes);
                                            let msg = serde_json::json!({
                                                "type": "tts_chunk",
                                                "data": b64,
                                                "chunk_index": idx,
                                                "format": "wav",
                                                "text": en_for_msg,
                                                "subtitle": zh,
                                            });
                                            let _ = tx.send(msg.to_string());
                                        },
                                    ).await;
                                    match result {
                                        Ok(n) => tracing::info!("[TTS] 提前合成完成: {n} 个分片"),
                                        Err(e) => tracing::warn!("[TTS] 提前合成失败: {e}"),
                                    }
                                });
                            }
                        }
                    }
                }).await;

                match result {
                    elio_core::mainloop::StepResult::ToolCall(ref name, ..) => {
                        guard.worldview.push(
                            format!("工具 {name} 已提交，等待结果..."),
                            PerceptSource::ToolResult,
                        );
                    }
                    _ => {}
                }

                result
            }; // guard 在此释放

            match step_result {
                elio_core::mainloop::StepResult::Response(text) => {
                    tracing::info!("Elio 回复: {:.100}", text);

                    // 解析并打印各区块
                    let think_blocks: Vec<&str> = text
                        .split("<think>").skip(1)
                        .filter_map(|s| s.split("</think>").next())
                        .map(|s| s.trim())
                        .collect();
                    let think_text = think_blocks.join("\n---\n");
                    if !think_text.is_empty() {
                        tracing::info!("<think>\n{}\n</think>", think_text);
                    }
                    let tts_text = strip_think_tags(&text);
                    if let Some(speech) = tts::parse_speech_blocks(&tts_text) {
                        tracing::info!("<en>\n{}\n</en>", speech.en);
                        if !speech.zh.is_empty() {
                            tracing::info!("<zh>\n{}\n</zh>", speech.zh);
                        }
                    }

                    // 标记文本流结束
                    let _ = heartbeat_tx.send(
                        serde_json::json!({
                            "type": "message_complete",
                            "usage": {"input_tokens": 0, "output_tokens": 0}
                        }).to_string()
                    );

                    // ── TTS 语音合成（后台异步，不阻塞心跳） ──────────────
                    // Phase 2 已合成第一段 → 去掉第一段再合成剩余段落
                    // Phase 2 没跑          → 正常合成全部段落
                    let tts_text = strip_think_tags(&text);
                    if let Some(ref tts) = heartbeat_tts {
                        let parse_text = if tts_started.load(std::sync::atomic::Ordering::Relaxed) {
                            strip_first_speech_block(&tts_text)
                        } else {
                            tts_text.clone()
                        };
                        if !parse_text.trim().is_empty() {
                            if let Some(speech) = tts::parse_speech_blocks(&parse_text) {
                            let tts = tts.clone();
                            let tx = heartbeat_tx.clone();
                            let en_text = speech.en;
                            let en_for_msg = en_text.clone();
                            let zh_text = speech.zh;
                            let emotion = speech.emotion;
                            tokio::spawn(async move {
                                tracing::info!("TTS 合成: emotion={emotion}, en=「{:.60}」", en_text);
                                let result = tts.synthesize_stream(
                                    &en_text,
                                    &emotion,
                                    move |wav_bytes, idx| {
                                        let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);
                                        tracing::debug!("[TTS] chunk {idx}: WAV {} 字节, base64 {} 字节",
                                            wav_bytes.len(), b64.len());
                                        let msg = serde_json::json!({
                                            "type": "tts_chunk",
                                            "data": b64,
                                            "chunk_index": idx,
                                            "format": "wav",
                                            "text": en_for_msg,
                                            "subtitle": zh_text,
                                        });
                                        let _ = tx.send(msg.to_string());
                                    },
                                ).await;
                                match result {
                                    Ok(n) => tracing::info!("TTS 完成: {n} 个分片"),
                                    Err(e) => tracing::warn!("TTS 合成失败: {e}"),
                                }
                            });
                            }
                        }
                    }
                }
                elio_core::mainloop::StepResult::ToolCall(name, input, id) => {
                    tracing::info!("Elio 调用工具: {name}");
                    let tool = {
                        let guard = session.inner.lock().await;
                        guard.tools.get(&name).cloned()
                    };
                    if let Some(tool) = tool {
                        let ctx = ToolContext {
                            cwd: std::env::current_dir().unwrap_or_default(),
                            session_id: "elio".into(),
                            user_message: None,
                        };
                        let logger = {
                            let guard = session.inner.lock().await;
                            guard.logger.clone()
                        };
                        let session = session.clone();
                        let tx = heartbeat_tx.clone();
                        tokio::spawn(async move {
                            let start = std::time::Instant::now();
                            let result = tool.execute(input, ctx).await;
                            let elapsed = start.elapsed();

                            let result_text = result
                                .content
                                .iter()
                                .map(|b| match b {
                                    ToolContentBlock::Text { text } => text.clone(),
                                    ToolContentBlock::Image { .. } => "[图片]".into(),
                                })
                                .collect::<Vec<_>>()
                                .join("\n");

                            let mut guard = session.inner.lock().await;
                            guard.conversation.add_tool_result(id.clone(), result_text.clone(), result.is_error);
                            guard.worldview.push(
                                format!("工具 {name} 已执行完毕（耗时 {:.1}s）", elapsed.as_secs_f64()),
                                PerceptSource::ToolResult,
                            );
                            let status = if result.is_error { "失败" } else { "成功" };
                            guard.memory.record_event(MemoryEvent {
                                text: format!("工具 {name} 执行{status}: {result_text}"),
                                event_type: elio_core::memory::EventType::ToolResult,
                                session_id: None,
                            });
                            logger.log(
                                elio_core::log::EVENT_API_REQUEST,
                                &format!("工具 {name} 执行完毕（{:.1}s）", elapsed.as_secs_f64()),
                                Some("system"),
                            );
                            let _ = tx.send(serde_json::json!({
                                "type": "tool_complete",
                                "tool": name,
                                "elapsed": elapsed.as_secs_f64(),
                            }).to_string());
                        });
                    }
                }
                elio_core::mainloop::StepResult::Idle => {}
                elio_core::mainloop::StepResult::Error(e) => {
                    tracing::warn!("Elio step 错误: {e}");
                    let error = serde_json::json!({
                        "type": "error",
                        "message": e,
                        "code": "LLM_ERROR"
                    });
                    let _ = heartbeat_tx.send(error.to_string());
                }
            }

            // 3. 记忆维护 tick（慢路径推理）
            let mut guard = session.inner.lock().await;
            guard.memory_tick().await;

            // 4. 保证两次 step 之间至少间隔 30s
            let elapsed = step_start.elapsed();
            if elapsed < std::time::Duration::from_secs(30) {
                tokio::time::sleep(std::time::Duration::from_secs(30) - elapsed).await;
            }
        }
    });

    let app = routes::create_routes()
        .route("/ws", get(ws_handler))
        .route("/ws/{session_id}", get(ws_handler))
        .with_state(app_state);

    // 启动
    info!("Elio Server 监听 {addr}");
    info!("WebSocket: ws://{addr}/ws/elio");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// WebSocket 升级处理器
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let rx = state.response_tx.subscribe();
    ws.on_upgrade(move |socket| handle_socket(socket, state, rx))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>, rx: tokio::sync::broadcast::Receiver<String>) {
    // 获取默认会话
    if let Some(session) = state.session_mgr.get_default() {
        ws::handle_ws(socket, &session, rx).await;
    } else {
        tracing::error!("没有可用会话");
    }
}

/// 解析记忆目录路径（支持相对路径）
fn resolve_memory_dir(dir: &PathBuf) -> PathBuf {
    if dir.is_absolute() {
        dir.clone()
    } else {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        cwd.join(dir)
    }
}

/// 解析提示词目录路径
fn resolve_prompts_dir() -> PathBuf {
    let cwd_prompts = std::env::current_dir()
        .unwrap_or_default()
        .join("prompts");
    if cwd_prompts.exists() {
        return cwd_prompts;
    }
    let manifest_prompts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("prompts");
    manifest_prompts
}

/// 解析日志目录路径
fn resolve_logs_dir() -> PathBuf {
    let cwd_logs = std::env::current_dir()
        .unwrap_or_default()
        .join("logs");
    if cwd_logs.exists() || cwd_logs.parent().map_or(false, |p| p.exists()) {
        return cwd_logs;
    }
    PathBuf::from("logs")
}

/// 去掉 <think>...</think> 思考块（保留其余文本）
fn strip_think_tags(text: &str) -> String {
    let mut result = String::new();
    let mut in_think = false;
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
    while i < chars.len() {
        if !in_think && i + 6 < chars.len()
            && chars[i] == '<'
            && chars[i+1] == 't'
            && chars[i+2] == 'h'
            && chars[i+3] == 'i'
            && chars[i+4] == 'n'
            && chars[i+5] == 'k'
            && chars[i+6] == '>'
        {
            in_think = true;
            i += 7;
            continue;
        }
        if in_think && i + 8 < chars.len()
            && chars[i] == '<'
            && chars[i+1] == '/'
            && chars[i+2] == 't'
            && chars[i+3] == 'h'
            && chars[i+4] == 'i'
            && chars[i+5] == 'n'
            && chars[i+6] == 'k'
            && chars[i+7] == '>'
        {
            in_think = false;
            i += 8;
            continue;
        }
        if !in_think {
            result.push(chars[i]);
        }
        i += 1;
    }
    result.trim().to_string()
}

/// 去掉第一个 <en>...</en><zh>...</zh> 对
/// Phase 2 已合成第一段后，主路径用此函数去掉第一段，只合成剩余段落
fn strip_first_speech_block(text: &str) -> String {
    if let Some(en_start) = text.find("<en>") {
        if let Some(en_end_rel) = text[en_start..].find("</en>") {
            let en_end = en_start + en_end_rel + "</en>".len();
            // 找 </en> 后的第一个 <zh>
            if let Some(zh_start_rel) = text[en_end..].find("<zh>") {
                let zh_start = en_end + zh_start_rel;
                if let Some(zh_end_rel) = text[zh_start..].find("</zh>") {
                    let zh_end = zh_start + zh_end_rel + "</zh>".len();
                    let mut result = String::with_capacity(text.len());
                    result.push_str(&text[..en_start]);
                    result.push_str(&text[zh_end..]);
                    return result.trim().to_string();
                }
            }
            // 没有 <zh>，只去掉到 </en>
            let mut result = String::with_capacity(text.len());
            result.push_str(&text[..en_start]);
            result.push_str(&text[en_end..]);
            return result.trim().to_string();
        }
    }
    text.to_string()
}

/// 流式版本: 逐块剥离 <think>...</think> 内容
///
/// 与 strip_think_tags() 的差异：
/// - 有状态（in_think 跨 chunk 保持）
/// - 每次 feed() 处理一个 chunk，返回可安全显示的文本
struct ThinkStripper {
    in_think: bool,
}

impl ThinkStripper {
    fn new() -> Self {
        Self { in_think: false }
    }

    /// 处理一个文本块，返回剥离 <think> 块后的内容
    fn feed(&mut self, chunk: &str) -> String {
        let mut out = String::new();
        let chars: Vec<char> = chunk.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if !self.in_think
                && i + 6 < chars.len()
                && chars[i] == '<'
                && chars[i + 1] == 't'
                && chars[i + 2] == 'h'
                && chars[i + 3] == 'i'
                && chars[i + 4] == 'n'
                && chars[i + 5] == 'k'
                && chars[i + 6] == '>'
            {
                self.in_think = true;
                i += 7;
                continue;
            }
            if self.in_think
                && i + 8 < chars.len()
                && chars[i] == '<'
                && chars[i + 1] == '/'
                && chars[i + 2] == 't'
                && chars[i + 3] == 'h'
                && chars[i + 4] == 'i'
                && chars[i + 5] == 'n'
                && chars[i + 6] == 'k'
                && chars[i + 7] == '>'
            {
                self.in_think = false;
                i += 8;
                continue;
            }
            if !self.in_think {
                out.push(chars[i]);
            }
            i += 1;
        }
        out
    }
}
