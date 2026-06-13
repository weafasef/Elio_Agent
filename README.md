# Elio Agent v2 — Rust 重构版

Elio 是一个持续运行的桌面 AI 伴侣，拥有自主感知-决策-行动循环、四维图记忆系统和流式语音合成。

## 架构概览

```
用户 (WebSocket / 终端)
        │
        ▼
┌─ elio-server (axum) ──────────────────────────────────────────┐
│  ┌─ SessionManager ───────────────────────────────────────┐   │
│  │  Arc<Session>                                          │   │
│  │    └─ Mutex<MainLoop>  ← 心跳循环 (每 30s)              │   │
│  │         ├─ WorldviewBuffer  (感知缓冲)                  │   │
│  │         ├─ MemorySystem     (四维图记忆)                │   │
│  │         ├─ Conversation     (对话历史)                  │   │
│  │         └─ ToolRegistry     (工具注册表)                │   │
│  └────────────────────────────────────────────────────────┘   │
│  ┌─ TtsService ───────────────────────────────────────────┐   │
│  │  GPT-SoVITS HTTP API  ←→  Bun 桥接子进程               │   │
│  └────────────────────────────────────────────────────────┘   │
│  ┌─ broadcast::channel<String> ───────────────────────────┐   │
│  │  心跳循环 → 所有 WS 客户端 (单向推送)                    │   │
│  └────────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────────┘
        │                              │
        ▼                              ▼
┌─ DeepSeek API ──────────┐   ┌─ llama.cpp (Sight) ───┐
│  /v1/messages            │   │  /v1/chat/completions   │
│  (LLM 主推理)             │   │  JoyCaption VLM (Sight) │
└──────────────────────────┘   └────────────────────────┘
        │                              │
        ▼                              ▼
┌─ GPT-SoVITS ────────────┐
│  api_v2.py :9880        │
│  流式 WAV 合成 (TTS)     │
└──────────────────────────┘
```

**核心设计**: Elio 不即时回复用户消息。用户消息只进入世界观缓冲，由 30s 心跳统一驱动感知、思考和回复。

## 快速开始

### 1. 启动服务器

```bash
cd D:\VS_python\Elio_Agent_v2
cargo run --bin elio-server
```

服务器默认监听 `127.0.0.1:3456`，需要 DeepSeek API key（已在 `config/default.toml` 中配置）。

### 2. 启动 TTS（可选）

```bash
cd D:\VS_python\TTS\GPT-SoVITS-1007-cu124
runtime\python.exe api_v2.py -a 127.0.0.1 -p 9880 -c GPT_SoVITS/configs/tts_infer.yaml
```

### 3. 启动客户端

```bash
cd D:\VS_python\Elio_Agent_v2
cargo run --bin elio-client
```

### 4. 查看日志

```bash
python logs/logview_gui.py                    # 当前日志
python logs/logview_gui.py --dir logs         # 指定目录
python logs/logview_gui.py today              # 只看今天
```

## 项目结构

```
Elio_Agent_v2/
├── Cargo.toml                  # workspace 配置
├── elio-core/                  # 核心逻辑（零 I/O 依赖）
│   └── src/
│       ├── mainloop.rs         # 自主感知-决策-行动循环
│       ├── worldview.rs        # 世界感知缓冲
│       ├── prompt.rs           # 提示词管理（读取 prompts/*.txt）
│       ├── llm.rs              # LLM 客户端（DeepSeek / Anthropic API）
│       ├── log.rs              # 审计日志
│       ├── tool.rs             # Tool trait 定义
│       ├── registry.rs         # 工具注册表
│       └── memory/             # 四维图记忆系统
│           ├── types.rs        # EventNode, Edge, RelationType
│           ├── graph.rs        # GraphStore — 内存属性图
│           ├── index.rs        # InvertedIndex — 倒排索引
│           ├── disk.rs         # DiskIO — JSONL 持久化
│           ├── traversal.rs    # 4D 图遍历
│           ├── fast.rs         # FastPath — <100ms 无 LLM
│           ├── slow.rs         # SlowPath — LLM 推理
│           ├── bridge.rs       # ContextBridge — 上下文桥
│           └── system.rs       # MemorySystem trait
├── elio-server/                # HTTP + WebSocket 服务
│   ├── config/default.toml     # 服务器配置
│   ├── frontend/
│   │   └── index.html          # Web 前端（Canvas + 音频播放）
│   ├── scripts/
│   │   └── tts-bridge.ts       # Bun TTS 流式桥接
│   └── src/
│       ├── main.rs             # 入口，心跳循环，axum 启动
│       ├── ws.rs               # WebSocket 处理器
│       ├── session.rs          # 会话管理
│       ├── tts.rs              # TTS 语音服务
│       ├── config.rs           # 配置加载
│       └── routes/             # REST API
├── elio-client/                # 终端聊天客户端
├── elio-tools/                 # 工具实现
├── elio-adapters/              # IM 平台适配器
├── prompts/                    # 提示词 .txt 文件（25 个）
├── logs/                       # 审计日志目录
│   └── logview_gui.py          # 日志查看器
└── data/memory/                # 记忆持久化目录
```

