//! WebSocket 处理器 — 与 elio-client / IM 适配器通信

use crate::session::Session;
use axum::extract::ws::{Message, WebSocket};
use elio_core::mainloop::StepResult;
use tracing::{error, info};

fn json_msg(v: serde_json::Value) -> Message {
    Message::Text(v.to_string().into())
}

/// 处理单个 WebSocket 连接
pub async fn handle_ws(mut ws: WebSocket, session: &Session) {
    info!("WebSocket 客户端已连接");

    let mainloop = &session.inner;

    loop {
        let msg = match ws.recv().await {
            Some(Ok(msg)) => msg,
            Some(Err(e)) => {
                error!("WebSocket 接收错误: {e}");
                break;
            }
            None => break,
        };

        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        let json: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                let _ = ws.send(json_msg(serde_json::json!({
                    "type": "error", "message": format!("JSON 解析失败: {e}")
                }))).await;
                continue;
            }
        };

        let msg_type = json["type"].as_str().unwrap_or("").to_string();

        match msg_type.as_str() {
            "user_message" => {
                let user_text = json["text"].as_str().unwrap_or("");
                if user_text.is_empty() {
                    continue;
                }
                info!("收到用户消息: {:.100}", user_text);

                let mut loop_guard = mainloop.lock().await;
                loop_guard.on_user_message(user_text);
                drop(loop_guard); // 尽快释放锁

                // 处理推理循环
                process_response_loop(&mut ws, mainloop).await;
            }
            "ping" => {
                let _ = ws.send(json_msg(serde_json::json!({"type": "pong"}))).await;
            }
            other => {
                let _ = ws.send(json_msg(serde_json::json!({
                    "type": "error", "message": format!("未知消息类型: {other}")
                }))).await;
            }
        }
    }

    info!("WebSocket 客户端已断开");
}

/// 处理 LLM 响应循环（支持多轮工具调用）
async fn process_response_loop(ws: &mut WebSocket, mainloop: &tokio::sync::Mutex<elio_core::mainloop::MainLoop>) {
    let mut rounds = 0;
    loop {
        if rounds >= 10 {
            let _ = ws.send(json_msg(serde_json::json!({
                "type": "error", "message": "工具调用轮次过多，已终止"
            }))).await;
            break;
        }
        rounds += 1;

        let result = {
            let mut guard = mainloop.lock().await;
            guard.step().await
        };

        match result {
            StepResult::Response(text) => {
                let _ = ws.send(json_msg(serde_json::json!({"type": "content_start"}))).await;
                let _ = ws.send(json_msg(serde_json::json!({
                    "type": "content_delta",
                    "delta": {"text": text}
                }))).await;
                let _ = ws.send(json_msg(serde_json::json!({"type": "message_complete"}))).await;
                break;
            }
            StepResult::ToolCall(name, input, id) => {
                info!("工具调用: {name}");
                let result = {
                    let mut guard = mainloop.lock().await;
                    guard.execute_tool(&name, input, &id).await
                };
                match result {
                    StepResult::Response(text) => {
                        let _ = ws.send(json_msg(serde_json::json!({"type": "content_start"}))).await;
                        let _ = ws.send(json_msg(serde_json::json!({
                            "type": "content_delta",
                            "delta": {"text": text}
                        }))).await;
                        let _ = ws.send(json_msg(serde_json::json!({"type": "message_complete"}))).await;
                        break;
                    }
                    StepResult::ToolCall(..) => continue,
                    StepResult::Error(e) => {
                        let _ = ws.send(json_msg(serde_json::json!({"type": "error", "message": e}))).await;
                        break;
                    }
                    StepResult::Idle => break,
                }
            }
            StepResult::Error(e) => {
                let _ = ws.send(json_msg(serde_json::json!({"type": "error", "message": e}))).await;
                break;
            }
            StepResult::Idle => break,
        }
    }
}
