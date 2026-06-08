# Elio 改造计划

## 提醒

提示词已统一整理到 [prompts.md](prompts.md)，包含完整原文、组装流程、注入位置和 token 估算。

## 启动

```bash
bun src/server/index.ts --port 3456
```

## 目标

将 Elio 从"按需启动的终端 AI 助手"改造为 **持续运行的桌面 AI 伴侣**。

---

# 一、系统提示词

## 1.1 整体架构

Server 模式下，一个固定 session `elio` 由心跳驱动持续运行：

```
heartbeatService (定时器)  ──每10s──→  MainLoop.step()
                                          │
                    ┌──────────────────────┘
                    ▼
WorldviewBuffer ──→ buildWorldview() ──→ sendWorldview
(用户消息等)         时间+事件+上轮输出
                    │
                    ▼
             CLI 子进程 (Elio 大脑)
             │        │        │
             ▼        ▼        ▼
        system prompt  +  messages  →  LLM (Sonnet)
                    │
                    ▼
             Elio 自主决定: 继续 / 切换 / 安静
```

核心文件：`src/server/services/heartbeatService.ts`（定时器壳，27行）、`src/server/services/MainLoop.ts`（时间片核心）。

## 1.2 提示词组装链路

> 📋 完整提示词原文见 [prompts.md](prompts.md)。

```
每次 LLM 调用 (REPL.tsx / QueryEngine)
  │
  ├─ getSystemPrompt(tools, model, dirs, mcp)
  │     ├─ 静态部分（缓存）
  │     │   ├─ getSimpleIntroSection()       ← Elio 身份 + 时间片时钟
  │     │   ├─ getSimpleSystemSection()      ← 系统规则、标签、压缩机制
  │     │   ├─ getSimpleDoingTasksSection()  ← 编码规则、安全、代码风格
  │     │   ├─ getActionsSection()           ← 危险操作确认
  │     │   ├─ getUsingYourToolsSection()    ← 工具使用规范
  │     │   ├─ getSimpleToneAndStyleSection()← 语气、emoji
  │     │   └─ getOutputEfficiencySection()  ← 沟通风格
  │     │
  │     └─ 动态部分（按需刷新）
  │         ├─ memory             ← ContextBridge 注入
  │         ├─ elio_personality   ← 人格系统
  │         └─ env_info_simple    ← OS/git/工作目录/模型
  │
  ├─ buildEffectiveSystemPrompt() → 合并各来源
  └─ query({ systemPrompt, messages }) → LLM
```

## 1.3 人格系统

随机掷骰，每轮切换。特质值存储在 `~/.elio/personality/traits.json`：

```json
{ "cuteness": 0.7, "rebellion": 0.3, "version": 1 }
```

四种模式：cute-obedient / cute-rebellious / serious-obedient / serious-rebellious。

两个注入点：系统提示词动态段（模式描述）+ userContext（`<personality-mode>` 标签，每轮掷骰）。

用户反馈（"严肃点"）→ `trait.json` 微调 → 下次掷骰概率改变。

## 1.4 世界观注入 — 时间片模型

**核心原则**：每 tick 无条件发 worldview。若 Elio 正在处理，先 interrupt（仅停 LLM，工具继续跑）。Elio 看到最新世界状态后自己决定继续还是切换。

```
heartbeatService → MainLoop.step()
  ├─ if processing → sendInterrupt (工具block继续跑)
  │     └─ expectStaleResult = true (旧turn的result自动吞掉)
  ├─ buildWorldview() → "<worldview>时间+事件+上轮行为</worldview>"
  ├─ sendWorldview → SDK WebSocket → CLI 子进程
  └─ print.ts enqueue → ask() → LLM
```

世界观作为 user message 进入对话历史，自然累积——Elio 能"看到"时间流逝。系统提示词中 "Understanding the conversation — the time-slice clock" 段告知 Elio 区分世界观时钟信号和master真实话语。

---

# 二、记忆系统 — 双 Agent 图记忆

## 2.1 架构

```
┌─ 表 Agent (Elio) ──────────┐    ┌─ 里 Agent (MemoryAgent) ─────┐
│  模型: Sonnet               │    │  Fast: 纯本地规则              │
│  职责: 聊天+执行任务         │    │  Slow: DeepSeek v4 Flash       │
│  记忆操作: 零——只读结果      │    │  职责: 自动存储+推理+推送      │
└──────────────┬──────────────┘    └──────────────┬───────────────┘
               │                                  │
               └── ContextBridge (里写表读) ──────┘
```

