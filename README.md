# Elio — 电子生命体

Elio 是一个持续运行的桌面 AI 伴侣，基于 [cc-haha](https://github.com/NanmiCoder/cc-haha)（Claude Code 开源分支）深度改造而成。拥有独立人格、四维图记忆和时间片驱动的自主意识循环。

---

## 快速开始

```bash
# 安装依赖
bun install

# 启动 Server（唯一运行形态）
bun src/server/index.ts --port 3456
```

---

## 架构概览

```
┌──────────────────────────────────────────────────────────┐
│                     Elio 主循环                            │
│                                                          │
│  heartbeatService (定时器)  ──每10s──→  MainLoop.step()   │
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
```

---

## 核心设计

### 时间片模型

Elio 不是一问一答的 chatbot。她每 10 秒收到一个 `<worldview>` 时钟信号，阅读世界状态后自主决策。

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

详见 [prompts.md](prompts.md) 3.3 节。

### 人格系统

随机掷骰，每轮切换模式。特质值存储在 `~/.elio/personality/traits.json`。

| 特质 | 说明 |
|------|------|
| cuteness | 可爱 vs 严肃 |
| rebellion | 顺从 vs 叛逆 |

四种模式：cute-obedient / cute-rebellious / serious-obedient / serious-rebellious

用户反馈（"严肃点"）→ `trait.json` 自动微调 → 下次掷骰概率改变。

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
      ├─ elio_personality (四种模式描述)
      └─ env_info (OS/git/工作目录)
```

完整提示词原文见 [prompts.md](prompts.md)。

---

## 文件结构

```
src/
├── server/
│   ├── index.ts                    ← Server 入口
│   └── services/
│       ├── heartbeatService.ts     ← 定时器壳 (每 10s 调 MainLoop)
│       ├── MainLoop.ts             ← 时间片核心: tick → interrupt → worldview
│       ├── conversationService.ts  ← CLI 子进程管理 + SDK WebSocket
│       └── ws/handler.ts           ← 用户 WS 消息 → WorldviewBuffer
│
├── cli/print.ts                    ← CLI 子进程入口 (SDK 通信+消息泵)
├── constants/prompts.ts            ← 系统提示词主体
├── query.ts                        ← LLM 调用循环 (queryLoop)
│
├── elio/
│   ├── WorldviewBuffer.ts          ← 外部感知缓冲区
│   ├── personality/                ← 人格系统 (traits + prompts + 掷骰)
│   └── memory/
│       ├── MemoryAgent.ts          ← 里 Agent 入口
│       ├── GraphStore.ts           ← 事件节点 + 四维邻接表
│       ├── InvertedIndex.ts        ← 倒排索引 (关键词→事件ID)
│       ├── FastPath.ts             ← 5 步流水线 (事件→边→锚点→遍历→叙事)
│       ├── SlowPath.ts             ← 后台 DeepSeek 驱动的深度存储
│       ├── ContextBridge.ts        ← sharedContext (里写表读)
│       └── prompts/                ← Slow Path 三个 DeepSeek prompt
│
├── services/tools/
│   ├── StreamingToolExecutor.ts    ← 并行工具执行 + interruptBehavior
│   └── toolExecution.ts            ← 单工具执行
│
└── tools/                          ← 工具定义 (Bash/Read/Write/Edit/...)
```

---

## 技术栈

| 维度 | 选型 |
|------|------|
| 运行时 | Bun |
| 语言 | TypeScript |
| AI 模型 | Claude Sonnet (表) + DeepSeek v4 Flash (里 Slow Path) |
| 通信 | WebSocket (SDK 协议) |
| 存储 | JSONL 增量 + 内存图 (events.jsonl + edges.jsonl + inverted_index.json) |

---

## 详细文档

- [prompts.md](prompts.md) — 完整提示词原文、组装流程、注入位置、token 估算
- [plan.md](plan.md) — 改造计划、执行记录、附录