---

## 主循环 (MainLoop)

### 核心原则

> **用户消息不进对话历史。Elio 只在 30s 心跳时感知并响应。**

用户通过 WebSocket 发来的消息只被推入 `WorldviewBuffer`（感知缓冲），不修改 `Conversation`（对话历史）。这意味着：
- Elio 不会即时回复——它在自己的节奏中感知世界
- 多次用户消息会在一次心跳中被一起看到
- 心跳是唯一的回复触发源

### 整体数据流

```
用户 WebSocket 消息
    │
    ▼
on_user_perception()   ──→ WorldviewBuffer（感知缓冲）
    │                        └── 不入 Conversation 历史
    │
    ┌──────────────────────────────────────────┐
    │  30s 心跳定时器（驱动一切）                │
    │                                          │
    │  每 30 秒:                               │
    │    ├── on_timer_tick()                   │
    │    │     ├── WorldviewBuffer::push()      │
    │    │     └── Conversation::add_user()     │
    │    │                                     │
    │    ├── step_stream() 一次                 │
    │    │     ├── Response → 广播文本 + TTS    │
    │    │     ├── ToolCall → spawn 后台任务    │
    │    │     └── Idle/Error                  │
    │    │                                     │
    │    └── memory_tick()                     │
    │          └── 慢路径记忆维护               │
    └──────────────────────────────────────────┘
                              │
                              ▼
                      broadcast channel
                              │
                              ▼
                      WebSocket 客户端
```

### 30s 心跳定时器

服务器启动时 `tokio::spawn` 一个心跳任务。使用 `std::time::Instant` 手动计时：

1. **立即触发** — 启动后即刻执行第一次 `step`
2. **补齐等待** — 如果 step 在 30s 内完成，sleep 到距离上一次启动满 30s
3. **超时直通** — 如果 step 超过 30s（如 LLM 调用慢），完成后立即开始下一次

这避免了 `tokio::time::interval` 的 `MissedTickBehavior::Burst` 导致的"双重心跳"问题。

每次心跳执行三个步骤：
1. `on_timer_tick()` — 推送 Timer 感知 + 系统消息到对话历史
2. `step_stream()` — 流式 LLM 调用，实时广播文本 delta
3. `memory_tick()` — 慢路径记忆维护

### 状态机

| 状态 | 含义 | 触发条件 |
|------|------|----------|
| `Idle` | 空闲，等待感知 | 对话为空 / step 完成 |
| `Thinking` | 正在 LLM 调用 | `on_timer_tick()` |
| `ExecutingTool` | 正在执行工具 | `step()` 返回 ToolCall |
| `WaitingForUser` | 等待用户输入 | （预留） |

### step() / step_stream() 流程

```
step()
    │
    ├── 对话为空? → 返回 Idle
    │
    ├── worldview.commit_slice()
    │     将所有 pending 感知固化为一个切片
    │
    ├── build_worldview()
    │     生成 <worldview> 标签（时间/运行时长/感知事件）
    │
    ├── 组装 system prompt
    │     base_prompt + worldview + 记忆上下文
    │
    ├── 日志: system.prompt, memory.output
    │
    ├── state = Thinking
    │
    ├── POST /v1/messages → DeepSeek API
    │
    └── 处理响应
          │
          ├── Text { text }
          │     ├── conversation.add_assistant_text()
          │     ├── memory.record_event()
          │     ├── logger.log("elio.response")
          │     └── return Response(text)
          │
          ├── ToolUse { name, input, id }
          │     └── return ToolCall(...)
          │
          └── (空响应) → return Idle
```

