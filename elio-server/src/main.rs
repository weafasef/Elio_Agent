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

    // 心跳任务（每 30s 推送 Timer 感知 + 调用 step() → 广播结果到所有 WS 客户端）
    let heartbeat_state = Arc::clone(&app_state);
    let heartbeat_tx = response_tx.clone();
    let heartbeat_tts = tts_service.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        // 首次 tick 立即触发，不需要等 30s
        interval.tick().await;
        loop {
            interval.tick().await;
            tracing::debug!("heartbeat tick");

            let session = match heartbeat_state.session_mgr.get_default() {
                Some(s) => s,
                None => continue,
            };

            // 1. 定时器 tick（推 Timer 感知 + system tick 到对话）
            let mut guard = session.inner.lock().await;
            guard.on_timer_tick();
            drop(guard);

            // 2. 单次 step（工具异步执行，不阻塞心跳）
            // 用块作用域控制 guard 生命周期，ToolCall 分支提取所需数据后释放锁
            let step_result = {
                let mut guard = session.inner.lock().await;
                let result = guard.step().await;
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

                    // 去掉 <think> 思考块（只显示实际回复内容）
                    let display_text = strip_think_tags(&text);
                    let start = serde_json::json!({"type": "content_start", "blockType": "text"});
                    let delta = serde_json::json!({"type": "content_delta", "text": display_text});
                    let complete = serde_json::json!({
                        "type": "message_complete",
                        "usage": {"input_tokens": 0, "output_tokens": 0}
                    });
                    let _ = heartbeat_tx.send(start.to_string());
                    let _ = heartbeat_tx.send(delta.to_string());
                    let _ = heartbeat_tx.send(complete.to_string());

                    // ── TTS 语音合成（后台异步，不阻塞心跳） ──────────────
                    let tts_text = strip_think_tags(&text);
                    if let Some(ref tts) = heartbeat_tts {
                        if let Some(speech) = tts::parse_speech_blocks(&tts_text) {
                            let tts = tts.clone();
                            let tx = heartbeat_tx.clone();
                            let ja_text = speech.ja;
                            let ja_for_msg = ja_text.clone();
                            let zh_text = speech.zh;
                            let emotion = speech.emotion;
                            tokio::spawn(async move {
                                tracing::info!("TTS 合成: emotion={emotion}, ja=「{:.60}」", ja_text);
                                let result = tts.synthesize_stream(
                                    &ja_text,
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
                                            "text": ja_for_msg,
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
