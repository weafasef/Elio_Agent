# 世界感知与主循环

Elio 的自主感知-决策-行动循环（MainLoop），以及世界观注入系统。

## 架构概览

```
30s 心跳定时器
    │
    ▼
MainLoop::on_timer_tick()
    │
    ├── 1. 推入 Timer 感知 → WorldviewBuffer
    ├── 2. 加入 <system tick> → Conversation
    └── 3. state = Thinking
              │
              ▼
MainLoop::step()
    │
    ├── 1. commit_slice() → 固化待处理感知
    ├── 2. build_worldview() → <worldview> 含时间/运行时长/感知
    ├── 3. 读取记忆上下文 (ContextBridge)
    ├── 4. 组装完整 system prompt
    ├── 5. 日志: system.prompt (完整提示词)
    ├── 6. POST /v1/messages → DeepSeek API
    ├── 7. 解析响应
    │      ├── text → 日志: elio.response, 返回给客户端
    │      └── tool_use → ToolRegistry::execute() → 递归 step()
    └── 8. 更新 FastPath 记忆
```

## WorldviewBuffer（世界观缓冲）

收集外部事件，在每次 `step()` 中被消费并注入系统提示词。

### 感知来源

| 来源 | 触发 | 示例 |
|------|------|------|
| `User` | 用户发送消息 | "你好" |
| `System` | 系统事件 | 配置变更 |
| `ToolResult` | 工具执行完毕 | "文件读取成功" |
| `Timer` | 30s 心跳 | "定时心跳 — 30秒已过去" |

### 数据结构

```
WorldviewBuffer
    ├── pending: VecDeque<Percept>     ← 待处理感知队列
    ├── recent_slices: VecDeque<PerceptionSlice>  ← 最近 7 个切片
    ├── start_time: SystemTime          ← 用于计算运行时长
    └── max_slices: 7
```

### 世界观生成

`build_worldview()` 每次 `step()` 调用时生成完整的 `<worldview>` 标签：

```xml
<worldview>
当前时间: 2026/6/10 23:03:53（夜间）
已持续运行: 2 小时 15 分钟
[💬 用户] 你好
[⏰ 定时] 定时心跳 — 30秒已过去
</worldview>
```

包含三个部分：

1. **当前时间** — 带时段上下文
   - 5-8时 → 清晨
   - 9-11时 → 上午
   - 12-13时 → 中午
   - 14-17时 → 下午
   - 18-21时 → 傍晚
   - 22-4时 → 夜间

2. **已持续运行** — 从进程启动到现在的时长

3. **近期外部感知** — 最近切片中的用户消息/工具结果/心跳

## 系统提示词组装

每次 LLM 请求前，PromptManager 从 `prompts/*.txt` 组装完整提示词：

```
┌─ 静态部分（编译时确定）─────────────────────────┐
│  1. identity.txt       — Elio 身份宣言           │
│  2. language.txt       — 日语规则                │
│  3. speech_blocks.txt  — <think>/<ja>/<zh> 协议  │
│  4. worldview.txt      — 世界观说明              │
│  5. loyalty.txt        — 忠诚宣言                │
│  6. system_1~5.txt     — 系统指令               │
│  7. doing_tasks_*.txt  — 任务指引               │
│  8. actions.txt        — 行为准则               │
│  9. tools_*.txt        — 工具使用说明           │
│ 10. tone_*.txt         — 语气风格               │
│ 11. efficiency_*.txt   — 输出效率               │
├─ 动态边界 ───────────────────────────────────────┤
├─ 动态部分（运行时注入）───────────────────────────┤
│ 12. <worldview>...</worldview>  ← WorldviewBuffer│
│ 13. 记忆上下文                   ← ContextBridge │
└──────────────────────────────────────────────────┘
```

## 主循环流程（step 方法）

```
step() 被调用
    │
    ├── 对话为空? → 返回 Idle
    │
    ├── worldview.commit_slice()
    │     将所有 pending 感知固化为一个 PerceptionSlice
    │
    ├── build_worldview()
    │     生成时间/运行时长/感知文本
    │
    ├── 组装 system prompt
    │     base_prompt + worldview + 记忆上下文
    │
    ├── 调用 LLM (DeepSeek API)
    │     POST /v1/messages
    │     { system, messages, tools, max_tokens }
    │
    ├── 处理响应
    │     │
    │     ├── Text { text } ──────→ 记录日志 → 返回 Response
    │     │
    │     └── ToolUse { name, input, id }
    │           │
    │           ▼
    │     execute_tool()
    │           │
    │           ├── ToolRegistry::execute()
    │           ├── 结果追加到 Conversation
    │           ├── 记录 FastPath
    │           └── 递归 step()
    │
    └── 返回 StepResult
          ├── Idle        → 无工作
          ├── Response(t) → Elio 回复文本
          ├── ToolCall    → LLM 请求工具（由 execute_tool 处理）
          └── Error(e)    → 错误信息
```

## 30 秒心跳定时器

服务器启动时 `tokio::spawn` 一个 30s 间隔的定时器：

```
每隔 30 秒:
    │
    ├── 1. on_timer_tick()
    │      ├── WorldviewBuffer::push("定时心跳", Timer)
    │      └── Conversation::add_user_message("<system tick>")
    │
    └── 2. step()
           ├── WorldviewBuffer 中有 timer 感知
           ├── Conversation 中有 <system tick>
           ├── build_worldview 包含当前时间/时长
           └── Elio 感知到时间流逝，可以主动说话
```

效果：即使没有用户消息，Elio 每 30 秒也会收到一次世界观更新，感知到时间的流逝。

## 完整消息流（用户对话）

```
用户: "你好"
    │
    ▼
WebSocket 收到 user_message
    │
    ▼
MainLoop::on_user_message("你好")
    ├── Conversation::add_user_message("你好")
    ├── MemorySystem::record_event("你好")
    ├── WorldviewBuffer::push("你好", User)
    └── state = Thinking
    │
    ▼
MainLoop::step()
    ├── build_worldview()
    │   → <worldview> 当前时间: 夜间, 已运行: 2h, [用户] 你好
    ├── assembly system prompt (25 个 .txt + worldview + 记忆)
    ├── POST /v1/messages (DeepSeek API)
    │
    ▼
收到响应: <ja>こんばんは、マスター！</ja>
    │
    ▼
MainLoop::on_response()
    ├── Conversation::add_assistant_text("...")
    ├── MemorySystem::record_event("...")
    ├── logger::log("elio.response", "...")
    └── WebSocket 推送 content_delta + message_complete
```