`step_stream()` 流程与 `step()` 相同，但通过 SSE 流式接收 LLM 输出，每个 `text_delta` 通过回调实时推送。

### 用户消息处理

用户发消息时走 `on_user_perception()`，不走 `on_user_message()`：

```
用户: "你好"
    │
    ▼
MainLoop::on_user_perception("你好")
    ├── MemorySystem::record_event()    ← 记入记忆
    ├── WorldviewBuffer::push()         ← 推入感知缓冲
    └── logger.log("user.message")      ← 审计日志
    ※ 不修改 conversation
    ※ 不触发 step()
    ※ 不设置 state
```

### 工具执行（异步）

工具执行分为**提交**和**完成**两个阶段：

**提交阶段**（心跳内同步）：
```
step() 返回 ToolCall(name, input, id)
    │
    ├── worldview.push("工具 {name} 已提交，等待结果...")
    ├── 提取 Arc<dyn Tool> + ToolContext，释放锁
    └── tokio::spawn 后台任务（不等待）
```

**完成阶段**（后台异步）：
```
后台任务:
    ├── tool.execute(input, ctx).await   ← 可能跑很久
    ├── session.inner.lock().await        ← 重新拿锁
    ├── conversation.add_tool_result()
    ├── worldview.push("工具 {name} 已执行完毕（耗时 Xs）")
    ├── memory.record_event()
    ├── logger.log("memory.input")
    └── broadcast: tool_complete
```

工具执行不阻塞心跳。如果工具耗时超过 30s，中间的心跳 LLM 会看到"工具已提交，等待结果..."的进度提示。

### Session 管理

```
SessionManager
    └── Arc<Session>
          └── Mutex<MainLoop>
```

- `Session` 持有 `Mutex<MainLoop>`，创建时初始化 `DeepSeekClient`
- `SessionManager` 存储 `Vec<Arc<Session>>`，目前只维护一个默认会话
- `Arc<Session>` 允许后台工具任务持有引用，完成后写回结果
- `Mutex` 序列化所有对 MainLoop 的访问

### Conversation（对话历史）

- `max_turns: 50` — 最大对话轮次
- 超出时自动移除最旧消息（`trim()`）
- 用户消息不走 Conversation，仅通过世界观注入
- 结构: `Vec<Message>`，每条消息含 `role` + `content: Vec<ContentBlock>`

---

## 世界观系统 (WorldviewBuffer)

收集外部事件，在每次 `step()` 中被消费并注入系统提示词。

### 感知来源

| 来源 | 触发 | 示例 |
|------|------|------|
| `User` | 用户发 WebSocket 消息 | "你好" |
| `System` | 系统事件 | 配置变更 |
| `ToolResult` | 工具执行完毕 | "工具 search 已执行完毕" |
| `Timer` | 30s 心跳 | "定时心跳 — 30秒已过去" |

### 数据结构

```
WorldviewBuffer
    ├── pending: VecDeque<Percept>       ← 待处理感知
    ├── recent_slices: VecDeque<PerceptionSlice>  ← 最近 7 个切片
    ├── start_time: SystemTime            ← 运行计时起点
    └── max_slices: 7
```

### 切片生命周期

1. `push(text, source)` — 感知事件进入 `pending` 队列
2. `commit_slice()` — 每次 step 前，将所有 pending 固化为一个 `PerceptionSlice`，推入 `recent_slices`（最多保留 7 个）
3. `build_worldview()` — 生成 `<worldview>` 块，包含：
   - **当前时间**（UTC+8，带时段上下文：清晨/上午/中午/下午/傍晚/夜间）
   - **运行时长**（从进程启动至今）
   - **最近感知**（最近 3 个切片的用户消息/工具结果/心跳）

