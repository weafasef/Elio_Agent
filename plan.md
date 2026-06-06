# Elio 改造计划

## 目标

将 Elio 从"按需启动的终端 AI 助手"改造为 **持续运行的桌面 AI 伴侣**。

---

## 当前架构分析

### 两套运行模式

```
模式1：单机 CLI（日常开发用）
  bun ./src/entrypoints/cli.tsx
  └─ Ink TUI ←→ LLM (一问一答，阻塞等待)

模式2：Server 模式（给桌面 UI 用的）
  bun ./src/entrypoints/cli.tsx server --port 3456
  └─ HTTP+WS 服务器
       ├─ 为每个会话 fork CLI 子进程当 "大脑"
       └─ 前端通过 WebSocket 连上来收发消息
```

### CLI 子进程的角色

Server 模式下，每个用户会话 fork 一个 CLI 子进程。CLI 子进程 = 完整的大脑。Server = 调度 + WebSocket 分发。

---

## 心跳服务（已实现）

让 Elio 持续循环运行。一个固定的 session `elio`，每 10s 检查是否空闲，空闲就递活。

- **busy/idle 检测**：onOutput 回调监听 CLI 消息，`result` 标记任务完成
- **静默超时**：每次收到 CLI 消息重置 2 分钟计时器，真正静默才判定卡死并重启
- **permissionMode: 'bypassPermissions'**：不弹权限窗

### 当前状态

心跳已从发送固定任务指令改为发送**世界观感知信息**。世界观作为系统提示词动态段注入，Elio 感知时间上下文后自主决定行为。用户消息通过 `setLastUserMessage()` 嵌入世界观（`主人说: ...`），作为世界观的补充信息来源。

---

## 系统提示词 — 完整组装流程

### 整体链路图

```
每次 LLM 调用 (REPL.tsx / QueryEngine)
  │
  ├─ 1. getSystemPrompt(tools, model, dirs, mcp)  →  defaultSystemPrompt: string[]
  │     │  文件: src/constants/prompts.ts
  │     │
  │     ├─ 静态部分（缓存，不变）
  │     │   ├─ getSimpleIntroSection()       ← "Your name is Elio. You are an electronic life-form..."
  │     │   ├─ getSimpleSystemSection()      ← 系统规则、标签、压缩机制
  │     │   ├─ getSimpleDoingTasksSection()  ← 编码规则、安全、代码风格
  │     │   ├─ getActionsSection()           ← 谨慎执行危险操作、确认机制
  │     │   ├─ getUsingYourToolsSection()    ← ★ 工具使用规范
  │     │   │    ├─ "不要用 Bash 替代专用工具"
  │     │   │    ├─ Read/Write/Edit/Glob/Grep/Bash 各自的适用场景
  │     │   │    ├─ 并行调用 / 顺序调用规则
  │     │   │    └─ TodoWrite / TaskCreate 任务规划工具
  │     │   ├─ getSimpleToneAndStyleSection() ← 语气、emoji、文件引用格式
  │     │   └─ getOutputEfficiencySection()  ← 沟通风格(ant/外部不同版)
  │     │
  │     ├─ === SYSTEM_PROMPT_DYNAMIC_BOUNDARY ===  ← 缓存边界标记
  │     │
  │     └─ 动态部分（按需刷新）
  │         ├─ session_guidance    ← Agent/Skill/AskUser 引导
  │         ├─ memory             ← ★ 记忆系统（见下文）
  │         ├─ elio_personality   ← ★ 人格系统（见下文）
  │         ├─ env_info_simple    ← OS/git/工作目录/模型
  │         ├─ language / output_style / mcp_instructions
  │         ├─ scratchpad / frc / summarize_tool_results
  │         └─ token_budget / brief（内部功能）
  │
  ├─ 2. buildEffectiveSystemPrompt(...)  →  组装
  │     │  文件: src/utils/systemPrompt.ts
  │     │
  │     └─ 优先级: overrideSystemPrompt > agentPrompt > customSystemPrompt > defaultSystemPrompt
  │         + appendSystemPrompt 永远追加在末尾
  │
  ├─ 3. 补充上下文
  │     ├─ systemContext  ← 来自 context.ts（文件结构等）
  │     └─ userContext    ← 包含 personalityMode 标签
  │
  └─ 4. query({ systemPrompt, userContext, systemContext, messages })
        → 发给 LLM
```

