//! Elio Client — 终端 WebSocket 客户端
//!
//! 连接到 elio-server，提供聊天界面。
//! 用法: cargo run --bin elio-client [--server ws://127.0.0.1:3456]

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use std::env;

const DEFAULT_URL: &str = "ws://127.0.0.1:3456/ws";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 从命令行参数或环境变量获取服务器地址
    let server_url = env::args().nth(1)
        .or_else(|| env::var("ELIO_SERVER_URL").ok())
        .unwrap_or_else(|| {
            let default = format!("{}/elio", DEFAULT_URL);
            println!("用法: elio-client [ws://地址:端口]\n默认连接: {default}\n");
            default
        });

    println!("🔗 连接到 Elio Server: {server_url}");

    // 建立 WebSocket 连接
    let (ws_stream, _) = connect_async(&server_url).await
        .map_err(|e| anyhow::anyhow!("连接失败: {e}"))?;
    println!("✅ 已连接！输入消息按回车发送，Ctrl+C 退出\n");

    let (mut write, mut read) = ws_stream.split();

    // 任务 1: 从 stdin 读取用户输入并发送
    let stdin_task = tokio::spawn(async move {
        let stdin = BufReader::new(tokio::io::stdin());
        let mut lines = stdin.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            let msg = serde_json::json!({
                "type": "user_message",
                "text": line
            });

            if let Err(e) = write.send(Message::Text(msg.to_string().into())).await {
                eprintln!("发送失败: {e}");
                break;
            }
        }
    });

    // 任务 2: 从 WebSocket 读取服务器消息并显示
    let output_handle = tokio::spawn(async move {
        while let Some(msg_result) = read.next().await {
            match msg_result {
                Ok(Message::Text(text)) => {
                    handle_server_message(&text);
                }
                Ok(Message::Close(_)) => {
                    println!("\n❌ 服务器连接已关闭");
                    break;
                }
                Err(e) => {
                    eprintln!("\n❌ 接收错误: {e}");
                    break;
                }
                _ => {}
            }
        }
    });

    // 等待任一任务结束
    tokio::select! {
        _ = stdin_task => {},
        _ = output_handle => {},
    }

    println!("\n👋 再见！");
    Ok(())
}

/// 处理服务器发来的 JSON 消息
fn handle_server_message(text: &str) {
    let json: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => {
            println!("[原始] {text}");
            return;
        }
    };

    let msg_type = json["type"].as_str().unwrap_or("unknown");

    match msg_type {
        "content_start" => {
            // 内容开始，新行准备输出
            print!("\n🤖 ");
        }
        "content_delta" => {
            // 增量内容
            if let Some(delta) = json["delta"]["text"].as_str() {
                print!("{delta}");
            }
        }
        "message_complete" => {
            // 消息结束
            println!("\n");
        }
        "tool_use_complete" => {
            if let Some(name) = json["name"].as_str() {
                println!("\n🔧 工具 [{name}] 执行完成");
            }
        }
        "error" => {
            let msg = json["message"].as_str().unwrap_or("未知错误");
            eprintln!("\n❌ 错误: {msg}");
        }
        "pong" => {
            // 心跳响应，忽略
        }
        "system_notification" => {
            if let Some(subtype) = json["subtype"].as_str() {
                if subtype == "tts_chunk" {
                    // TTS 音频数据，暂不处理
                }
            }
        }
        other => {
            println!("\n[服务器消息: {other}] {json}");
        }
    }

    // 立即刷新 stdout
    use std::io::Write;
    std::io::stdout().flush().ok();
}