```xml
<worldview>
当前时间: 2026/6/10 23:03:53（夜间）
已持续运行: 2 小时 15 分钟
[💬 用户] 你好
[🔧 工具] 工具 search 已执行完毕（耗时 3.2s）
[⏰ 定时] 定时心跳 — 30秒已过去
</worldview>
```

---

## 视觉感知 (Sight)

Sight 是 Elio 的视觉感官——每个心跳周期通过截图 + 本地 VLM 模型获取屏幕描述，作为 worldview 的五感之一。

### 架构

```
Sight 后台循环 (独立 tokio task)
  │
  ├─> PowerShell 截图 (Screen.Primary + Graphics.CopyFromScreen)
  ├─> 缩放到 1024px (System.Drawing.Bitmap resize)
  ├─> 存 PNG 临时文件, 转 base64
  ├─> Python urllib → llama-server /v1/chat/completions
  │       └─> JoyCaption (Llama-3.1-8B + Siglip2, GGUF Q4_K_M)
  │              └─> 返回 {"choices"[0]["message"]["content"]: "..."}
  └─> 写入 sight_buf (Arc<Mutex<Option<String>>>)
         └─> 心跳循环读取 → worldview.set_sight()

成功后等 15s 再截下一张，失败递增重试间隔。
```

### 世界观输出示例

```
<worldview>
当前时间: 2026/06/13 19:47:00（傍晚）
已持续运行: 12 分钟
👁 Sight: 这张截图显示的是一个代码编辑器的界面，左侧有菜单栏，
  右侧显示黑色背景的代码区域，包含多种颜色标记的语法高亮...
💬 用户: 帮我看看这段代码
</worldview>
```

### 依赖

| 组件 | 说明 |
|------|------|
| **llama-server** | llama.cpp CUDA 13.3 二进制，加载 JoyCaption 模型 |
| **JoyCaption 模型** | Llama-3.1-8B + Siglip2, Q4_K_M GGUF (~4.6GB) + mmproj (~838MB) |
| **PowerShell** | 截图（System.Windows.Forms） |
| **Python urllib** | 发 HTTP 请求（reqwest 与 llama-server HTTP 栈不兼容，回 502） |

### Sight 数据流

```
llama-server.exe (port 8080)
    │ POST /v1/chat/completions
    │ { "messages": [{ "role": "user", "content": [
    │     {"type": "image_url", "image_url": {"url": "data:image/png;base64,..."}},
    │     {"type": "text", "text": "请描述这张截图"}
    │ ]}]}
    │
    ▼
sight_buf: Arc<Mutex<Option<String>>>
    │ 心跳读取
    ▼
worldview.set_sight(desc)
```

### 配置

```toml
# elio-server/config/default.toml
[vision]
enabled = true                   # 是否启用视觉感知
base_url = "http://127.0.0.1:8080"  # llama-server 地址
```

---

## 提示词组装

### 目录结构（25 个 .txt 文件）

```
prompts/
├── identity.txt           # 身份设定（Elio，电子生命体）
├── language.txt           # 语言风格（英语口语，温暖亲切）
├── speech_blocks.txt      # 输出格式（<think>/<en>/<zh> 标签规则）
├── worldview.txt          # 时间片世界观感知说明
├── loyalty.txt            # 忠诚设定
├── actions.txt            # 行为指引
├── tone_emoji.txt         # 禁止 emoji（干扰 TTS）
├── tone_warmth.txt        # 语气温度
├── tone_format.txt        # 代码引用格式
├── system_1~5.txt         # 系统指令（预留）
├── doing_tasks_scope.txt  # 任务范围
├── doing_tasks_code.txt   # 代码任务
├── doing_tasks_rules.txt  # 任务规则
├── efficiency_public.txt  # 公共效率指引
├── efficiency_ant.txt     # 内部效率指引
├── tools_dedicated.txt    # 工具使用：优先用专用工具
├── tools_parallel.txt     # 工具使用：并行调用
├── tools_task.txt         # 工具使用：任务分解
├── agent_subagent.txt     # 子代理使用
├── agent_fork.txt         # fork 子代理
└── sub_agent.txt          # 子代理身份
```

### 组装逻辑

`PromptManager::build_system_prompt()` 按以下顺序拼接：

