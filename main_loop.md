# 世界感知与主循环

Elio 的自主感知-决策-行动循环（MainLoop），以及世界观注入系统。

> ⚠️ 本文档基于实际代码行为，不反映理想设计。

## 架构概览

```
用户 WebSocket 消息
    │
    ▼
on_user_perception()   ──→ WorldviewBuffer（感知缓冲）
    │                        └── 不入 Conversation 历史
    │
    ┌───────────────────────────────────────┐
    │  30s 心跳定时器（驱动一切）            │
    │                                       │
    │  每 30 秒:                            │
    │    ├── on_timer_tick()                │
    │    │     ├── WorldviewBuffer::push()   │
    │    │     └── Conversation::add_user()  │
    │    │                                  │
    │    ├── step() 一次                    │
    │    │     ├── Response → 广播          │
    │    │     ├── ToolCall → spawn 异步任务 │
    │    │     └── Idle/Error               │
    │    └── memory_tick()                  │
    └───────────────────────────────────────┘
                              │
                              ▼
                      broadcast channel
                              │
                              ▼
                      WebSocket 客户端
```

**核心要点**: 用户消息不进对话历史，Elio **不即时回复**，只在 30s 心跳时才感知并响应。

---

## 消息流（用户对话）

```
用户: "你好"
    │
    ▼
WebSocket 收到 {"type": "user_message", "text": "你好"}
    │
    ▼
ws::handle_ws()                            [ws.rs:38-46]
    │
    ├── session.inner.lock().await
    │
    └── MainLoop::on_user_perception("你好")  [mainloop.rs:151-165]
          ├── MemorySystem::record_event("你好")     ← 记入记忆
          ├── WorldviewBuffer::push("你好", User)    ← 推入感知缓冲
          └── logger::log("user.message", ...)
          ※ 不修改 conversation
          ※ 不触发 step()
          ※ 不设置 state

    ... 等待下一次心跳 ...
```

---

## 30 秒心跳定时器（实际驱动核心）