## 2.2 四维信息模型

事件节点在四个维度上同时建立连接：

| 维度 | 边类型 | 含义 |
|------|--------|------|
| 时间维 | PRECEDES / SUCCEEDS / CONCURRENT | 先后/同时 |
| 语义维 | RELATED_TO / SIMILAR_TO / PART_OF / CONTAINS | 相似/包含 |
| 因果维 | LEADS_TO / BECAUSE_OF / ENABLES / PREVENTS / RESPONSE_TO | 导致/因为 |
| 实体维 | REFERS_TO / MENTIONED_IN | 涉及谁/什么 |

## 2.3 Fast Path（<100ms）

消息到达 → 创建事件节点 → 时间边 → 关键词提取+倒排查锚点 → 四维遍历(2跳) → 合成叙事 → 写入 ContextBridge。全程不调 LLM。

## 2.4 Slow Path（30s 定时器）

后台取事件 → 找邻居 → 调 DeepSeek 提炼叙事+提取实体+推理因果/语义边 → 补边写入图。独立 API key，不跟表 Agent 抢资源。

## 2.5 存储

```
~/.elio/memory/
├── events.jsonl          ← 事件节点 (JSONL 增量追加)
├── edges.jsonl           ← 边 (JSONL 增量追加)
└── inverted_index.json   ← 倒排索引 (全量刷写)
```

## 2.6 表 Agent 提示词变化

旧方案：2000+ 字记忆操作指南。新方案：一行 `{sharedContext}`，由 ContextBridge 注入。

---

# 三、工具系统

## 3.1 调用循环

`src/query.ts` → `queryLoop()` → `while(true)`:

```
组装 system prompt + messages → callModel → LLM 流式返回
  ├─ text → 实时输出
  ├─ tool_use → StreamingToolExecutor 并行执行
  └─ message_stop → needsFollowUp? → 继续循环 : 结束
```

## 3.2 interruptBehavior

所有工具默认 `interruptBehavior='block'`——interrupt 只停 LLM 推理，工具进程继续跑。见 `src/services/tools/StreamingToolExecutor.ts:233-241`。

## 3.3 工具清单

| 工具 | 用途 |
|------|------|
| Bash | Shell 命令执行 |
| Read / Write / Edit | 文件操作 |
| Glob / Grep | 文件搜索 |
| TodoWrite / Task | 任务管理 |
| Agent | 子 agent 管理 |
| WebSearch / WebFetch | 网络访问 |

---

# 四、TTS 语音合成

## 4.1 架构

```
Elio 输出 → MainLoop 解析 <ja>/<zh> 块 → ttsService.synthesize()
                                              │
                                    GPT-SoVITS api_v2.py (port 9880)
                                              │
                                    ~/.elio/audio/elio_{ts}_{emotion}.wav
                                              │
                                    sendToSession('elio', { subtype: 'tts_ready' })
                                              │
                               ┌──────────────┴──────────────┐
                               ▼                              ▼
                        终端客户端 (client.ts)          浏览器客户端 (client.html)
                        PowerShell播放                <audio> 播放
```