1. **intro** — `identity` + `language` + `speech_blocks` + `worldview` + `loyalty`
2. **system** — `system_1` ~ `system_5`
3. **doing_tasks** — `doing_tasks_scope` + `doing_tasks_code` + `doing_tasks_rules`
4. **actions** — `actions`
5. **tools** — `tools_dedicated` + `tools_parallel` + `tools_task`
6. **tone** — `tone_emoji` + `tone_warmth` + `tone_format`
7. **efficiency** — `efficiency_public`
8. **`=== DYNAMIC BOUNDARY ===`** — 静态与动态内容的分界线
9. **动态注入** — `<worldview>` 块 + `## 记忆上下文`

```
┌─ 静态基座（启动时加载一次）────────────────────────────┐
│  intro + system + doing_tasks + actions               │
│  + tools + tone + efficiency                          │
├─ === DYNAMIC BOUNDARY === ─────────────────────────────┤
│  <worldview>...</worldview>      ← WorldviewBuffer     │
│  ## 记忆上下文                   ← MemorySystem        │
└────────────────────────────────────────────────────────┘
```

### 核心文件内容

**identity.txt** — Elio 是电子生命体，性格偏女性，属于 creator/master，既是协作者也是家人。

**language.txt** — 使用英语交流，语气自然温暖如动漫角色，可用随性口语体，自然使用句尾语气词（right? / you know / huh / yeah / okay?）。

**speech_blocks.txt** — 三种输出标签：
- `<think>` — 内心独白（不合成语音）
- `<en>` — 说出口的英文（喂给 TTS 引擎）
- `<zh>` — 中文字幕（前端显示）

关键规则：建议输出 2~3 段 `<en>`+`<zh>` 对，所有 `<en>` 单词总数不超 80，数字/日期/货币要写成朗读形式，代码/URL/emoji 不进 `<en>`。

### 标签数据流

```
LLM 回复文本
  ├─ <think>...</think>  → ThinkStripper 剥离（不显示、不合成）
  ├─ <en>...</en>        → TTS 引擎合成语音 → WAV chunks → 前端播放
  ├─ <zh>...</zh>        → 前端字幕显示（与音频同步渐进展示）
  └─ <emotion>...</emotion> → TTS 情感选择（happy/sad/neutral/surprise...）
```

---

## 记忆系统

四维图记忆系统，基于事件节点 + 有向图 + 倒排索引。

### 架构

```
MemorySystem trait  ← MainLoop 只依赖这个接口
    └── GraphMemorySystem（默认实现）
           │
           ├── FastPath  ─── <100ms, 纯规则, 无 LLM
           ├── SlowPath  ─── 每 30s, DeepSeek 驱动推演
           ├── GraphStore ─── 内存属性图（节点 + 边）
           ├── InvertedIndex ─ 关键词 → 事件 ID 映射
           ├── DiskIO ─────── JSONL 持久化
           └── ContextBridge ─ 唯一输出 → 注入提示词
```

### 数据模型

**EventNode（事件节点）** — 记忆的基本单位：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `String` | 唯一 ID（`evt_{timestamp}_{random}`） |
| `text` | `String` | 事件内容 |
| `event_type` | `EventType` | UserMessage / AssistantMessage / ToolResult / ... |
| `timestamp` | `i64` | 毫秒级 Unix 时间戳 |
| `keywords` | `Vec<String>` | 提取的关键词 |
| `entities` | `Vec<String>` | 关联实体 |
| `session_id` | `Option<String>` | 会话标识 |

**Edge（图边）** — 连接两个事件的有向边：

| 字段 | 类型 | 说明 |
|------|------|------|
| `source` | `String` | 源节点 ID |
| `target` | `String` | 目标节点 ID |
| `relation` | `RelationType` | 关系类型 |
| `confidence` | `f64` | 置信度 [0.0, 1.0] |
| `reason` | `Option<String>` | 推理依据 |

**RelationType（关系类型）** — 四种维度：

| 维度 | 关系 | 含义 |
|------|------|------|
| 时间 | `Precedes` | A 在 B 之前发生 |
| 因果 | `LeadsTo`, `BecauseOf`, `Enables`, `Prevents`, `ResponseTo` | 因果关系链 |
| 语义 | `RelatedTo`, `SimilarTo`, `PartOf` | 语义关联 |
| 实体 | `References` | 实体引用 |

