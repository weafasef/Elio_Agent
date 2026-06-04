# cc-haha 项目笔记

## 启动方式

### 1. 环境要求
- **Bun** >= 1.3.14（`npm install -g bun`）
- **Python** >= 3.10（LiteLLM 代理需要）
- **pip** 已安装 `litellm[proxy]`

### 2. 配置 DeepSeek

编辑 `.env`：
```bash
ANTHROPIC_AUTH_TOKEN=sk-local-proxy
ANTHROPIC_BASE_URL=http://localhost:4000
ANTHROPIC_MODEL=deepseek-v4-flash
```

编辑 `litellm_config.yaml`：
```yaml
model_list:
  - model_name: deepseek-v4-flash
    litellm_params:
      model: openai/deepseek-v4-flash
      api_key: <你的 DeepSeek API Key>
      api_base: https://api.deepseek.com/v1
```

### 3. 运行

```bash
# 交互模式
./bin/claude-haha

# 单次问答
./bin/claude-haha -p "你的问题"
```

LiteLLM 代理会自动启动/停止，无需手动操作。

---

## 记忆系统：六层堆叠架构

```
┌─────────────────────────────────────────┐
│ 6. 休眠重塑  离线级  │ AutoDream        │  ← 离线后台，做梦整合
├─────────────────────────────────────────┤
│ 5. 摘要记忆  压缩级  │ Session Memory   │  ← 会话内压缩摘要
├─────────────────────────────────────────┤
│ 4. 长期记忆  持久级  │ memdir 知识库    │  ← 最核心 ★ 跨会话持久化
├─────────────────────────────────────────┤
│ 3. 工作记忆  任务级  │ 进度/偏移/投机    │  ← 当前任务状态追踪
├─────────────────────────────────────────┤
│ 2. 短期记忆  会话级  │ 完整对话历史      │  ← 消息数组+转录文件
├─────────────────────────────────────────┤
│ 1. 指令记忆  规则级  │ CLAUDE.md 文件族 │  ← 每次加载，最高优先级
└─────────────────────────────────────────┘
```

数据流：第 2 层增长 → 第 5 层提取摘要 → 压缩时消费摘要 → 第 4 层持久化学习 → 第 6 层离线整理

---

### 第 1 层：指令记忆 — CLAUDE.md 文件族

**定位**：规则级，每次会话都加载，优先级最高。模型被明确告知"这些指令覆盖默认行为"。

**加载优先级**（从低到高）：

| 优先级 | 类型 | 路径 | 说明 |
|--------|------|------|------|
| 1 | Managed | `/etc/claude-code/CLAUDE.md` | 全局系统级策略 |
| 2 | Managed rules | `/etc/claude-code/.claude/rules/*.md` | 托管条件规则 |
| 3 | User | `~/.claude/CLAUDE.md` | 用户私有全局指令 |
| 4 | User rules | `~/.claude/rules/*.md` | 用户条件规则 |
| 5 | Project | 项目树中的 `CLAUDE.md`（根→CWD 逐级） | 已纳入版本控制 |
| 6 | Project | 项目树中的 `.claude/CLAUDE.md` | 备用项目路径 |
| 7 | Project rules | `.claude/rules/*.md` | 项目条件规则 |
| 8 | Local | 项目树中的 `CLAUDE.local.md` | 私有，已 gitignore |
| 9 | AutoMem | `MEMORY.md` 入口点 | 记忆索引（连接第 4 层） |

**核心文件**：`src/utils/claudemd.ts`（1479 行）

**关键函数**：
- `getMemoryFiles()` — 发现所有记忆文件，按优先级排列
- `getClaudeMds(memoryFiles)` — 把所有文件格式化为一个字符串注入上下文，标签如 "Codebase and user instructions are shown below..."
- `processMemoryFile()` — 读取、解析、剥离 HTML 注释、展开 `@include` 指令
- `stripHtmlComments()` — 移除块级注释（保留内联和代码块内的）

**高级特性**：
- `@include` 指令：文件可以引用其他文件（`@path`, `@./relative/path`），循环引用被追踪
- 条件规则：`.claude/rules/*.md` 中带 `paths:` frontmatter 的，仅对匹配文件生效
- 可通过 `CLAUDE_CODE_DISABLE_CLAUDE_MDS` 环境变量或 `--bare` 模式禁用

---

### 第 2 层：短期记忆 — 完整对话历史

