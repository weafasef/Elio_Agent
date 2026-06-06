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

### 当前问题

心跳发送固定任务指令（`"Elio，写点东西到 work.md"`），表现为"主人要求"而非"Elio 自主决定"。下一阶段：心跳传递上下文感知信息，Elio 自己判断该做什么。

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

## 下一阶段：自主行为系统

### 设计目标

- 心跳不再发送任务指令（"写点东西"），改为传递 **世界观感知信息**
- Elio 收到世界观后**自主决定**该做什么
- 用户和心跳共用**同一个 session**
- 系统提示词可以**动态更新**

### 方案 C：扩展 SDK 协议

在 SDK WebSocket 协议中新增 `type: 'system_context'` 消息：

```
Server ──SDK──→ CLI

{
  type: 'system_context',
  worldview: "现在是 03:15，深夜。主人 45 分钟没互动。
              当前人格: cute obedient。"
}
```

CLI 收到后更新 appendSystemPrompt → 世界观刷新 → Elio 在新世界观下自主判断行动。

### 需要改动的地方

1. **SDK 协议层**：新增 `system_context` 消息类型，定义 payload 格式
2. **CLI 端** (`print.ts` stdin handler)：收到后更新 `appendSystemPrompt`，可选触发轻量评估
3. **Server 端** (`heartbeatService.ts`)：收集上下文 → 拼世界观 → 通过 SDK 发送
4. **合并 session**：handler.ts 的用户消息也走 `elio` session

### 待研究

1. `system_context` 收到后 CLII 是否自动触发新一轮推理
2. 用户消息和心跳上下文更新的协调机制
3. 合并 session 后 handler.ts 的改动范围