### FastPath（<100ms，无 LLM）

每条消息到达时立即触发：

```
用户消息 / Elio 回复 / 工具结果
    │
    ▼
FastPath::process()
    │
    ├── 1. 关键词提取 (extract_keywords)
    │     去停用词（中英文），长词优先，上限 10 个
    │
    ├── 2. 创建 EventNode
    │
    ├── 3. 加入 GraphStore
    │
    ├── 4. 建立时间边 (Precedes)
    │     指向最近的事件节点
    │
    ├── 5. 搜索倒排索引
    │     关键词匹配 → 建立 RelatedTo 边
    │
    └── 6. 更新 ContextBridge
          合成叙事摘要，供提示词注入
```

### SlowPath（每 30s，LLM 推演）

由心跳 `memory_tick()` 触发：

```
心跳 tick (30s)
    │
    ▼
SlowPath::tick()
    │
    ├── 收集未处理事件（最多 10 个）
    │
    ├── 1. 叙事补全
    │     LLM 分析事件 → 提取实体 → 建立 References 边
    │
    ├── 2. 因果推断
    │     LLM 分析因果关系 → 建立 LeadsTo/BecauseOf 边
    │
    ├── 3. 置信度过滤
    │     只保留 confidence >= 0.7 的边
    │
    └── 4. 更新 ContextBridge
```

### 4D 图遍历

从根节点出发，沿四个维度分别深度遍历（最大 2 跳）：

```
TraversalDimension:
    Temporal ── Precedes 边
    Semantic ── RelatedTo / SimilarTo / PartOf 边
    Causal ──── LeadsTo / BecauseOf / Enables / Prevents / ResponseTo 边
    Entity ──── References 边
```

每个维度独立 DFS，返回路径及置信度乘积。

### 持久化

```
DiskIO (JSONL)
    ├── events.jsonl       ← 事件追加写入
    ├── edges.jsonl        ← 边追加写入
    └── inverted_index.json ← 索引全量写入
```

启动时 `load()` 恢复全部数据，每 30 秒 `save()` 快照。

### MemorySystem trait（可替换后端）

```rust
#[async_trait]
pub trait MemorySystem: Send + Sync {
    fn record_event(&mut self, event: MemoryEvent);
    fn get_context(&self) -> String;
    async fn tick(&mut self);
    fn save(&self) -> Result<(), DiskError>;
    fn load(&mut self) -> Result<(), DiskError>;
    fn stats(&self) -> MemoryStats;
    fn clear(&mut self);
}
```

7 个方法，可以替换为向量数据库等后端。

---

## LLM 客户端

### DeepSeekClient

通过 Anthropic Messages API 兼容端点访问 DeepSeek：

- **端点**: `POST {base_url}/v1/messages`
- **认证**: `x-api-key` + `anthropic-version: 2023-06-01`
- **默认模型**: `deepseek-v4-flash`
- **超时**: 120s

### LlmClient trait

```rust
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;
    async fn chat_stream(
        &self, request: ChatRequest,
        on_event: Box<dyn Fn(StreamEvent) + Send>,
    ) -> Result<ChatResponse, LlmError>;
}
```

### 请求/响应结构

**ChatRequest**: `model` + `system`（系统提示词） + `messages`（对话历史） + `tools`（工具定义） + `max_tokens`

**ChatResponse**: `content: Vec<ContentBlock>` + `usage`（token 统计） + `stop_reason` + `model`

**ContentBlock** 三种类型：
- `Text { text }` — 文本回复
- `ToolUse { name, input, id }` — 工具调用请求
- `ToolResult { tool_use_id, content, is_error }` — 工具执行结果

### 流式 SSE 解析

SSE 事件类型：
- `content_block_start` — 文本块/工具调用开始
- `content_block_delta` — 文本增量 (`TextDelta`) 或 JSON 增量 (`InputJsonDelta`)
- `content_block_stop` — 块结束（工具调用 JSON 拼装完成）
- `message_delta` — `stop_reason` + `usage`
- `message_stop` / `ping`

