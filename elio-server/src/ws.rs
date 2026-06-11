//! WebSocket 处理器
//!
//! 用户消息作为「世界感知」只推入 WorldviewBuffer，由 30s 心跳驱动 Elio 回复。
//! Elio 的回复通过 broadcast channel 从 heartbeat 推送到所有 WS 连接。

use crate::session::Session;
use axum::extract::ws::{Message, WebSocket};
use tokio::sync::broadcast;
use tracing::info;

pub fn json_msg(v: serde_json::Value) -> Message {
    Message::Text(v.to_string().into())
}

/// 处理 WebSocket 连接
///
/// - 用户消息 → 只推入 MainLoop.worldview（不入 conversation）
/// - Elio 回复 ← broadcast channel（由全局 heartbeat 推送）
pub async fn handle_ws(
    mut ws: WebSocket,
    session: &Session,
    mut rx: broadcast::Receiver<String>,
) {
    info!("WebSocket 客户端已连接");

    loop {
        tokio::select! {
            // ── 来自客户端的消息 ──────────────────────────────────────────
            msg = ws.recv() => {
                match msg {
                    Some(Ok(Message::Text(t))) => {
                        let text = t.to_string();
                        let json: serde_json::Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        match json["type"].as_str() {
                            Some("user_message") => {
                                let user_text = json["text"].as_str().unwrap_or("");
                                if !user_text.is_empty() {
                                    // 用户消息 → WorldviewBuffer 仅（世界感知）
                                    let mut guard = session.inner.lock().await;
                                    guard.on_user_perception(user_text);
                                    info!("用户感知已推入: {:.100}", user_text);
                                }
                            }
                            Some("ping") => {
                                let _ = ws.send(json_msg(serde_json::json!({"type": "pong"}))).await;
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(Message::Close(_))) => break,
                    Some(Err(e)) => {
                        info!("WebSocket 断开: {e}");
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }

            // ── 来自 heartbeat 的 Elio 回复 ──────────────────────────────
            result = rx.recv() => {
                match result {
                    Ok(response) => {
                        if ws.send(Message::Text(response.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        info!("广播消息跳过 {n} 条");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    info!("WebSocket 客户端已断开");
}