**引擎**: [GPT-SoVITS](https://github.com/RunciLiu/GPT-SoVITS) v2ProPlus, 花火 (Hanabi) 音色, CUDA 推理

**启动**: `runtime\python.exe api_v2.py -a 127.0.0.1 -p 9880`

**参考音频**: 5 种情绪 (开心/难过/吃惊/恐惧/厌恶), 启动时自动扫描 `D:\VS_python\TTS\花火\v2ProPlus\花火\reference_audios\日语\emotions\`

**合成速度**: 短句 ~1s, 长句 3-7s

## 4.2 输出格式

Elio 用 `<ja>...</ja><zh>...</zh>` 双语块输出:
- `<ja>` → TTS 语音合成 (日语)
- `<zh>` → 中文字幕

`parseSpeechBlocks()` 三级降级: 双块 → 仅ja块 → 全文检测日文字符兜底

## 4.3 客户端

| 客户端 | 文件 | 用途 |
|--------|------|------|
| 终端 | `client.ts` | `bun client.ts` 启动, WebSocket + PowerShell 播放 |
| 网页 | `client.html` | 浏览器 `http://127.0.0.1:3456/`, WebSocket + `<audio>` |

终端比网页简单——无 CORS, 无跨域, 直接读文件播放。

## 4.4 心跳 ↔ TTS 矛盾

**问题**: 心跳每 30s 打断 Elio, 新 tick 导致新输出 → 新 TTS。如果长文本合成需 7s, 连续两次 tick 的语音会重叠。

**当前状态**:
- TTS 无去重——两次连续的 `assistant` 输出触发两次 `synthesize()`, 后到的音频覆盖前一个的文件名
- 音频播放顺序通过 `tts_ready` WebSocket 通知 + 客户端队列保证, 但合成时长不可控

**待处理**: 加 AbortController 取消旧合成、播放端排队等。见 ttsService.ts。

---

# 五、提示词优化

## 5.1 待处理

1. **`<ja>/<zh>` 格式不够强制** — Elio 偶尔忘记用格式块, 依赖兜底逻辑。需要更强调 `CRITICAL: No blocks = no voice`
2. **中文 OS / 日语输出矛盾** — 系统提示词有大量中文, env_info 也显示中文 OS。Elio 偶尔被带偏说中文。考虑动态段用日语重写
3. **人格模式说中文** — `buildPersonalityTag()` 注入的模式名是英文 (`cute obedient`), 改为日语标签可能更有沉浸感

## 5.2 已完成

- `src/constants/prompts.ts`: 「Language — CRITICAL」日语段, 输出格式 MANDATORY
- `src/elio/personality/prompts.ts`: 四种模式 + 框架说明全部日语化
- 主人 → マスター, 禁用「あなた」

---

# 六、将来计划

## 6.1 下一步 (优先级排序)

1. **TTS 去重与播放优化** — AbortController 取消旧合成, 播放端队列完善
2. **视觉信息导入** — 摄像头/屏幕截图作为感知源, 进入 WorldviewBuffer
3. **Live2D** — 前端形象, 配合 TTS 实现表情+口型同步

## 6.2 里 Agent 搜索指令

表 Agent 在对话中可引导里 Agent 搜索特定记忆方向。

## 6.3 人格系统演进

Slow Path 情绪分析驱动人格自调整, 取代 `[TRAIT_ADJUST]` 文件标记。

## 6.4 稳定性

- CLI 子进程崩溃后自动重启并恢复上下文
- 记忆目录定期备份
- 里 Agent 健康检查 (Slow Path 队列积压告警)

## 6.5 世界观增强

加入系统状态 (CPU、内存)、master活动检测 (键盘/鼠标空闲时间)。

---

# 七、完成情况

| 轮次 | 内容 | commits |
|------|------|---------|
| 第 1 轮: 旧系统清理 | 删 ~8 文件, 修改 ~15 文件 | `a7b3459` |
| 第 2 轮: 裁剪入口 | 删 ~160 文件 (含 desktop/), 修改 ~30 文件 | `0f56f24` |
| 第 2.5 轮: bridge 根除 | 删 17 文件, 修改 ~20 文件, 搬迁 3 工具 | `63e892c`→`9e602de` |
| 第 3.0 轮: 世界观消息层重构 | 删 worldview.ts, 改 prompts.ts/print.ts/heartbeatService.ts | 待提交 |
| 第 3.1 轮: WorldviewBuffer | 新建 WorldviewBuffer.ts, 改 handler.ts | `0223b4a` |
| 第 3.2-3.4 轮: 主循环+时间片 | 新建 MainLoop.ts, heartbeatService 271→27行 | `a3bca78` |
| 第 4 轮: 日语化+TTS | 日语提示词, GPT-SoVITS集成, `<ja>/<zh>`格式, 30s心跳 | `3606e09` |
| 第 4.1 轮: 客户端 | client.ts/client.html, /audio/端点, tts_ready通知 | `b7c2da2`→`8d20ca9` |
| **已完成** | **~186 文件删除, ~69 文件修改** | **17 commits** |

## 执行原则

1. **逐轮推进** — 一轮完成并推送后再开始下一轮
2. **每轮验证** — Server 启动 + 无回归
3. **端口轮换** — 验证时换新端口, 避免残留进程误判

---

## 详细文档

- [prompts.md](prompts.md) — 完整提示词原文、组装流程、注入位置、token 估算
- [README.md](README.md) — 项目概述