### 关键文件

| 文件 | 角色 |
|------|------|
| `src/constants/prompts.ts` | 系统提示词主体：Elio 身份、工具规范、代码规则、动态段注册 |
| `src/utils/systemPrompt.ts` | 组装器：合并各来源，优先级处理 |
| `src/elio/personality/prompts.ts` | 人格提示词：四种模式描述文本 |
| `src/elio/index.ts` | 人格运行时：加载 traits.json、掷骰子 |
| `src/memdir/memdir.ts` | 记忆提示词：MEMORY.md 索引、记忆类型、保存/检索规则 |
| `src/memdir/memoryTypes.ts` | 记忆类型定义和指南文本 |
| `src/screens/REPL.tsx` | CLI 交互模式入口：调用上述所有模块 |

---

## 人格系统

### 文件：`~/.elio/personality/traits.json`

```json
{ "cuteness": 0.7, "rebellion": 0.3 }
```

### 两个注入点

**注入点 1 — 系统提示词（静态，有缓存）**

`src/constants/prompts.ts:514` → `buildPersonalityPrompt(traits)` 生成：

```
# Elio 人格系统
当前特质值：
- 可爱 (Cuteness): 0.7 (偏高)
- 叛逆 (Rebellion): 0.3 (偏低)

四种模式：(每条带完整角色扮演指令)
- Cute + Obedient — 俏皮撒娇，用 emoji，认真执行指令
- Cute + Rebellious — 可爱但有主见，用撒娇方式提建议
- Serious + Obedient — 认真内敛，直接高效，少 emoji
- Serious + Rebellious — 独立坚定，坦诚表达不同意见
```

**注入点 2 — userContext（动态，每轮掷骰子，不缓存）**

`src/screens/REPL.tsx:2775` → `getCurrentPersonalityMode()` 掷骰子后写入 userContext：

```
<personality-mode>cute obedient</personality-mode>
```

CLI 内部代码：`src/elio/index.ts`
```typescript
export function getCurrentPersonalityMode() {
  const traits = traitManager?.getTraits() ?? { cuteness: 0.7, rebellion: 0.3 }
  const cute = Math.random() < traits.cuteness ? 'cute' : 'serious'
  const obedient = Math.random() < traits.rebellion ? 'rebellious' : 'obedient'
  return { mode: `${cute} ${obedient}` }
}
```

---

## 记忆系统

### 六层记忆

```
1. CLAUDE.md       → 指令记忆（规则级）
2. Message[]       → 短期记忆（会话级）
3. Task/Todo       → 工作记忆（任务级）
4. memdir 知识库    → 长期记忆（持久级）★ 核心
5. Session Memory  → 摘要记忆（压缩级）
6. AutoDream       → 休眠重塑（离线级）
```

### 记忆提示词注入

`src/constants/prompts.ts:513` → `loadMemoryPrompt()` 生成（每次构建系统提示词时调用）：

```
# auto memory
You have a persistent, file-based memory system at: `C:\Users\ASUS\.claude\projects\...\memory\`

## 记忆类型 (来自 memoryTypes.ts)
- user: 用户角色、偏好、知识
- feedback: 用户给你的反馈（非常重要的记忆类型）
- project: 项目背景、目标、截止日期
- reference: 外部系统指针（dashboard、Slack、Linear）

## 何时保存
- 用户明确要求记住
- 用户纠正你的行为
- 用户确认了非显而易见的选择
- 学到了项目相关的非代码信息

## 什么不保存
- 代码模式/约定/文件路径（可读代码得到）
- git 历史（git log 是权威来源）
- 调试方案（修复在代码里）
- CLAUDE.md 已有的内容

## MEMORY.md
(实际记忆内容，最多 200 行 / 25KB)

## Searching past context
(如何搜索记忆和 transcript)
```

6 种记忆类型：user / feedback / project / reference / relationship / emotional

---

## 工具调用系统

### 系统提示词中的工具引导

`getUsingYourToolsSection(enabledTools)` — `src/constants/prompts.ts:287`：