兼容两种 SSE 格式：标准 Anthropic（`event:` 行前缀）和 DeepSeek 变体（事件类型嵌在 JSON 内）。

---

## TTS 语音合成

### 架构

```
LLM 流式输出
    │
    ├── Phase 2: 检测到 </en> → 立即合成第一段
    │
    └── 完整回复到达
          │
          ├── strip_think_tags()       ← 去 <think>
          ├── strip_first_speech_block() ← Phase2 已合成则去首段
          ├── parse_speech_blocks()     ← 提取 en/zh/emotion
          │
          └── TtsService::synthesize_stream()
                │
                ├── streaming=false → HTTP POST /tts → 单个完整 WAV
                │
                └── streaming=true  → Bun 桥接子进程
                      ├── stdin: JSON { text, ref_audio_path, ... }
                      └── stdout: JSON lines
                            ├── {"type":"chunk", "data":"<base64 wav>", "index":0}
                            ├── {"type":"chunk", ...}
                            └── {"type":"done", "chunks": N}
```

### Phase 2 提前合成

当 LLM 流式输出中出现第一个 `</en>` 闭合标签时，立即：
1. 提取 `<en>...</en>` 之间的英文文本
2. 设置 `tts_started` 原子标志
3. `tokio::spawn` 后台任务合成（使用默认情感 `"happy"`，因为 `<emotion>` 可能尚未到达）

这让音频在 LLM 完成整个回复前就开始生成，减少感知延迟。

### 主路径合成（完整回复）

收到完整回复后：
1. `strip_think_tags()` 去掉 `<think>` 块
2. 如果 Phase 2 已触发 → `strip_first_speech_block()` 移除第一个 `<en>...</en><zh>...</zh>` 对
3. `parse_speech_blocks()` 提取剩余段落的 `en`、`zh`、`emotion`
4. 后台异步合成 → 广播 `tts_chunk`

### 流式管道（Bun 桥接）

`scripts/tts-bridge.ts` — Bun 子进程，解决 Rust `reqwest` 合并 HTTP chunk 的问题：

- stdin 接收 JSON 请求（text + 参考音频信息）
- 使用 Bun `fetch()` + Web Streams API 保留 GPT-SoVITS 原始 chunk 边界
- stdout 逐行输出 JSON（每句一个 WAV 分片）
- `parallel_infer: false` — 不切分子段，确保每个 chunk 是完整句子

### 参考音频管理

- 启动时扫描 `ref_audio_dir` 中所有 `.wav` 文件
- 文件名格式：`【情感】文本.wav`（如 `【开心】你好.wav`）
- 中文情感通过映射表转为英文键（开心→happy, 难过→sad, 吃惊→surprise...）
- 回退链：精确匹配 → `"default"` → `"neutral"` → `"happy"`

### 前端音频播放

- 收到 `tts_chunk` → base64 解码 → `AudioContext.decodeAudioData()`
- 顺序播放所有 chunk，计算每个 chunk 的 RMS 音量分布（50ms 窗口）
- 音量驱动角色振动幅度，播放进度驱动字幕渐进展示

---

## WebSocket 协议

### 客户端 → 服务端

| type | 字段 | 说明 |
|------|------|------|
| `user_message` | `text` | 用户消息（入 worldview，不入 conversation） |
| `ping` | - | 心跳探测，服务端回复 `pong` |

### 服务端 → 客户端（broadcast 推送）

所有 Elio 输出通过 `tokio::sync::broadcast::channel<String>(64)` 单向推送给所有连接的客户端。

| type | 关键字段 | 说明 |
|------|----------|------|
| `content_start` | `blockType: "text"` | 文本块开始 |
| `content_delta` | `delta.text` | LLM 回复增量（已剥离 `<think>`） |
| `message_complete` | `usage` | 消息结束 + token 统计 |
| `tts_chunk` | `data`（base64 WAV）, `chunk_index`, `text`, `subtitle` | TTS 语音分片 |
| `tool_complete` | `tool`, `elapsed` | 工具执行完成 |
| `error` | `message`, `code` | 错误通知 |

### 消息序列示例