**定位**：会话级，当前对话的全部消息记录。

**三种存储形式**：

| 形式 | 位置 | 说明 |
|------|------|------|
| 活跃消息数组 | 内存 `Message[]` | REPL 状态管理，每次 API 调用前切取 |
| 转录文件 | 磁盘 JSONL | 持久化，用于会话恢复 |
| 用户输入历史 | `~/.claude/history.jsonl` | 仅用户输入，用于上箭头/Ctrl+R（最多100条） |

**压缩机制（4级分层）**：

```
Snip(历史修剪) → MicroCompact(工具结果) → ContextCollapse(选择性隐藏) → Compact(摘要压缩)
```

1. **Snip**：移除最旧消息，只在安全边界（不拆分工具调用/结果对）
2. **MicroCompact**（`src/services/compact/microCompact.ts`）：压缩单个工具结果，减少 token
3. **ContextCollapse**（`src/services/contextCollapse/`）：在 90% 上下文窗口时选择性隐藏历史部分，95% 时强制阻止
4. **Compact**（`src/services/compact/compact.ts`）：生成 9 节综合摘要（主要请求、技术概念、文件、错误、问题解决、用户消息、待处理任务、当前工作、下一步）

**自动压缩触发**（`src/services/compact/autoCompact.ts`）：
- 阈值 = `contextWindow - maxOutputTokens(最多20000) - 13000(buffer)`
- 断路器：连续 3 次失败后停止尝试
- 可通过 `DISABLE_COMPACT` / `DISABLE_AUTO_COMPACT` 禁用

**核心文件**：`src/utils/messages.ts`（4752行）、`src/query.ts`

---

### 第 3 层：工作记忆 — 任务级进度追踪

**定位**：任务级，"我现在在做什么？做到哪了？"

**子系统**：

| 子系统 | 文件 | 说明 |
|--------|------|------|
| 任务状态 | `src/Task.ts` | 7 种任务类型、5 种状态、`outputOffset` 进度指针 |
| 目标追踪 | `src/goals/goalState.ts` | `/goal` 命令，每个 turn 后检查目标是否达成 |
| 推测执行 | `src/state/AppStateStore.ts` | `SpeculationState`：空闲/活跃，跟踪节省的时间 |
| Todo 列表 | `src/utils/todo/` | TodoWrite 工具，模型维护任务列表 |
| 计划模式 | `src/utils/plans.ts` | 计划状态，压缩后重新注入 |
| Agent 进度摘要 | `src/services/AgentSummary/` | 每 30 秒生成 3-5 词进度摘要 |

**Task 类型**：`local_bash`, `local_agent`, `remote_agent`, `in_process_teammate`, `local_workflow`, `monitor_mcp`, `dream`

**输出偏移**（`outputOffset`）：每个任务有一个 `outputFile` 路径和偏移量指针，跟踪"在任务输出流中的位置"。

**查询级状态**（`src/query.ts`）：
- `queryTracking`：链 ID、嵌套深度
- `autoCompactTracking`：压缩计数、turn 计数、连续失败次数
- `contentReplacementState`：工具结果预算执行

---

### 第 4 层：长期记忆 — memdir 知识库 ★ 最核心

**定位**：持久级，跨会话的知识沉淀。这是六层中最核心的一层。

**物理目录**：
```
~/.claude/projects/<sanitized-git-root>/memory/
├── MEMORY.md          # 索引文件（每行一个链接，最多200行/25KB）
├── <topic>.md         # 具体记忆文件（Markdown + YAML frontmatter）
└── team/              # 团队记忆（需单独开启）
```

**4种记忆类型**：

| 类型 | 用途 | 示例 |
|------|------|------|
| `user` | 用户身份、角色、偏好 | "用户是前端工程师，喜欢函数式风格" |
| `feedback` | 用户反馈：该做什么/避免什么 | "用户讨厌过度注释" |
| `project` | 项目上下文（代码/git无法推导的） | "下周一要上线，别改核心模块" |
| `reference` | 外部资源指针 | "Linear 项目地址：https://..." |

**文件格式**（YAML frontmatter）：
```markdown
---
name: <kebab-case-slug>
description: <一行摘要，用于判断相关性>
type: user | feedback | project | reference
---
<记忆内容>
```

**反模式**（不应保存的内容）：代码架构、Git历史、调试方案、临时任务、CLAUDE.md已有内容