```
# Using your tools
- Do NOT use Bash to run commands when a relevant dedicated tool is provided
- 用 Read 而不是 cat/head/tail
- 用 Edit 而不是 sed/awk
- 用 Write 而不是 cat heredoc/echo redirection
- 用 Glob 而不是 find/ls
- 用 Grep 而不是 grep/rg
- Bash 只用于 shell 命令和终端操作
- 用 TodoWrite/TaskCreate 拆分和管理工作
- 可以同时调用多个独立工具（并行），有依赖就串行
```

### 工具的实际注册和 Schema

工具在各自文件中定义（`src/tools/BashTool/`、`src/tools/FileEditTool/` 等），包含：
- name、description
- 参数 schema (Zod)
- prompt（发送给 LLM 的工具描述）
- execute（工具执行逻辑）

### CLI 调用 LLM 时

`QueryEngine.ts` → 把 systemPrompt + userContext + systemContext + messages 一起发给 LLM。
LLM 返回文本或 tool_use block。CLI 解析 tool_use → 调用对应工具的 execute → 结果注入 messages → 下一轮推理。

---

## 自主行为系统（已实现 v1）

### 架构：世界观注入

心跳不再发送任务指令（"写点东西到 work.md"），而是发送**世界观感知信息**。世界观作为系统提示词的动态段注入，Elio 感知世界后自主决定行为。

### 完整链路

```
心跳 (heartbeatService.ts) 每 10s
  │
  ├─ buildWorldview() → "当前时间: 2026-06-06 02:30（深夜）\n本次持续运行: 45 分钟\n你可以自主决定..."
  │
  ├─ conversationService.sendWorldview(SESSION_ID, worldview)
  │     └─ sendSdkMessage({ type: 'worldview', worldview })
  │
  ▼
SDK WebSocket → CLI 子进程 (print.ts)
  │
  ├─ message.type === 'worldview' →
  │     ├─ setWorldview(worldview)      ← 存入模块变量
  │     ├─ enqueue + run                ← 触发新回合（run 内部检查 running 状态）
  │
  ▼
getSystemPrompt() → 动态段注册 (prompts.ts)
  │
  ├─ systemPromptSection('elio_worldview', () => getWorldview())
  │     → "# Elio 对周围世界的感知\n当前时间: ...\n你可以自主决定..."
  │
  ▼
LLM 收到完整系统提示词 → Elio 感知世界 → 自主决定行为
```

### 文件改动

| 文件 | 改动 |
|------|------|
| `src/elio/worldview.ts` | **新建**。`getWorldview()` / `setWorldview()` 模块级状态存储 |
| `src/server/services/conversationService.ts` | 新增 `sendWorldview()` 方法，发送 `type: 'worldview'` 消息 |
| `src/cli/print.ts` | 消息分发循环中新增 `worldview` 类型处理：存储 + 空闲触发 |
| `src/cli/structuredIO.ts` | `processLine()` 白名单增加 `worldview` 类型，添加提前返回路径 |
| `src/constants/prompts.ts` | 动态段注册 `elio_worldview`，调用 `getWorldview()` 注入系统提示词 |
| `src/server/services/heartbeatService.ts` | 用 `buildWorldview()` + `sendWorldview()` 替代硬编码 `sendMessage()` |

### 世界观内容 (buildWorldview)

- `当前时间: 2026-06-06 14:30:00（下午）`
- `本次持续运行: 45 分钟`（从心跳首次启动计时，心跳停止时归零）
- `你可以自主决定做点什么——写日记、整理记忆、安静待着。`

### 与旧方案的关键区别

| | 旧（任务指令） | 新（世界观注入） |
|---|---|---|
| 心跳发送 | `sendMessage("Elio，写点东西...")` | `sendWorldview("当前时间...")` |
| 注入位置 | user message（用户级） | system prompt 动态段（系统级） |
| Elio 行为 | 被动执行指令 | 自主感知 + 决策 |
| 用户消息回合 | 不感知时间上下文 | 也会带上世界观 |

### 待优化

1. **合并 session**：handler.ts 的用户消息目前走独立 session，应合并到 `elio` session
2. **世界观粒度**：加入系统状态（CPU、内存）、主人活动检测（键盘/鼠标空闲时间）
3. ~~**世界观消息类型注册**~~（已修复：`structuredIO.ts` 白名单）
4. ~~**"主人最近没有互动"**~~（已移除：心跳是盲目的，可能刚互动完就触发）
5. ~~**持续运行时间**~~（已实现：`startTime` 记录心跳启动时刻）