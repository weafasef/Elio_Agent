//! Elio Client — 终端 WebSocket 客户端 + TTS 音频播放
//!
//! 连接到 elio-server，提供聊天界面和语音播放。
//! 用法: cargo run --bin elio-client [--server ws://127.0.0.1:3456]
//!
//! TTS 播放参考 Elio_Agent v1 (TypeScript)：
//! 将 WAV 写临时文件 → PowerShell System.Media.SoundPlayer 同步播放

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::mpsc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const DEFAULT_URL: &str = "ws://127.0.0.1:3456/ws";

/// 音频播放命令
enum AudioCommand {
    /// 播放 WAV 字节（同步阻塞直到播完）
    Play(Vec<u8>),
    /// 退出音频线程
    Shutdown,
}

/// 获取临时音频目录
fn audio_temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("elio-tts");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 从命令行参数或环境变量获取服务器地址
    let server_url = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("ELIO_SERVER_URL").ok())
        .unwrap_or_else(|| {
            let default = format!("{}/elio", DEFAULT_URL);
            println!("用法: elio-client [ws://地址:端口]\n默认连接: {default}\n");
            default
        });

    println!("🔗 连接到 Elio Server: {server_url}");

    // 建立 WebSocket 连接
    let (ws_stream, _) = connect_async(&server_url)
        .await
        .map_err(|e| anyhow::anyhow!("连接失败: {e}"))?;
    println!("✅ 已连接！输入消息按回车发送，Ctrl+C 退出\n");

    let (mut write, mut read) = ws_stream.split();

    // ── 音频播放线程（PowerShell SoundPlayer 同步播放） ─────────────────
    // 参考 v1 client.ts: 写 WAV 文件 → PowerShell PlaySync() 顺序播放
    let (audio_tx, audio_rx) = mpsc::channel::<AudioCommand>();
    let _audio_thread = std::thread::spawn(move || {
        while let Ok(cmd) = audio_rx.recv() {
            match cmd {
                AudioCommand::Play(wav_bytes) => {
                    // 写临时 WAV 文件
                    let tmp_dir = audio_temp_dir();
                    let tmp_path = tmp_dir.join(format!(
                        "tts_{}.wav",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos())
                            .unwrap_or(0)
                    ));

                    if let Err(e) = std::fs::write(&tmp_path, &wav_bytes) {
                        eprintln!("[音频] 写临时 WAV 文件失败: {e}");
                        continue;
                    }

                    // PowerShell SoundPlayer.PlaySync() — 同步阻塞直到播完
                    let ps_script = format!(
                        "$p = New-Object Media.SoundPlayer '{}'; $p.PlaySync(); $p.Dispose()",
                        tmp_path.display().to_string().replace('\'', "''")
                    );
                    match std::process::Command::new("powershell")
                        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                    {
                        Ok(mut child) => {
                            let _ = child.wait(); // 同步等待播放结束
                        }
                        Err(e) => {
                            eprintln!("[音频] PowerShell 播放失败: {e}");
                        }
                    }
                    // 清理临时文件
                    let _ = std::fs::remove_file(&tmp_path);
                }
                AudioCommand::Shutdown => {
                    break;
                }
            }
        }
    });

    // ── 任务 1: 从 stdin 读取用户输入并发送 ──────────────────────────────
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

            if let Err(e) = write
                .send(Message::Text(msg.to_string().into()))
                .await
            {
                eprintln!("发送失败: {e}");
                break;
            }
        }
    });

    // ── 任务 2: 从 WebSocket 读取服务器消息 ───────────────────────────────
    let audio_tx2 = audio_tx.clone();
    let output_handle = tokio::spawn(async move {
        while let Some(msg_result) = read.next().await {
            match msg_result {
                Ok(Message::Text(text)) => {
                    handle_server_message(&text, &audio_tx2);
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

    let _ = audio_tx.send(AudioCommand::Shutdown);
    println!("\n👋 再见！");
    Ok(())
}

/// 处理服务器发来的 JSON 消息
fn handle_server_message(text: &str, audio_tx: &mpsc::Sender<AudioCommand>) {
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
            print!("\n🤖 ");
        }
        "content_delta" => {
            if let Some(text) = json["text"].as_str() {
                print!("{text}");
            } else if let Some(delta) = json["delta"]["text"].as_str() {
                print!("{delta}");
            }
        }
        "message_complete" => {
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
        "tts_chunk" => {
            // TTS 音频数据 — base64 解码 → 音频线程写文件 + PowerShell 播放
            if let Some(data) = json["data"].as_str() {
                let chunk_index = json["chunk_index"].as_i64().unwrap_or(0);
                // 显示字幕（非首片不再重复显示）
                if chunk_index == 0 {
                    if let Some(sub) = json["subtitle"].as_str() {
                        if !sub.is_empty() {
                            println!("📝 {sub}");
                        }
                    }
                    print!("🔊 ");
                }
                use base64::Engine;
                match base64::engine::general_purpose::STANDARD.decode(data) {
                    Ok(wav_bytes) => {
                        let _ = audio_tx.send(AudioCommand::Play(wav_bytes));
                    }
                    Err(e) => {
                        eprintln!("\n[音频] base64 解码失败: {e}");
                    }
                }
            }
        }
        other => {
            if other != "tool_complete" {
                println!("\n[服务器消息: {other}] {json}");
            }
        }
    }

    use std::io::Write;
    std::io::stdout().flush().ok();
}
