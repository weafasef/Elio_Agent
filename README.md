# Elio Agent v2 — Rust 重构版

Elio 是一个持续运行的桌面 AI 伴侣，原本基于 Claude Code (cc-haha) 的 TypeScript 分支重构为 Rust。

## 架构

```
elio-client (终端) ──WS──> elio-server (axum)
                                │
                                ├── LLM Client (reqwest) ──HTTP──> DeepSeek API
                                ├── Tool 系统 (Tool trait + 扩展)
                                ├── MainLoop (自主 30s 心跳循环)
                                ├── 世界观系统 (时间/运行时长/外部感知)
                                ├── 记忆系统 (FastPath + SlowPath + 图存储)
                                ├── TTS 服务 (GPT-SoVITS)
                                ├── 审计日志 (logs/YYYY-MM-DD.jsonl)
                                └── IM 适配器 (飞书/钉钉/Telegram/微信)
```

## 快速开始

### 1. 启动服务器

```bash
cd D:\VS_python\Elio_Agent_v2
cargo run --bin elio-server
```

服务器默认监听 `127.0.0.1:3456`，需要 DeepSeek API key（已在 `config/default.toml` 中配置）。

### 2. 启动客户端

```bash
cd D:\VS_python\Elio_Agent_v2
cargo run --bin elio-client
```

### 3. 查看日志

```bash
python logs/logview_gui.py                    # 当前日志
python logs/logview_gui.py --dir logs         # 指定目录
python logs/logview_gui.py today              # 只看今天
```

## 项目结构

```
elio/
├── Cargo.toml                 # workspace 配置
├── elio-core/                 # 核心逻辑（零 I/O 依赖）
│   └── src/
│       ├── mainloop.rs        # 自主感知-决策-行动循环
│       ├── worldview.rs       # 外部感知体（时间/运行时长/感知）
│       ├── prompt.rs          # 提示词管理（读取 prompts/*.txt）
│       ├── llm.rs             # LLM 客户端 (DeepSeek)
│       ├── log.rs             # 审计日志
│       ├── tool.rs            # Tool trait 定义
│       ├── registry.rs        # 工具注册表
│       └── memory/            # 四维图记忆系统
│           ├── types.rs       # EventNode, Edge, RelationType
│           ├── graph.rs       # GraphStore — 内存图存储
│           ├── index.rs       # InvertedIndex — 倒排索引
│           ├── disk.rs        # DiskIO — JSONL 持久化
│           ├── traversal.rs   # 4D 图遍历
│           ├── fast.rs        # FastPath — <100ms 无 LLM
│           ├── slow.rs        # SlowPath — LLM 推理路径
│           ├── bridge.rs      # ContextBridge — 上下文桥
│           └── system.rs      # MemorySystem trait（可替换后端）
├── elio-server/               # HTTP + WebSocket 服务
│   ├── config/default.toml    # 服务器配置
│   └── src/
│       ├── main.rs            # 入口，axum 启动
│       ├── ws.rs              # WebSocket 处理器
│       ├── session.rs         # 会话管理
│       ├── tts.rs             # TTS 语音服务
│       ├── config.rs          # 配置加载
│       └── routes/            # REST API
├── elio-client/               # 终端聊天客户端
│   └── src/main.rs
├── elio-tools/                # 工具实现（待完善）
├── elio-adapters/             # IM 平台适配器（待完善）
├── prompts/                   # 提示词 .txt 文件（25 个）
├── logs/                      # 审计日志目录
│   └── logview_gui.py         # 日志查看 GUI
└── data/memory/               # 记忆持久化目录
```

## 功能状态

| 功能 | 状态 |
|------|------|
| LLM 对话 (DeepSeek) | ✅ 完成 |
| 记忆系统 (FastPath/SlowPath) | ✅ 完成 |
| 四维图遍历 | ✅ 完成 |
| JSONL 持久化 | ✅ 完成 |
| 旧 TS 数据兼容 | ✅ 完成 (291事件/314边) |
| 世界观注入 (时间/运行时长) | ✅ 完成 |
| 30s 自主心跳循环 | ✅ 完成 |
| 审计日志 (logs/*.jsonl) | ✅ 完成 |
| 日志 GUI 查看器 | ✅ 完成 |
| HTTP + WebSocket 服务 | ✅ 完成 |
| 终端聊天客户端 | ✅ 完成 |
| TTS 语音服务 (GPT-SoVITS) | 🔧 待接入 |
| 工具系统 (Shell/文件/Web) | 🔧 待实现 |
| 飞书/钉钉/Telegram/微信适配器 | 📅 计划中 |

## 启动性能

| 指标 | TypeScript 版 | Rust 版 |
|------|-------------|---------|
| 启动时间 | ~1.8s | **<50ms** |
| 运行时内存 | ~200-400MB | **~20MB** |
| 依赖 | Bun + 63 npm 包 | Cargo + ~15 crates |
| 二进制体积 | ~150MB + node_modules | **~5MB** |

## 技术栈

- **语言**: Rust 2024 edition
- **HTTP**: axum 0.8
- **WebSocket**: tokio-tungstenite
- **LLM API**: reqwest (DeepSeek / Anthropic Messages API)
- **序列化**: serde + serde_json
- **异步**: tokio
- **持久化**: JSONL (追加写入)
- **配置**: TOML
