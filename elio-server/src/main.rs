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
use elio_core::memory::{GraphMemorySystem, MemorySystem};
use elio_core::prompt::PromptManager;
use session::SessionManager;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

/// 共享应用状态
pub struct AppState {
    session_mgr: SessionManager,
    config: Config,
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

    // 创建会话
    let mut session_mgr = SessionManager::new();
    let mainloop_config = elio_core::mainloop::MainLoopConfig {
        model: config.llm.model.clone(),
        llm_base_url: config.llm.base_url.clone(),
        max_tokens: 4096,
        system_prompt,
        ..Default::default()
    };
    session_mgr.create_default(mainloop_config, Box::new(graph_memory));

    // 心跳任务（30s 记忆维护）
    tokio::spawn(async {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            tracing::debug!("heartbeat tick");
        }
    });

    // 提取地址信息（先于 with_state 移动 app_state）
    let addr = format!("{}:{}", config.server.host, config.server.port);

    // 构建 axum 路由
    let app_state = Arc::new(AppState {
        session_mgr,
        config,
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
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    // 获取默认会话
    if let Some(session) = state.session_mgr.get_default() {
        ws::handle_ws(socket, session).await;
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
    // 优先在 CWD 下找 prompts/
    let cwd_prompts = std::env::current_dir()
        .unwrap_or_default()
        .join("prompts");
    if cwd_prompts.exists() {
        return cwd_prompts;
    }
    // 回退到 Cargo manifest 目录
    let manifest_prompts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("prompts");
    manifest_prompts
}