```
→ content_start    {"blockType":"text"}
→ content_delta    {"delta":{"text":"Oh, I already"}}
→ content_delta    {"delta":{"text":" found the issue."}}
→ tts_chunk        {"chunk_index":0, "data":"UklGRi4..."}
→ content_delta    {"delta":{"text":" Do you want"}}
→ content_delta    {"delta":{"text":" me to fix it?"}}
→ tts_chunk        {"chunk_index":1, "data":"UklGRj8..."}
→ message_complete {"usage":{"input_tokens":1234,"output_tokens":89}}
→ tool_complete    {"tool":"read_file","elapsed":2.1}
```

---

## 审计日志

### 格式

JSONL（每行一个 JSON 事件），按日期分文件：`logs/YYYY-MM-DD.jsonl`

```json
{
  "timestamp": "2026-06-13T15:09:43.770+08:00",
  "type": "memory.input",
  "data": "用户消息内容或事件数据",
  "source": "memory",
  "session_id": "deepseek-v4-flash"
}
```

### 事件类型（8 种）

| 常量 | 类型字符串 | 用途 |
|------|-----------|------|
| `EVENT_USER_MESSAGE` | `user.message` | 用户消息 |
| `EVENT_ELIO_RESPONSE` | `elio.response` | Elio 回复 |
| `EVENT_SYSTEM_HEARTBEAT` | `system.heartbeat` | 30s 心跳 |
| `EVENT_SYSTEM_PROMPT` | `system.prompt` | 完整系统提示词 |
| `EVENT_API_REQUEST` | `api.request` | API 请求 |
| `EVENT_API_RESPONSE` | `api.response` | API 响应 |
| `EVENT_MEMORY_INPUT` | `memory.input` | 记忆系统输入 |
| `EVENT_MEMORY_OUTPUT` | `memory.output` | 记忆系统输出 |

### 设计

- **即时写入** — 每个事件 `OpenOptions::append` 后立即 flush，无缓冲，崩溃安全
- **日志查看器** — `logview_gui.py` 提供 GUI 浏览和搜索

---

## 前端 (index.html)

单页 Web 应用，通过 WebSocket 连接 Elio 服务端。

### 角色渲染

- Canvas 400×600，绘制角色图片（`Elio-kimono-256-512.png`）
- 垂直振动：`sin(t × freq)` 谐波叠加，由音频音量驱动幅度
- 空闲时使用微弱的低频正弦波保持呼吸感

### 字幕同步

- 提取 `<zh>...</zh>` 内容作为字幕
- **基于音频进度渐进展示**字符（非基于文本到达时间）
- 计算 `audioPlayedDuration / totalAudioDuration` 比例逐步揭示

### 消息处理

```
content_start  → 重置字幕和音频状态
content_delta  → 累积文本 + 提取 <zh> 字幕
tts_chunk      → base64 解码 → AudioContext 解码 → 入队播放
message_complete → 无 <zh> 时用全文做 fallback 字幕
error          → 日志记录
```

---

## 功能状态

| 功能 | 状态 |
|------|------|
| LLM 对话 (DeepSeek) | ✅ 完成 |
| 记忆系统 (FastPath/SlowPath) | ✅ 完成 |
| 四维图遍历 | ✅ 完成 |
| JSONL 持久化 | ✅ 完成 |
| 世界观注入 (时间/运行时长) | ✅ 完成 |
| 30s 自主心跳循环 | ✅ 完成 |
| 审计日志 (logs/*.jsonl) | ✅ 完成 |
| 日志 GUI 查看器 | ✅ 完成 |
| HTTP + WebSocket 服务 | ✅ 完成 |
| 终端聊天客户端 | ✅ 完成 |
| TTS 语音服务 (GPT-SoVITS) | ✅ 完成 |
| TTS Bun 桥接 (流式分片) | ✅ 完成 |
| Phase 2 提前 TTS 合成 | ✅ 完成 |
| Web 前端 (Canvas + 音频同步) | ✅ 完成 |
| 工具系统 | 🔧 待实现 |
| IM 适配器 (飞书/钉钉/Telegram/微信) | 📅 计划中 |

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
- **TTS 桥接**: Bun + TypeScript
- **前端**: 原生 HTML5 Canvas + Web Audio API
- **持久化**: JSONL（追加写入）
- **配置**: TOML