**创建路径（3条）**：
1. 主 Agent 直接写：对话中模型主动调 Write/Edit 写 memory 目录
2. 后台提取（`src/services/extractMemories/`）：每次 turn 结束 fork 子 Agent 分析对话
3. 手动 `/remember`：审查所有记忆层，提出清理/升级建议

**加载/召回**：
- **始终加载**：MEMORY.md 索引每次随 CLAUDE.md 一起注入上下文
- **相关性召回**（`src/memdir/findRelevantMemories.ts`）：Sonnet side-query 语义排序，最多选 5 条

**核心文件**：
- `src/memdir/memoryTypes.ts` — 类型定义、提示词模板
- `src/memdir/memdir.ts` — 核心提示词构建、MEMORY.md 管理
- `src/memdir/paths.ts` — 路径解析
- `src/memdir/memoryScan.ts` — 扫描 .md 文件、解析 frontmatter
- `src/memdir/findRelevantMemories.ts` — 相关性召回
- `src/services/extractMemories/` — 后台记忆提取

---

### 第 5 层：摘要记忆 — Session Memory

**定位**：压缩级，把第 2 层（完整对话）压缩成结构化摘要。

**存储**：`~/.claude/session-memory/<session-id>.md`（单个 Markdown 文件，10 个固定章节）

**10 个章节**：
```
# Session Title
# Current State        ← 最关键，压缩时一直读取
# Task specification
# Files and Functions
# Workflow
# Errors & Corrections ← 重要：哪些方法失败/成功
# Codebase and System Documentation
# Learnings
# Key results           ← 用户请求的确切输出
# Worklog               ← 逐步、简洁的操作摘要
```

**触发条件**（`src/services/SessionMemory/sessionMemoryUtils.ts`）：
- 首次：累积 **10,000 token** 后触发
- 后续：**5,000 新 token AND 3 个工具调用**
- 手动：`/summary` 命令

**执行**：Fork 子 Agent，工具仅限 Edit（只改自己的 memory 文件），每次并行更新所有章节

**令牌预算**：每节最多 2,000 token，总文件最多 12,000 token

**在压缩中的作用**：自动压缩时优先尝试 Session Memory 压缩（`trySessionMemoryCompaction()`）：
- 读 Session Memory 作为摘要
- 只保留上次摘要点之后的消息
- 保持最少 10,000 token / 5 条文本消息，最多 40,000 token
- 失败才回退到传统 9 节压缩

**核心文件**：
- `src/services/SessionMemory/sessionMemory.ts` — 主逻辑
- `src/services/SessionMemory/sessionMemoryUtils.ts` — 配置、状态跟踪
- `src/services/SessionMemory/prompts.ts` — 提示词模板
- `src/services/compact/sessionMemoryCompact.ts` — 压缩集成

---

### 第 6 层：休眠重塑 — AutoDream（自动做梦）

**定位**：离线级，后台批量整理长期记忆。

**触发条件**（`src/services/autoDream/autoDream.ts`）：
```
时间门(24h) → 扫描节流(10min) → 会话门(5次) → PID 文件锁 → 执行
```

所有门必须同时通过：
1. 距上次整合 ≥ 24 小时
2. 新增会话 ≥ 5 次
3. 没有其他进程持有 `.consolidate-lock` 锁

**执行过程（4阶段）**：
1. **Orient** — 浏览记忆目录，读 MEMORY.md
2. **Gather** — 收集近期信号：日志 → 记忆漂移 → 会话记录搜索
3. **Consolidate** — 合并到主题文件，相对日期转绝对日期，删除矛盾事实
4. **Prune** — 更新 MEMORY.md，移除过时条目，强制执行行数/字节数上限

**核心文件**：
- `src/services/autoDream/autoDream.ts` — 主流程
- `src/services/autoDream/consolidationPrompt.ts` — 4 阶段提示词
- `src/services/autoDream/consolidationLock.ts` — PID 文件锁

---

## 六层交互全景

