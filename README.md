# Elio — 电子生命体

Elio 是一个持续运行的桌面 AI 伴侣，基于 [cc-haha](https://github.com/NanmiCoder/cc-haha)（Claude Code 开源分支）深度改造而成。拥有独立人格、四维图记忆、时间片驱动的自主意识循环，以及流式 TTS 语音输出。

---

## 快速开始

```bash
# 安装依赖
bun install

# 启动 Server（唯一运行形态）
bun src/server/index.ts --port 3456
```

**终端客户端（含 TTS 语音播放）：**

```bash
bun client.ts
```

---

## 架构概览

```
┌──────────────────────────────────────────────────────────┐
│                     Elio 主循环                            │
│                                                          │
│  heartbeatService (定时器)  ──每30s──→  MainLoop.step()   │
│                                            │             │
│                     ┌──────────────────────┘             │
│                     ▼                                    │
│  WorldviewBuffer ──→ buildWorldview() ──→ sendWorldview  │
│  (用户消息等)         时间+事件+上轮输出                    │
│                     │                                    │
│                     ▼                                    │
│              CLI 子进程 (Elio 大脑)                        │
│              │        │        │                         │
│              ▼        ▼        ▼                         │
│         system prompt  +  messages  →  LLM (Sonnet)      │
│                     │                                    │
│                     ▼                                    │
│              Elio 自主决定: 继续 / 切换 / 安静             │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│                     记忆系统 (独立时钟)                     │
│                                                          │
│  里 Agent (MemoryAgent)                                  │
│  ├─ Fast Path: 输入到达 → 事件节点 → 四维遍历 → 叙事 (<100ms)│
│  └─ Slow Path: 后台 30s → DeepSeek 补叙事+实体+因果边      │
│                     │                                    │
│                     ▼                                    │
│  ContextBridge → 表 Agent 读记忆上下文 (一行提示词)         │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│                     TTS 语音系统                           │
│                                                          │
│  LLM 输出 (<ja>…</ja>)                                    │
│       │                                                  │
│       ▼                                                  │
│  ttsService.synthesize()                                 │
│       │  streaming_mode=true, batch_size=1               │
│       ▼                                                  │
│  GPT-SoVITS API (端口 9880)                               │
│       │  逐句 PCM 流式返回                                 │
│       ▼                                                  │
│  onChunk 回调 → sendToSession(tts_chunk)                  │
│       │  WebSocket 推送每句 WAV                            │
│       ▼                                                  │
│  client.ts (终端) / client.html (浏览器)                   │
│       │  下载 → 排队 → PowerShell PlaySync() 连续播放      │
│       ▼                                                  │
│  🔊 首句 ~2s 延迟（非流式 ~11s）                           │
└──────────────────────────────────────────────────────────┘
```

---

## 核心设计

### 时间片模型

Elio 不是一问一答的 chatbot。她每 30 秒收到一个 `<worldview>` 时钟信号，阅读世界状态后自主决策。

```
t=0:  worldview → Elio: "跑个脚本"
t=10: interrupt (仅停LLM，工具继续) → worldview: "master spoke + 脚本还在跑"
      → Elio 自己判断: 继续等结果还是先回复
t=15: tool_result 到达
t=20: worldview → Elio 看到完整结果 → 决策
```

- 每 tick **无条件**发 worldview，不等空闲
- interrupt 只停 LLM 推理，工具进程继续跑（默认 `interruptBehavior='block'`）
- Elio 从 worldview 中看到"上轮做了什么"，自己判断继续还是切换

### 世界观注入

世界观作为 user message 直接进入对话历史，自然累积——Elio 能"看到"时间流逝和事件序列。

```
messages:
  user: <worldview>当前时间 15:30 下午  无外部事件  你上轮: 整理记忆</worldview>
  assistant: 继续整理了 3 条记忆...
  user: <worldview>当前时间 15:30:10  master says: "帮我看看这个"  你上轮: 整理记忆</worldview>
  assistant: 好的master...
```

详见 [prompts.md](prompts.md) 3.2 节。

### 记忆系统：双 Agent + 四维图

| | 表 Agent (Elio) | 里 Agent (MemoryAgent) |
|---|---|---|
| 模型 | Sonnet (主 key) | Fast: 纯本地 / Slow: DeepSeek v4 Flash |
| 职责 | 聊天+执行任务 | 后台自动存储+推理+推送 |
| 记忆操作 | 零——只读结果 | 全部自动化 |

事件通过四个维度连接：
- **时间维** — 先后/同时
- **语义维** — 相似/包含
- **因果维** — 导致/因为
- **实体维** — 涉及谁/什么

Fast Path (<100ms) 在消息到达时立刻创建节点+时间边+关键词检索+合成叙事。
Slow Path (每 30s) 调 DeepSeek 补叙事描述、提取实体、推理因果边。

### TTS 语音系统

Elio 通过 GPT-SoVITS 将日文 `<ja>` 块合成为语音，支持多角色/多情绪。

#### 语音自动发现

启动时 `ttsService.ts` 扫描 `D:\VS_python\TTS\` 下所有角色目录，自动发现：

```
D:\VS_python\TTS\
├── GPT-SoVITS-1007-cu124\     ← 引擎本体
│   └── GPT_SoVITS\configs\tts_infer.yaml
├── 纳西妲_ZH\
│   └── v2ProPlus\纳西妲_ZH\
│       ├── GPT_weights_v2ProPlus\xxx.ckpt
│       ├── SoVITS_weights_v2ProPlus\xxx.pth
│       └── reference_audios\
│           └── 中文\emotions\
│               ├── 【开心】今天天气真好.wav
│               ├── 【难过】有点伤心.wav
│               └── 【中立】你好.wav
└── 可琳\
    └── v4\可琳\
        └── reference_audios\
            └── 日语\emotions\
                └── ...
```

#### 切换语音

编辑项目根目录的 [voice.json](voice.json)：

```json
{"activeVoice": "可琳"}
```

重启 Elio Server → 自动更新 `tts_infer.yaml` → 重启 GPT-SoVITS API 加载新权重。

#### 流式 TTS 播放

```
LLM 输出 <ja>...</ja> 块
  → ttsService.synthesize(jaText, zhText, 'happy', onChunk)
    → GPT-SoVITS API (streaming_mode=true, batch_size=1, cut5 切句)
      → 逐句返回 PCM (~2s 首句延迟)
        → onChunk 回调 → MainLoop 发送 tts_chunk WebSocket 消息
          → client.ts 下载 WAV → 排队 → PowerShell PlaySync() 连续播放
```

- `streaming_mode=true` + `batch_size=1`：每句独立产出，不等全部合成
- `cut5` 按日文标点自动切句
- `top_k=5`：reduce from 15，微量提速
- 多个 `<ja>` 块全部 join 后一次请求，GPT-SoVITS 负责切句

### 提示词组装

每次 LLM 调用：
```
getSystemPrompt(tools, model, dirs, mcp)
  ├─ 静态部分（缓存）
  │   ├─ Elio 身份 + 时间片时钟说明
  │   ├─ 系统规则 + 工具规范 + 代码风格
  │   └─ 语气/emoji/输出格式
  │
  └─ 动态部分（按需刷新）
      ├─ memory (ContextBridge 注入)
      └─ env_info (OS/git/工作目录)
```

完整提示词原文见 [prompts.md](prompts.md)。

---

## 终端客户端

`client.ts` 是一个轻量 TUI 客户端，连接 Elio Server WebSocket，接收消息并自动播放 TTS 语音。

```bash
bun client.ts
```

**特性：**

- WebSocket 连接到 `ws://127.0.0.1:3456/ws/elio`
- 增量显示 `<think>` / `<ja>` / `<zh>` 块（随 LLM 流式输出逐块出现）
- 收到 `tts_chunk` 消息后立即下载 WAV，送入持久 PowerShell 播放器
- 单播放器进程处理所有 chunk，无进程启停间隙
- `/quit` 或 `Ctrl+C` 优雅退出

**消息类型：**

| 类型 | 说明 |
|------|------|
| `content_delta` | LLM 流式输出片段，客户端实时解析 speech 块 |
| `tts_chunk` | 单句 WAV 就绪，下载并入队播放 |
| `tool_use_complete` | 工具调用完成提示 |
| `message_complete` | 本轮消息结束 |
| `system_notification` | 系统通知 |

---

## 文件结构

```
Elio_Agent/
├── client.ts                          ← 终端 TUI 客户端（含 TTS 播放）
├── voice.json                         ← 当前活跃语音配置
├── prompts.md                         ← 完整提示词原文
├── plan.md                            ← 改造计划与执行记录
│
├── src/
│   ├── server/
│   │   ├── index.ts                   ← Server 入口（HTTP + WS）
│   │   ├── api/
│   │   │   └── previewFs.ts           ← 静态文件/音频 MIME 服务
│   │   ├── ws/
│   │   │   ├── handler.ts             ← WebSocket 连接管理 + 消息路由
│   │   │   └── events.ts              ← ClientMessage / ServerMessage 类型
│   │   └── services/
│   │       ├── heartbeatService.ts    ← 定时器壳（每 30s 调 MainLoop）
│   │       ├── MainLoop.ts            ← 时间片核心: tick → interrupt → worldview
│   │       ├── conversationService.ts ← CLI 子进程管理 + SDK WebSocket
│   │       ├── ttsService.ts          ← TTS 语音合成（GPT-SoVITS 集成）
│   │       ├── settingsService.ts     ← 用户设置管理
│   │       ├── providerService.ts     ← AI Provider 管理
│   │       └── ...                    ← 其他服务
│   │
│   ├── cli/print.ts                   ← CLI 子进程入口（SDK 通信+消息泵）
│   ├── constants/prompts.ts           ← 系统提示词主体
│   ├── query.ts                       ← LLM 调用循环（queryLoop）
│   │
│   ├── elio/
│   │   ├── index.ts                   ← Elio 模块入口
│   │   ├── WorldviewBuffer.ts         ← 外部感知缓冲区
│   │   └── memory/
│   │       ├── MemoryAgent.ts         ← 里 Agent 入口
│   │       ├── GraphStore.ts          ← 事件节点 + 四维邻接表
│   │       ├── InvertedIndex.ts       ← 倒排索引（关键词→事件ID）
│   │       ├── FastPath.ts            ← 5 步流水线（事件→边→锚点→遍历→叙事）
│   │       ├── SlowPath.ts            ← 后台 DeepSeek 驱动的深度存储
│   │       ├── ContextBridge.ts       ← sharedContext（里写表读）
│   │       └── prompts/               ← Slow Path 三个 DeepSeek prompt
│   │
│   ├── services/tools/
│   │   ├── StreamingToolExecutor.ts   ← 并行工具执行 + interruptBehavior
│   │   └── toolExecution.ts           ← 单工具执行
│   │
│   └── tools/                         ← 工具定义（Bash/Read/Write/Edit/...）
│
├── adapters/                          ← IM 适配器（飞书/钉钉/Telegram/微信）
└── docs/                              ← 详细文档
```

---

## 技术栈

| 维度 | 选型 |
|------|------|
| 运行时 | Bun |
| 语言 | TypeScript |
| AI 模型 | Claude Sonnet (表) + DeepSeek v4 Flash (里 Slow Path) |
| 通信 | WebSocket（SDK 协议 + `/ws/elio` 用户通道） |
| TTS 引擎 | GPT-SoVITS v2/v4（`api_v2.py`，端口 9880） |
| 音频播放 | PowerShell `System.Media.SoundPlayer`（终端客户端） |
| 存储 | JSONL 增量 + 内存图（events.jsonl + edges.jsonl + inverted_index.json） |

---

## 依赖服务

| 服务 | 端口 | 说明 |
|------|------|------|
| Elio Server | 3456 | 主服务 |
| GPT-SoVITS API | 9880 | TTS 语音合成 |

启动 GPT-SoVITS：
```bash
cd D:\VS_python\TTS\GPT-SoVITS-1007-cu124
runtime\python.exe api_v2.py -a 127.0.0.1 -p 9880 -c GPT_SoVITS/configs/tts_infer.yaml
```

---

## 详细文档

- [prompts.md](prompts.md) — 完整提示词原文、组装流程、注入位置、token 估算
- [plan.md](plan.md) — 改造计划、执行记录、附录