服务器启动时 `tokio::spawn` 一个 30s 间隔的心跳任务（[main.rs:109-175](d:\VS_python\Elio_Agent_v2\elio-server\src\main.rs#L109)）：

```
每 30 秒:
    │
    ├── 1. MainLoop::on_timer_tick()         [mainloop.rs:171-177]
    │        ├── WorldviewBuffer::push("定时心跳 — 30秒已过去", Timer)
    │        ├── Conversation::add_user_message("<system tick>")
    │        └── state = Thinking
    │
    ├── 2. MainLoop::step() 单次调用         [main.rs:129-218]
    │        │
    │        ├── Response(text)
    │        │     ├── broadcast: content_start
    │        │     ├── broadcast: content_delta
    │        │     └── broadcast: message_complete
    │        │
    │        ├── ToolCall(name, input, id)
    │        │     ├── worldview.push("工具 {name} 已提交，等待结果...")
    │        │     ├── 提取 Arc<dyn Tool>，释放锁
    │        │     ├── tokio::spawn 后台任务：
    │        │     │     ├── tool.execute(input).await  ← 可能跑很久
    │        │     │     ├── lock()
    │        │     │     ├── conversation.add_tool_result()
    │        │     │     ├── worldview.push("工具 {name} 已执行完毕（耗时 Xs）")
    │        │     │     ├── memory.record_event()
    │        │     │     ├── logger.log()
    │        │     │     └── broadcast: tool_complete
    │        │     └── 不等待后台任务，直接结束
    │        │
    │        ├── Idle → 什么都不做
    │        │
    │        └── Error(e)
    │              └── broadcast: {"type": "error"}
    │
    └── 3. MainLoop::memory_tick()           [mainloop.rs:290-293]
           └── 慢路径记忆维护
```

---

## WorldviewBuffer（世界观缓冲）

收集外部事件，在每次 `step()` 中被消费并注入系统提示词。

### 感知来源

| 来源 | 触发 | 示例 |
|------|------|------|
| `User` | 用户发 WebSocket 消息 | "你好" |
| `System` | 系统事件 | 配置变更 |
| `ToolResult` | 工具执行完毕 | "文件读取成功" |
| `Timer` | 30s 心跳 | "定时心跳 — 30秒已过去" |

### 数据结构

```
WorldviewBuffer                         [worldview.rs:8-14]
    ├── pending: VecDeque<Percept>     ← 待处理感知队列
    ├── recent_slices: VecDeque<PerceptionSlice>  ← 最近 7 个切片
    ├── start_time: SystemTime          ← 用于计算运行时长
    └── max_slices: 7
```

### 世界观生成

`build_worldview()` 每次 `step()` 调用时生成完整的 `<worldview>` 标签（[worldview.rs:86-98](d:\VS_python\Elio_Agent_v2\elio-core\src\worldview.rs#L86)）：

```xml
<worldview>
当前时间: 2026/6/10 23:03:53（夜间）
已持续运行: 2 小时 15 分钟
[💬 用户] 你好
[⏰ 定时] 定时心跳 — 30秒已过去
</worldview>
```

包含三个部分：

1. **当前时间** — 带时段上下文（UTC+8）
   - 5-8时 → 清晨
   - 9-11时 → 上午
   - 12-13时 → 中午
   - 14-17时 → 下午
   - 18-21时 → 傍晚
   - 22-4时 → 夜间

2. **已持续运行** — 从进程启动到现在的时长

3. **近期外部感知** — 最近切片中的用户消息/工具结果/心跳

---

## 系统提示词组装

每次 LLM 请求前，在 `step()` 中动态组装（[mainloop.rs:188-199](d:\VS_python\Elio_Agent_v2\elio-core\src\mainloop.rs#L188)）：

```
┌─ 基座（启动时由 PromptManager 从 prompts/*.txt 组装）──┐
│  由 PromptManager::build_system_prompt() 生成          │
│  （约 11 个 .txt 文件：identity, language, ...）       │
├─ 动态追加 ─────────────────────────────────────────────┤
│  1. <worldview>...</worldview>    ← WorldviewBuffer    │
│  2. ## 记忆上下文                  ← MemorySystem     │
└────────────────────────────────────────────────────────┘
```

实际拼接逻辑（[mainloop.rs:190-199](d:\VS_python\Elio_Agent_v2\elio-core\src\mainloop.rs#L190)）：
```rust
let mut system_prompt = self.config.system_prompt.clone();  // 基座
system_prompt.push_str("\n\n");
system_prompt.push_str(&worldview_text);                     // 世界观
system_prompt.push_str("\n\n## 记忆上下文\n");
system_prompt.push_str(&mem_ctx);                            // 记忆
```

---

## MainLoop 状态机

代码实际定义了 4 种状态（[mainloop.rs:41-50](d:\VS_python\Elio_Agent_v2\elio-core\src\mainloop.rs#L41)）：

| 状态 | 含义 | 触发 |
|------|------|------|
| `Idle` | 空闲，等待感知 | 对话为空 / step 完成 |
| `Thinking` | 正在 LLM 调用 | `on_timer_tick()` |
| `ExecutingTool` | LLM 正在执行工具 | `step()` 返回 ToolCall |
| `WaitingForUser` | 等待用户输入 | （预留） |

---

## MainLoop::step() 方法

每次 LLM 请求的完整流程（[mainloop.rs:180-256](d:\VS_python\Elio_Agent_v2\elio-core\src\mainloop.rs#L180)）：

```
step()
    │
    ├── 对话为空? → 返回 Idle
    │
    ├── worldview.commit_slice()
    │     将所有 pending 感知固化为一个 PerceptionSlice
    │
    ├── build_worldview()
    │     生成 <worldview> 时间/运行时长/感知文本
    │
    ├── 组装 system prompt
    │     base_prompt + worldview + 记忆上下文
    │
    ├── 日志: system.prompt（完整提示词）
    │
    ├── state = Thinking
    │
    ├── POST /v1/messages → DeepSeek API
    │
    ├── 处理响应
    │     │
    │     ├── Text { text }
    │     │     ├── conversation.add_assistant_text()
    │     │     ├── memory.record_event()
    │     │     ├── logger.log("elio.response")
    │     │     ├── state = Idle
    │     │     └── return StepResult::Response(text)
    │     │
    │     └── ToolUse { name, input, id }
    │           └── return StepResult::ToolCall(...)
    │               → 由心跳外层 loop 处理 execute_tool()
    │
    └── return StepResult::Idle（无文本也无 tool_use）
```

---

## 工具执行流程（异步）

工具执行现在分为**提交**和**完成**两个阶段，通过 worldview 向 LLM 报告进度。

### 提交阶段（心跳内同步完成）

```
step() 返回 ToolCall(name, input, id)
    │
    ├── worldview.push("工具 {name} 已提交，等待结果...", ToolResult)
    │   ↑ 下次心跳 LLM 就知道工具在跑了
    │
    ├── 提取 Arc<dyn Tool> + ToolContext，释放锁
    │
    └── tokio::spawn 后台任务
          └── 不等待，心跳结束
```

### 完成阶段（后台异步执行）

```
后台任务（可能在几秒或几分钟后完成）:
    │
    ├── tool.execute(input, ctx).await     ← 长时间执行
    │
    ├── 计算耗时 elapsed
    │
    ├── session.inner.lock().await          ← 重新拿锁写结果
    │
    ├── conversation.add_tool_result(id, text, is_error)
    ├── worldview.push("工具 {name} 已执行完毕（耗时 {elapsed}s）", ToolResult)
    ├── memory.record_event(...)
    ├── logger.log(...)
    └── broadcast: {"type": "tool_complete", "tool": name, "elapsed": ...}
```

### Worldview 中的进度呈现

```
工具刚提交后（下一次心跳 LLM 看到）：
  <worldview>
  ...
  [⏰ 定时] 定时心跳 — 30秒已过去
  [🔧 工具] 工具 search 已提交，等待结果...
  </worldview>
  → LLM 知道 "search 在跑，还没出结果"

工具完成后（再下一次心跳 LLM 看到）：
  <worldview>
  ...
  [🔧 工具] 工具 search 已执行完毕（耗时 3.2s）
  </worldview>
  → LLM 知道 "search 出结果了，来看 conversation 里的 tool_result"
```

注意：
- 如果工具执行耗时超过 30s，中间会经过若干次没有工具相关感知的心跳
- 工具结果写入 `conversation` 后，LLM 在下次 `step()` 时就能看到完整的 `tool_use` + `tool_result` 对

---

## TTS 语音合成（GPT-SoVITS）

当配置 `[tts].enabled = true` 时，elio-server 在收到 LLM 文本回复后会异步合成语音。

### 流程

```
StepResult::Response(text)
    │
    ├── 广播文本回复（即时）            ← 客户端先显示文字
    │
    └── tts::parse_speech_blocks(text)
          │
          ├── None（无日文+无标签） → 不合成
          │
          └── Some(SpeechBlocks { ja, zh, emotion })
                └── tokio::spawn 后台合成
                      ├── GPT-SoVITS API 流式调用
                      ├── 每句话 → on_chunk(wav_bytes, index)
                      └── broadcast: tts_chunk → 客户端播放
```

LLM 回复可包含语音标签控制合成内容：

```xml
<emotion>happy</emotion>
<en>Good morning! Beautiful weather today, isn't it?</en>
<zh>早上好！今天天气真好啊。</zh>
```

| 标签 | 用途 |
|------|------|
| `<en>...</en>` | 英文语音文本（合成用） |
| `<zh>...</zh>` | 中文字幕（客户端显示） |
| `<emotion>...</emotion>` | 情感（happy/sad/neutral/surprise...） |

无标签时直接使用全文作为 fallback。

### 参考音频

在 `config/default.toml` 中配置参考音频目录：

```toml
[tts]
enabled = true
base_url = "http://127.0.0.1:9880"
voice = "可琳"
default_emotion = "happy"
ref_audio_dir = "D:/VS_python/TTS/可琳/v4/可琳/reference_audios/中文/emotions"
lang = "ja"
streaming = true
```

参考音频按情感命名：`【开心】text.wav`、`【难过】text2.wav`。
服务启动时自动扫描目录，建立 emotion → 参考音频映射，支持 fallback 链。

---

## WebSocket 协议

### 客户端 → 服务端

| type | 字段 | 说明 |
|------|------|------|
| `user_message` | `text` | 用户消息（入 worldview，不入 conversation） |
| `ping` | - | 心跳探测，服务端回复 `{"type": "pong"}` |

### 服务端 → 客户端（broadcast 推送）

由心跳循环通过 `tokio::sync::broadcast` 推送（[main.rs](d:\VS_python\Elio_Agent_v2\elio-server\src\main.rs)）：

```
LLM 回复文本:
  第一步: {"type": "content_start", "blockType": "text"}
  第二步: {"type": "content_delta", "text": "Elio 回复内容..."}
  第三步: {"type": "message_complete", "usage": {"input_tokens": 0, "output_tokens": 0}}

工具执行完成（后台异步）:
  {"type": "tool_complete", "tool": "search", "elapsed": 3.2}

TTS 语音分片（后台异步）:
  {"type": "tts_chunk", "data": "<base64 wav bytes>", "chunk_index": 0, "format": "wav",
   "text": "こんにちは", "subtitle": "你好"}

错误:
  {"type": "error", "message": "...", "code": "LLM_ERROR"}
```

| type | 字段 | 说明 |
|------|------|------|
| `content_start` | `blockType` | 文本块开始 |
| `content_delta` | `text` 或 `delta.text` | 回复增量 |
| `message_complete` | `usage` | 消息结束 + token 统计 |
| `tool_complete` | `tool`, `elapsed` | 工具执行完成 |
| `tts_chunk` | `data`, `chunk_index`, `text`, `subtitle` | TTS 语音分片（base64 WAV） |
| `error` | `message`, `code` | 错误通知 |

---

## 对话历史

`Conversation` 结构（[mainloop.rs:53-93](d:\VS_python\Elio_Agent_v2\elio-core\src\mainloop.rs#L53)）：

- `max_turns: 50` — 最大对话轮次
- 超出时自动移除最旧的消息（`trim()`）
- **用户消息不走 Conversation** — 仅通过世界观注入

---

## Session 管理层

实际架构中还有一层封装（[session.rs](d:\VS_python\Elio_Agent_v2\elio-server\src\session.rs)）：

```
SessionManager
    └── Arc<Session>              ← Arc 包装，支持后台任务
          └── Mutex<MainLoop>     ← 线程安全封装
```

- `Session` 持有 `Mutex<MainLoop>`，在创建时初始化 `DeepSeekClient`
- `SessionManager` 存储 `Vec<Arc<Session>>`，目前只维护一个默认会话
- `Arc<Session>` 允许后台工具任务持有会话引用，执行完成后写回结果

---

## 与文档的关键差异（历史记录）

此文档已从原始版本修正，对齐到实际代码行为。曾有的差异：

1. ~~用户消息调用 `on_user_message()` 入 conversation~~ → 实际：`on_user_perception()` 只入 worldview
2. ~~用户发消息即时回复~~ → 实际：30s 心跳驱动回复
3. ~~存在 `on_response()` 方法~~ → 实际：回复处理在 `step()` 内联
4. ~~WebSocket 双工推送~~ → 实际：单向 broadcast（心跳→客户端）
5. ~~缺少 Session/Mutex 管理层~~ → 已补充
6. ~~缺少 ExecutingTool 状态~~ → 已补充
7. ~~缺少 memory_tick 环节~~ → 已补充
8. ~~`execute_tool()` 递归调 `step()` + 心跳内层 loop~~ → 改为单次 step + 后台异步执行
9. ~~工具执行阻塞心跳~~ → 改为 tokio::spawn 后台执行，完成后写结果