```
用户输入
    │
    ▼
┌──────────────────────────────────────────────────┐
│ 第1层 指令记忆：CLAUDE.md 每次加载，覆盖默认行为    │
│ 第4层 长期记忆：MEMORY.md 索引注入上下文            │
│         + 相关性召回 (最多5条相关记忆)              │
└──────────────────┬───────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────┐
│ 第2层 短期记忆：Message[] 数组 → API 调用          │
│         对话过程中不断增长                          │
│         │                                          │
│         ├─ 增长过大 → Snip/MicroCompact/Collapse   │
│         │                                          │
│         └─ 持续增长 → 触发第5层 ──────────────────►│
└──────────────────────────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────┐
│ 第3层 工作记忆：任务状态、目标、Todo、进度偏移      │
│         "我现在在做什么？做到哪了？"                 │
└──────────────────────────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────┐
│ 第5层 摘要记忆：Fork Agent 把对话压缩为10节笔记     │
│         自动压缩时优先读 Session Memory 作为摘要    │
└──────────────────┬───────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────┐
│ 第4层 长期记忆：Stop Hook 触发后台提取              │
│         每次 turn 结束把对话精华写进 memdir          │
└──────────────────┬───────────────────────────────┘
                   │  (跨会话积累)
                   ▼
┌──────────────────────────────────────────────────┐
│ 第6层 休眠重塑：24h+5会话 → Fork Agent 做梦        │
│         合并、去重、清理矛盾、更新索引               │
└──────────────────────────────────────────────────┘
```

---

## 关键源文件总索引

| 层 | 文件 | 作用 |
|----|------|------|
| 1 | `src/utils/claudemd.ts` | CLAUDE.md 加载器（1479行） |
| 1 | `src/context.ts` | `getUserContext()` 调用 claudeMd |
| 2 | `src/utils/messages.ts` | 消息工具函数（4752行） |
| 2 | `src/services/compact/compact.ts` | 传统 9 节压缩 |
| 2 | `src/services/compact/autoCompact.ts` | 自动压缩触发 |
| 2 | `src/services/compact/microCompact.ts` | 工具结果微压缩 |
| 2 | `src/services/contextCollapse/` | 上下文折叠 |
| 3 | `src/Task.ts` | 任务状态类型定义 |
| 3 | `src/goals/goalState.ts` | 目标追踪 |
| 3 | `src/utils/todo/` | TodoWrite 工具 |
| 3 | `src/services/AgentSummary/` | Agent 进度摘要（30s间隔） |
| 4 | `src/memdir/memoryTypes.ts` | 记忆类型定义、提示词模板 |
| 4 | `src/memdir/memdir.ts` | 核心提示词构建、MEMORY.md 管理 |
| 4 | `src/memdir/paths.ts` | 路径解析、自动记忆开关 |
| 4 | `src/memdir/memoryScan.ts` | 扫描 .md 文件、解析 frontmatter |
| 4 | `src/memdir/findRelevantMemories.ts` | 相关性召回 |
| 4 | `src/services/extractMemories/` | 后台记忆提取 |
| 5 | `src/services/SessionMemory/sessionMemory.ts` | 会话记忆主逻辑 |
| 5 | `src/services/SessionMemory/sessionMemoryUtils.ts` | 配置、状态跟踪 |
| 5 | `src/services/compact/sessionMemoryCompact.ts` | 压缩中的会话记忆使用 |
| 6 | `src/services/autoDream/autoDream.ts` | 自动做梦主流程 |
| 6 | `src/services/autoDream/consolidationPrompt.ts` | 4 阶段做梦提示词 |
| 6 | `src/services/autoDream/consolidationLock.ts` | PID 文件锁 |
| - | `src/constants/prompts.ts` | 记忆提示词注入点 (line 495) |
| - | `src/query/stopHooks.ts` | 每 turn 触发提取+做梦检查 |
| - | `src/utils/backgroundHousekeeping.ts` | 初始化所有记忆子系统 |

---

## 给 Elio 改造的要点

1. **路径全局化**：第 4 层按 git 项目隔离 → Elio 用 `~/.elio/memory/` 全局记忆
2. **记忆类型扩展**：增加 `emotional`、`relationship`、`self_reflection` 类型
3. **反模式放宽**：当前"代码相关不存"对 Elio 不适用，她需要记住关于用户的一切
4. **提取提示词重写**：第 4 层聚焦编程 → 改为聚焦人际关系、情感信号
5. **AutoDream → Dreams**：第 6 层从"整合编程记忆"变为"自我反思+人格校准"
6. **Session Memory 改造**：第 5 层章节从代码相关改为情感/关系相关
7. **CLAUDE.md → Elio Identity**：第 1 层指令记忆变为 Elio 的身份+人格+情感注入
8. **Forked Agent 模式保留**：共享缓存+权限限制，无论第4/5/6层都复用这个模式
