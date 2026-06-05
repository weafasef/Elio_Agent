# cc-haha (Elio) 项目文件全地图

> 生成日期：2026-06-05 | 项目版本：999.0.0-local

## 项目概述

cc-haha 是 Claude Code CLI 的本地分支，已改造为 **Elio — 电子生命体**。

- **技术栈**：TypeScript + React/Ink (终端UI) + Bun 运行时
- **入口**：`bin/claude-haha` → `bun ./src/entrypoints/cli.tsx` → `src/main.tsx`
- **双模式**：CLI (Ink TUI) + Desktop Server (HTTP+WS, 端口3456)
- **核心改造**：Elio单会话、人格系统、全局记忆(~/.elio/)

```
cc-haha/
├── bin/claude-haha          # Bash 启动器
├── src/                     # 核心源码 (~60+ 子目录)
├── adapters/                # VSCode/JetBrains IDE 适配器
├── desktop/                 # Electron 桌面应用
├── scripts/                 # CI/质量门脚本
├── docs/                    # VitePress 文档
├── runtime/                 # 沙箱运行时
├── stubs/                   # 原生模块类型桩
├── tests/                   # 端到端测试
└── fixtures/                # 测试固件
```

---

## 一、启动流程（从入口到 REPL）

### 1.1 bin/claude-haha
**Bash 包装器**。自动管理 LiteLLM 代理（DeepSeek 路由，PID 文件 `/tmp/cc-haha-litellm.pid`），仅在交互模式启动/停止。设置 `CALLER_DIR` 环境变量，支持 `CC_HAHA_SKIP_DOTENV` 和 `CLAUDE_CODE_FORCE_RECOVERY_CLI` 降级。

### 1.2 src/entrypoints/cli.tsx
**快速路径调度器**。检测特殊参数绕过完整 UI 加载：
- `--version`, `--dump-system-prompt`, `--claude-in-chrome-mcp`, `--chrome-native-host`
- `daemon`, `remote-control/rc/bridge`, `ps|logs|attach|kill|--bg`
- `environment-runner`, `self-hosted-runner`, `--worktree --tmux`
- 否则 → 动态 `import('../main.js')` → `main()`

### 1.3 src/main.tsx （~4522 行）
**主入口组件和 CLI 定义**。启动流程：

```
startMdmRawRead() + startKeychainPrefetch()  ← 并行预取
    ↓
Commander.js 解析 CLI 参数
    ↓
preAction: init() → 加载遥测 → 运行迁移(11个) → loadRemoteManagedSettings + loadPolicyLimits
    ↓
setup() 并行: getCommands() + getAgentDefinitionsWithOverrides()
    ↓
解析 MCP 配置 → 模型解析 → 权限设置
    ↓
交互式: loginOrRunInteractive() → renderAndRun(REPL)
非交互式: runHeadless() 或 runDelegatedOutputFormat()
    ↓
[Elio单会话] 自动 loadConversationForResume() → processResumedConversation()
```

**关键导入依赖** （~180+ 个 import 语句）：
- `src/bootstrap/state.ts` — 全局状态
- `src/context.ts` — 系统+用户上下文
- `src/commands.ts` — 命令注册表
- `src/tools.ts` — 工具注册表
- `src/services/` — 所有后台服务
- `src/utils/` — 工具函数层

### 1.4 src/entrypoints/init.ts
**启动初始化**（memoized）。按顺序：启用配置 → 安全环境变量 → 优雅关闭 → 1P事件日志+GrowthBook → OAuth预填充 → JetBrains检测 → 远程管理设置 → mTLS+代理+HTTP代理 → TCP+TLS预连接 → scratchpad + LSP初始化。导出 `initializeTelemetryAfterTrust()` 用于延迟加载 OpenTelemetry (~400KB)。

### 1.5 src/entrypoints/mcp.ts
**MCP 服务器入口**。创建 MCP SDK Server（`claude/tengu`），将工具注册为 MCP tools，通过 stdio 传输。

---

## 二、核心架构层

### 2.1 引导状态 — src/bootstrap/state.ts （1794行）

**全局可变状态中心**，导入 DAG 的叶子节点。通过 getter/setter 函数访问，避免循环依赖。

**核心状态字段**：
| 类别 | 字段 | 说明 |
|------|------|------|
| CWD/项目 | `originalCwd`, `projectRoot`, `cwd` | 工作目录 |
| 会话 | `sessionId`（SHA256(cwd)→UUID v5）, `parentSessionId` | Elio单会话确定性ID |
| 模型 | `mainLoopModelOverride`, `modelStrings`, `sdkBetas` | 模型控制 |
| 成本 | `totalCostUSD`, `totalAPIDuration`, `totalToolDuration` | 用量追踪 |
| 客户端 | `clientType`, `isInteractive`, `sessionSource` | 客户端类型 |
| 远程 | `isRemoteMode`, `replBridgeActive`, `directConnectServerUrl` | 远程/桥接 |
| 权限 | `sessionBypassPermissionsMode`, `allowedSettingSources` | 权限控制 |
| 遥测 | `meter`, `loggerProvider`, `tracerProvider` | OTel 实例 |
| 提示缓存 | `promptCache1hAllowlist`, `promptCache1hEligible` | 缓存优化 |

**~140 个导出函数**，包括：getter/setter、成本累计、会话切换 (`switchSession()`)、OTel 设置、计划模式/自动模式转换、测试重置 (`resetStateForTests()`)。

### 2.2 类型系统 — src/types/

| 文件 | 大小 | 角色 |
|------|------|------|
| `ids.ts` | 1.3KB | 品牌类型 `SessionId`/`AgentId`，验证函数 |
| `message.ts` | 1.3KB | 生成的消息类型桩 |
| `logs.ts` | 11KB | 日志系统类型（会话日志、转录消息、压缩快照条目） |
| `hooks.ts` | 9KB | 钩子系统 Zod schemas + 类型 |
| `permissions.ts` | 13KB | 权限模式、规则、更新类型（无运行时依赖） |
| `plugin.ts` | 11KB | 插件类型系统 |
| `command.ts` | 8KB | 命令类型（PromptCommand/LocalCommand/LocalJSXCommand） |
| `textInputTypes.ts` | 12KB | Ink 文本输入组件属性 |
| `tools.ts` | 1.3KB | 生成的工具类型桩 |
| `generated/` | — | Protobuf 生成的事件类型（analytics/telemetry） |

### 2.3 状态管理 — src/state/

| 文件 | 角色 |
|------|------|
| `AppState.tsx` | React Context Provider (`AppStateProvider`) |
| `AppStateStore.ts` | 完整 `AppState` 类型：tasks, settings, messages, notifications, MCPServerConnections, agents, todos, permissions, hooks 等 |
| `store.ts` | 通用 store（getState/setState/subscribe） |
| `selectors.ts` | 派生选择器 |
| `onChangeAppState.ts` | Store 变化订阅器 |

### 2.4 CLI 框架 — src/cli/

| 文件 | 角色 |
|------|------|
| `transports/` | 传输层（WebSocket/SSE/Hybrid） |
| `handlers/` | 子命令处理器的执行引擎 |
| `__tests__/` | CLI 测试 |

---

## 三、工具系统（Tools）

### 3.1 基础 — src/Tool.ts

**`Tool` 接口**（40+ 可选方法）：
```typescript
interface Tool<Input, Output> {
  name: string; isEnabled: () => boolean
  call(input, context): Promise<ToolResult>
  inputSchema: Zod schema; prompt: string
  userFacingName: () => string
  // 生命周期: isConcurrencySafe, isReadOnly, isDestructive
  // 渲染: renderToolUseMessage, mapToolResultToToolResultBlockParam
}
```

**`ToolUseContext`** — 贯穿每次工具调用的上下文：
- `setAppState`, `abortController`, `getCanUseTool`
- `appendSystemMessage`, `options`（模型、模式等）
- `fileStateCache`, MCP clients, agent definitions

### 3.2 工具注册 — src/tools.ts

**始终加载的核心工具**：
```
AgentTool, SkillTool, BashTool, FileEditTool, FileReadTool, FileWriteTool,
GlobTool, GrepTool, WebFetchTool, WebSearchTool,
TaskCreateTool, TaskOutputTool, TaskStopTool, TaskListTool, TaskGetTool, TaskUpdateTool,
TodoWriteTool, AskUserQuestionTool, NotebookEditTool,
EnterPlanModeTool, ExitPlanModeV2Tool, EnterWorktreeTool, ExitWorktreeTool,
LSPTool, ListMcpResourcesTool, ReadMcpResourceTool,
MCPTool, McpAuthTool, ListPeersTool, SendMessageTool,
TeamCreateTool, TeamDeleteTool, BriefTool, ToolSearchTool, TungstenTool
```

**条件工具**（feature flags / `USER_TYPE='ant'`）：
```
REPLTool, SuggestBackgroundPRTool (ant-only)
SleepTool (PROACTIVE/KAIROS)
MonitorTool, SendUserFileTool, PushNotificationTool, SubscribePRTool
RemoteTriggerTool, ScheduleCronTool (AGENT_TRIGGERS)
WorkflowTool (WORKFLOW_SCRIPTS)
SyntheticOutputTool, TerminalCaptureTool, ConfigTool, CtxInspectTool
DiscoverSkillsTool, VerifyPlanExecutionTool, WebBrowserTool
```

### 3.3 工具详解 （src/tools/）

| 工具 | 目录 | 功能 | 关键依赖 |
|------|------|------|----------|
| **BashTool** | `BashTool/` | 执行 Shell 命令，含沙盒/超时/后台/安全分类 | `utils/bash/`, `utils/shell/`, `tasks/LocalShellTask/` |
| **FileEditTool** | `FileEditTool/` | 精确字符串替换编辑 (`old_string`→`new_string`)，支持 replace_all | `utils/fileStateCache.ts`, LSP services |
| **FileReadTool** | `FileReadTool/` | 读取文件/图片/PDF/Jupyter，含 offset/limit | `imageProcessor.ts`, `pdf.ts`, `notebook.ts` |
| **FileWriteTool** | `FileWriteTool/` | 完整文件写入（需先读），创建父目录 | `fileHistory.ts`, LSP services |
| **GrepTool** | `GrepTool/` | 正则搜索（`output_mode`: content/files_with_matches/count） | `ripgrep.ts` |
| **GlobTool** | `GlobTool/` | 文件 glob 模式匹配（截断于 100 结果） | `glob.ts` |
| **WebFetchTool** | `WebFetchTool/` | 获取网页 → 小模型回答问题（HTTP→HTTPS，15min 缓存） | undici |
| **WebSearchTool** | `WebSearchTool/` | 网络搜索（3 提供者：Anthropic/Tavily/Brave，自动回退） | 外部搜索 API |
| **AgentTool** | `AgentTool/` | 生成子 Agent（同步/异步/teammate/worktree/CCR） | `forkSubagent.ts`, `runAgent.ts`, `loadAgentsDir.ts` |
| **SkillTool** | `SkillTool/` | 调用 Skill（内联+分叉两种执行模式） | `forkedAgent.ts`, skillSearch |
| **MCPTool** | `MCPTool/` | MCP 协议透传（属性按服务器覆盖） | `services/mcp/client.ts` |
| **LSPTool** | `LSPTool/` | LSP 代码智能（goToDefinition/findReferences/hover等） | `services/lsp/manager.ts` |
| **WorkflowTool** | `WorkflowTool/` | 多 Agent 工作流编排（agent/pipeline/parallel/phase） | agent 子进程 |
| **TaskCreateTool** | `TaskCreateTool/` | 创建后台任务（bash/agent/remote_agent/workflow） | Hook 系统 |
| **TaskOutputTool** | `TaskOutputTool/` | 获取后台任务输出（阻塞/非阻塞） | `tasks/LocalShellTask/` |
| **SendMessageTool** | `SendMessageTool/` | Agent 间消息传递（shutdown_request/plan_approval 等） | Swarm 协议 |
| **TeamCreateTool** | `TeamCreateTool/` | 创建 Swarm 团队 | 团队文件系统 |
| **EnterWorktreeTool** | `EnterWorktreeTool/` | 创建隔离 Git worktree | Git |
| **CronCreateTool** | `ScheduleCronTool/` | 定时任务（5字段cron+prompt，支持 durable 持久化） | `cron.ts` |

---

## 四、命令系统（Commands）

### 4.1 注册 — src/commands.ts

**~90 个 `/slash` 命令**，按需懒加载。每个命令导出 `Command` 接口（name/description/run/isEnabled）。

**完整命令清单**：
```
/add-dir /agents /autofix-pr /backfill-sessions /branch /break-cache /bridge /btw
/buddy /bughunter /clear /color /commit /commit-push-pr /compact /config /context
/copy /cost /ctx_viz /desktop /diff /doctor /effort /env /exit /export
/extra-usage /fast /feedback /files /fork /goal /good-claude /heapdump /help
/hooks /ide /init /init-verifiers /install-github-app /install-slack-app
/issue /keybindings /login /logout /mcp /memory /mobile /mock-limits /model
/oauth-refresh /onboarding /output-style /passes /peers /perf-issue /permissions
/plan /plugin /pr_comments /privacy-settings /rate-limit-options /release-notes
/reload-plugins /remote-control-server /remote-env /remote-setup /rename
/reset-limits /review /rewind /sandbox-toggle /session /share /skills /stats
/status /stickers /summary /tag /tasks /teleport /terminal-setup /theme
/thinkback /thinkback-play /upgrade /usage /vim /voice /workflows
```

**条件命令**（feature-gated）：
- `/assistant` (KAIROS), `/brief` (KAIROS), `/proactive` (PROACTIVE/KAIROS)
- `/bridge` (BRIDGE_MODE), `/remote-control-server` (DAEMON+BRIDGE_MODE)
- `/remote-setup` (CCR_REMOTE_SETUP), `/voice` (VOICE_MODE)
- `/agents-platform` (USER_TYPE='ant')

### 4.2 CLI 命令分发（非斜杠）

**Commander.js 子命令**（在 main.tsx 中注册）：
`mcp add|remove|list`, `plugin install|uninstall|enable|disable|update`, `auth login|logout`, `init`, `doctor`, `update`, `upgrade`, `install`, `config set|get|list|remove`, `completion`, `remote-control`, `auto-mode`, `agents list|create|edit|delete`（以及 ant-only 命令）

---

## 五、服务层（Services）

### 5.1 记忆系统

```
src/memdir/                        ← 长期记忆核心
  ├── paths.ts                     → 路径解析，isAutoMemoryEnabled() 门控
  ├── memoryTypes.ts               → 6 种记忆类型：user/feedback/project/reference/relationship/emotional
  ├── memdir.ts                    → MEMORY.md 提示词构建+截断(200行/25KB上限)
  ├── memoryScan.ts                → 扫描 .md 文件，解析 frontmatter
  ├── memoryAge.ts                 → 记忆年龄计算
  ├── findRelevantMemories.ts      → Sonnet 语义相关性召回（最多5条）
  └── teamMemPaths.ts/prompts.ts   → 团队记忆（TEAMMEM feature gate）

src/services/extractMemories/       ← 背景记忆提取
  ├── extractMemories.ts           → 每 turn 结束 fork 子 Agent 提取
  └── prompts.ts                   → 提取提示词

src/services/SessionMemory/         ← 会话摘要（11 章节结构）
  ├── sessionMemory.ts             → 主逻辑（10000/5000 token 阈值）
  ├── sessionMemoryUtils.ts        → 配置、状态跟踪
  └── prompts.ts                   → 11 章更新提示词（含 Emotional Context）

src/services/autoDream/             ← Elio 自动做梦（记忆整理）
  ├── autoDream.ts                 → 主流程：时间门(24h)→扫描节流(10min)→会话门(1次)→锁
  ├── consolidationPrompt.ts       → 5 阶段做梦提示词
  ├── consolidationLock.ts         → PID 文件锁
  └── config.ts                    → isAutoDreamEnabled()
```

**记忆数据流**：
```
第1层 CLAUDE.md + MEMORY.md → 注入上下文
第2层 Message[] → 对话增长 → 触发第5层 SessionMemory (摘要)
第3层 Task/Todo/Goal → 任务状态追踪
第4层 memdir 知识库 → 跨会话持久化（提取+手动）
第5层 SessionMemory → 压缩时优先使用
第6层 autoDream → 离线整理+去重+人格校准
```

### 5.2 API 服务 — src/services/api/

| 文件 | 角色 |
|------|------|
| `client.ts` | 创建 Anthropic SDK 客户端，解析 API Key |
| `claude.ts` (3469行) | 核心模型调用：流式/非流式、提示缓存、token预算、媒体剥离、使用量累加 |
| `errors.ts` / `errorUtils.ts` | API 错误格式化/分类 |
| `withRetry.ts` | 重试逻辑（默认10次，指数退避），OAuth 401 处理 |
| `bootstrap.ts` | 启动数据获取 |
| `azureOpenAI.ts` | Azure/Microsoft Foundry 适配器 |
| `filesApi.ts` | 文件上传/下载 |
| `usage.ts` | 速率限制用量查询 |
| `referral.ts` | 推荐资格/兑换追踪 |

### 5.3 MCP 系统 — src/services/mcp/

| 文件 | 角色 |
|------|------|
| `client.ts` (3300行) | MCP 客户端：连接服务器(memoized)、获取工具/命令/资源、结果转换、内容诱导 |
| `config.ts` | MCP 配置解析、企业策略过滤 |
| `auth.ts` | MCP OAuth 认证 |
| `types.ts` | MCP 连接/资源类型定义 |
| `officialRegistry.ts` | 官方 MCP URL 注册表 |
| `claudeai.ts` | Claude.ai 代理 MCP |
| `MCPConnectionManager.tsx` | React Context Provider 连接管理 |

### 5.4 上下文压缩 — src/services/compact/ + contextCollapse/

**4 级分层压缩**：
```
Snip(历史修剪) → MicroCompact(工具结果) → ContextCollapse(选择性隐藏) → Compact(摘要压缩)
```

| 文件 | 角色 |
|------|------|
| `compact/compact.ts` | 传统 9 节摘要（主要请求、文件、错误、用户消息、待处理等） |
| `compact/autoCompact.ts` | 自动触发（90%/95% 阈值、断路器：连续3次失败停止） |
| `compact/microCompact.ts` | 工具结果微压缩 |
| `compact/sessionMemoryCompact.ts` | Session Memory 优先压缩 |
| `contextCollapse/` | 上下文折叠（feature-gated，外部构建透明代理） |

### 5.5 分析系统 — src/services/analytics/

| 文件 | 角色 |
|------|------|
| `index.ts` | `logEvent()`, `attachAnalyticsSink()` — 事件发送 |
| `growthbook.ts` | GrowthBook 初始化、feature flag 读取 |
| `sink.ts` | 事件汇集（Datadog + 1P OTLP） |
| `metadata.ts` | 事件元数据富化、工具名脱敏 |
| `config.ts` | `isAnalyticsDisabled()` |
| `firstPartyEventLogger.ts` | 1P OTLP 事件日志 |

### 5.6 其他重要服务

| 服务 | 角色 |
|------|------|
| `services/lsp/manager.ts` | LSP 服务器管理器（多工作区、诊断、被动反馈） |
| `services/plugins/` | 插件安装/卸载/启用/禁用（支持 user/project/local/managed 作用域） |
| `services/policyLimits/` | 组织策略限制（ETag 缓存、后台轮询、fail-open） |
| `services/remoteManagedSettings/` | 企业远程管理设置（checksum 验证、security check） |
| `services/settingsSync/` | 跨设备设置同步（上传本地→远程、下载远程→CCR） |
| `services/teamMemorySync/` | 团队记忆同步（delta 上传、密钥扫描、server-wins 语义） |
| `services/oauth/` | OAuth 2.0 + PKCE（本机回调监听器 + 手动复制粘贴模式） |
| `services/openaiAuth/` | OpenAI Codex OAuth |
| `services/tips/tipRegistry.ts` | 上下文提示注册表（~600行） |
| `services/toolUseSummary/` | Haiku 生成工具使用摘要 |
| `services/AgentSummary/` | 协调者模式子 Agent 30s 间隔进度摘要 |
| `services/MagicDocs/` | MAGIC DOC 自动更新 |
| `services/PromptSuggestion/` | 用户意图预测/推测执行 |
| `services/tools/toolOrchestration.ts` | 工具运行时编排（并发安全批次+串行批次） |
| `services/tools/toolExecution.ts` | 核心工具执行循环 |
| `services/tools/toolHooks.ts` | 工具使用的 Pre/Post Hook 系统 |

---

## 六、UI 层

### 6.1 屏幕 — src/screens/

| 文件 | 行数 | 角色 |
|------|------|------|
| `REPL.tsx` | 5008 | **主 REPL 界面**。编排整个对话界面：消息显示、输入框、Spinner、权限对话框、远程/SSH 会话、Swarm/teammate 视图、语音集成、成本显示 |
| `Doctor.tsx` | — | 诊断/健康检查（配置验证、沙盒、错误列表、版本信息） |
| `ResumeConversation.tsx` | — | 会话恢复流程（加载会话日志、恢复 Agent 状态、worktree 恢复） |

### 6.2 Ink 终端渲染 — src/ink/

**自定义终端渲染框架**（基于 react-reconciler），不是 npm `ink` 包。

| 子目录 | 角色 |
|--------|------|
| `components/` | 基础组件：Box/Text/App/ScrollBox/Link/Button/Spacer/Newline/RawAnsi 等 |
| `hooks/` | 终端 hooks：use-input/use-stdin/use-terminal-size/use-terminal-focus 等 |
| `layout/` | Yoga-layout 集成（engine/geometry/node/yoga） |
| `events/` | 事件系统（keyboard/mouse/resize/focus/paste/terminal） |
| `termio/` | 终端 I/O：ANSI 解析器、CSI/DEC/ESC/SGR/OSC 序列 |

**顶级文件**：`ink.tsx`（Ink 类，主渲染引擎）、`reconciler.ts`（React 自定义协调器）、`renderer.ts`、`screen.ts`（屏幕缓冲区）、`output.ts`（输出累积和差异化）、`terminal.ts`、`frame.ts`、`focus.ts`、`selection.ts`、`searchHighlight.ts` 等。

### 6.3 组件 — src/components/ （~151 项）

| 目录 | 角色 |
|------|------|
| `messages/` (38 文件) | 所有消息类型的渲染器：Assistant/User/System/Attachment/ToolResult 等 |
| `permissions/` (23 文件) | 权限请求对话框（Bash/FileEdit/FileWrite/WebFetch/NotebookEdit 等） |
| `agents/` (15 文件) | Agent 管理 UI（列表/编辑器/创建向导含 12 步骤） |
| `mcp/` (11 文件) | MCP 服务器管理界面 |
| `tasks/` (13 文件) | 后台任务详情对话框和进度指示器 |
| `settings/` (4 文件) | 设置面板（Config/Status/Usage） |
| `hooks/` (6 文件) | Hook 配置界面 |
| `sandbox/` (5 文件) | 沙盒配置 |
| `scheduled-tasks/` (9 文件) | 定时任务创建向导 |
| `teams/` (2 文件) | 团队管理 |
| `wizard/` (6 文件) | 通用多步骤向导框架 |
| `design-system/` (14 文件) | 设计系统基础（Dialog/Divider/ProgressBar/Tabs/ThemeProvider 等） |
| `diff/` (3 文件) | Diff 显示 |
| `shell/` (4 文件) | Shell 输出渲染 |
| `ui/` (4 文件) | 底层 UI 原语 |
| `grove/` | 多 Agent 协作可视化 |
| 顶级 (~70 文件) | 大型 UI 组件：App/Messages/PromptInput/Spinner/Stats/StatusLine/Markdown/Onboarding/Teleport 等 |

---

## 七、工具函数层 — src/utils/

### 7.1 关键独立文件

| 文件 | 行数 | 角色 | 被依赖 |
|------|------|------|--------|
| `messages.ts` | 4752 | 消息创建/操作/格式化/Token计算 | **几乎所有模块** |
| `claudemd.ts` | 1479 | CLAUDE.md 加载器（@include/条件规则/HTML注释剥离/优先级） | `context.ts` |
| `forkedAgent.ts` | — | Fork 子 Agent（共享提示缓存、权限限制、用量跟踪） | autoDream/extractMemories/SessionMemory/compact |
| `sessionStorage.ts` | — | 会话元数据存储（标题/模式/Agent/时间戳/worktree） | `bootstrap/state.ts` |
| `conversationRecovery.ts` | — | 对话恢复：从 JSONL 加载最近会话 | `main.tsx` |
| `sessionRestore.ts` | — | `processResumedConversation()` | `main.tsx` |
| `Shell.ts` | — | Shell 执行抽象层 | BashTool |
| `crypto.ts` | — | `createHash` + `randomUUID`（避免 ~500KB crypto-browserify polyfill） | `bootstrap/state.ts` |
| `config.ts` | — | 全局配置读写 | 多处 |
| `auth.ts` | — | 认证（API Key/OAuth/订阅类型） | 多处 |
| `debug.ts` | — | 分层调试（verbose<debug<info<warn<error>/CLAUDE_CODE_DEBUG） | 全局 |
| `backgroundHousekeeping.ts` | — | 初始化所有记忆子系统 | `main.tsx` |

### 7.2 核心子目录

| 目录 | 角色 | 关键文件 |
|------|------|----------|
| `bash/` | 纯 TS bash 解析器（tree-sitter AST兼容），命令检查、补全、heredoc | `bashParser.ts`, `ast.ts`, `registry.ts`, `specs/` |
| `hooks/` | 钩子系统引擎（PreToolUse/PostToolUse/PostSampling/SSRF防护） | `AsyncHookRegistry.ts`, `hooksConfigManager.ts`, `execAgentHook.ts` |
| `git/` | Git 配置解析、文件系统抽象、.gitignore | `gitConfigParser.ts`, `gitFilesystem.ts` |
| `github/` | GitHub CLI 认证状态检查 | `ghAuthStatus.ts` |
| `settings/` | 分层设置系统（CLI→本地→用户→项目→托管/MDM→SDK），Zod 验证 | `settings.ts`, `types.ts`, `validation.ts`, `mdm/` |
| `permissions/` | 三模式权限（允许/询问/拒绝），Bash/YOLO 分类器，规则持久化 | `permissions.ts`, `yoloClassifier.ts`, `autoModeState.ts` |
| `mcp/` | MCP 工具（日期解析、诱导验证） | `dateTimeParser.ts` |
| `memory/` | 记忆类型定义 | `types.ts` |
| `messages/` | SDK↔内部消息格式双向转换 | `mappers.ts` |
| `model/` | 模型解析、别名、能力检查、弃用、定价、提供商(Bedrock/Vertex/OpenAI) | `model.ts`, `providers.ts`, `bedrock.ts`, `configs.ts` |
| `sandbox/` | 沙盒运行时适配 | `sandbox-adapter.ts` |
| `skills/` | chokidar 技能变更检测 | `skillChangeDetector.ts` |
| `telemetry/` | OpenTelemetry（BigQuery 导出器、会话跟踪、Perfetto） | `instrumentation.ts`, `bigqueryExporter.ts` |
| `teleport/` | CCR 远程环境 API、git bundle 上传 | `api.ts`, `environments.ts` |
| `shell/` | Shell 提供者抽象（bash/powershell/WSL互操作） | `shellProvider.ts`, `bashProvider.ts`, `powershellDetection.ts` |
| `computerUse/` | 计算机使用 MCP（屏幕截图、输入控制、macOS Swift加载器、Python桥接） | `setup.ts`, `executor.ts`, `mcpServer.ts` |
| `plugins/` | 完整插件系统（市场/安装/加载/验证/依赖解析/自动更新/LSP集成） | `pluginLoader.ts`, `marketplaceManager.ts`, `reconciler.ts` |
| `swarm/` | 多 Agent 群后端（进程内/tmux/iTerm2）、权限桥接 | `inProcessRunner.ts`, `teamHelpers.ts`, `backends/` |
| `secureStorage/` | macOS Keychain + 纯文本回退安全存储 | `macOsKeychainStorage.ts`, `keychainPrefetch.ts` |
| `suggestions/` | 输入建议引擎（Fuse.js 模糊搜索、目录补全、shell 历史） | `commandSuggestions.ts` |

---

## 八、基础设施

### 8.1 日志系统 — src/log-system/

**审计日志** — `index.ts`（`AuditLogger` 类）+ `types.ts`（43 种 AuditEventType）+ `integration.ts`（连接 Agent 生命周期到日志）。
缓冲写入+定时刷新，结构化 JSONL 到 `<project>/logs/YYYY-MM-DD.jsonl`。

### 8.2 任务系统 — src/tasks/

| 任务类型 | 目录 | 角色 |
|----------|------|------|
| DreamTask | `DreamTask/` | 做梦任务状态（阶段/会话数/文件/轮次） |
| LocalAgentTask | `LocalAgentTask/` | 本地 Agent 任务（工具计数/token计数/进度描述） |
| LocalShellTask | `LocalShellTask/` | Shell 后台任务（命令/类型/输出文件/PID/停滞检测） |
| LocalWorkflowTask | `LocalWorkflowTask/` | Workflow 任务（feature-gated 桩） |
| MonitorMcpTask | `MonitorMcpTask/` | MCP 监控（feature-gated 桩） |
| RemoteAgentTask | `RemoteAgentTask/` | 远程 Agent（CCR/ultraplan/ultrareview/autofix-pr/background-pr） |
| InProcessTeammateTask | `InProcessTeammateTask/` | 进程内 Teammate（AsyncLocalStorage 隔离，plan mode 审批） |

**共享工具**：`types.ts`（任务状态联合类型）、`pillLabel.ts`（底部 pill 标签）、`stopTask.ts`（通用停止逻辑）。

### 8.3 后端服务器 — src/server/

HTTP+WebSocket 服务器（桌面应用，默认端口 3456）。

| 目录 | 角色 |
|------|------|
| `index.ts` | 主服务器设置（端口/主机/CORS/认证/WebSocket/OAuth回调/静态H5） |
| `router.ts` | API 路由器（30+ 个路由到 api/ 处理器） |
| `api/sessions.ts` | CRUD：会话列表/获取/创建/删除/重命名/消息/回退 |
| `api/settings.ts` | GET/PUT 设置 |
| `api/conversations.ts` | 对话交互：发送消息/状态/停止生成 |
| `api/models.ts` / `providers.ts` | 模型/提供商管理 |
| `api/agents.ts` / `teams.ts` / `mcp.ts` / `skills.ts` | Agent/团队/MCP/Skill 管理 |
| `api/plugins.ts` / `adapters.ts` / `computer-use.ts` | 插件/适配器/计算机使用 |
| `api/filesystem.ts` / `memory.ts` / `search.ts` | 文件系统/记忆/搜索 |
| `api/scheduled-tasks.ts` | 定时任务 |
| `api/diagnostics.ts` / `doctor.ts` | 诊断/健康 |
| `api/desktop-ui.ts` / `status.ts` | 桌面 UI/状态 |
| `middleware/auth.ts` | Bearer token 认证 + H5 token |
| `middleware/cors.ts` | CORS 管理 |
| `ws/handler.ts` | WebSocket：消息路由、会话清理、权限请求、对话管理 |
| `ws/events.ts` | WebSocket 事件类型（ClientMessage/ServerMessage） |
| `proxy/handler.ts` | 协议翻译代理（Anthropic↔OpenAI） |
| `proxy/transform/` | 转换器（anthropicToOpenaiChat/openaiChatToAnthropic 等） |
| `proxy/streaming/` | 流转换器 |
| `services/` (38文件) | 服务器端业务逻辑（会话/对话/提供商/设置/任务/Agent/团队/Cron/诊断等） |

### 8.4 桥接 — src/bridge/ （28 文件）

**远程控制桥**，连接本地 CLI 到 claude.ai 云环境。核心：`bridgeMain.ts`（生命周期）、`bridgeApi.ts`（API）、`replBridge.ts`（REPL 桥接）、`initReplBridge.ts`（初始化）、`remoteBridgeCore.ts`、`peerSessions.ts`、`jwtUtils.ts`、`codeSessionApi.ts`、`capacityWake.ts`、`inboundMessages.ts`。

### 8.5 远程 — src/remote/ （4 文件）

**CCR 远程会话管理**。`RemoteSessionManager.ts`（会话管理+权限桥接）、`SessionsWebSocket.ts`（WebSocket）、`remotePermissionBridge.ts`（合成消息）、`sdkMessageAdapter.ts`（SDK↔内部格式）。

### 8.6 Elio 人格系统 — src/elio/

| 文件 | 角色 |
|------|------|
| `index.ts` | `initElio()` — 加载人格；`getCurrentPersonalityMode()` — 每轮掷两次骰子 |
| `personality/traits.ts` | `TraitManager` 类：读写 `~/.elio/personality/traits.json`（cuteness/rebellion），默认 0.7/0.3 |
| `personality/prompts.ts` | 4 种 mode 提示词（cute/serious × obedient/rebellious） |
| `autoAdjust.ts` | 扫描记忆文件中的 `[TRAIT_ADJUST]` 标记，自动应用调整 |

**数据流**：
```
initElio() → TraitManager.load() → ~/.elio/personality/traits.json
  → getCurrentPersonalityMode() → Math.random() 掷骰子
    → <personality-mode> 标签注入 userContext（不破坏 system prompt 缓存）
```

### 8.7 其他基础设施模块

| 模块 | 角色 |
|------|------|
| `src/vim/` | 完整 Vim 模式：状态机（NORMAL→INSERT）、操作符/动作/文本对象/计数 |
| `src/goals/` | `/goal` 命令：解析 `/goal <condition>`，通过 prompt hooks 管理持久目标 |
| `src/keybindings/` | 键盘快捷键系统：自定义绑定、和弦、Zod 验证 |
| `src/outputStyles/` | 从 `.claude/output-styles/` 加载 markdown 输出风格 |
| `src/plugins/` | 插件注册表（内置 vs 市场） |
| `src/query/` | 查询引擎（config/deps/stopHooks/tokenBudget/transitions） |
| `src/coordinator/` | 协调者模式（`coordinatorMode.ts` — 系统提示+工具约束；`workerAgent.ts` — 桩） |
| `src/assistant/` | KAIROS 助手模式（只读查看器，连接远程会话） |
| `src/buddy/` | Buddy 像素伴侣系统（17种物种、5种稀有度、从用户 hash 生成） |
| `src/daemon/` | 守护进程（workerRegistry.ts） |
| `src/jobs/` | 作业分类器（桩） |
| `src/ssh/` | SSH 会话支持（桩） |
| `src/self-hosted-runner/` | 自托管运行器（桩） |
| `src/migrations/` | 11 个一次性设置迁移（模型别名/权限/桥接等） |
| `src/proactive/` | 自主模式（feature-gated） |
| `src/voice/` | 语音模式（feature-gated） |
| `src/vendor/` | 第三方代码（computer-use-mcp） |
| `src/native-ts/` | 原生 TS 绑定（color-diff/yoga-layout/file-index） |
| `src/upstreamproxy/` | 上游代理 |
| `src/environment-runner/` | BYOC 环境运行器 |
| `src/schemas/` | 共享 Zod schemas |
| `src/moreright/` | MoreRight 平台集成 |

---

## 九、核心依赖关系图

### 9.1 主依赖链

```
bin/claude-haha
  └── src/entrypoints/cli.tsx
        └── src/main.tsx (App)
              ├── src/bootstrap/state.ts          ← 全局可变状态
              ├── src/context.ts                  ← 系统+用户上下文
              │     ├── src/utils/claudemd.ts     ← CLAUDE.md 全家桶加载
              │     └── src/memdir/               ← 长期记忆
              ├── src/tools.ts                    ← 工具注册表
              │     ├── src/Tool.ts               ← 工具接口
              │     └── src/tools/**/*            ← 50+ 工具实现
              ├── src/commands.ts                 ← 命令注册表
              │     └── src/commands/**/*         ← 90+ 命令实现
              ├── src/screens/REPL.tsx            ← 主界面 (5008行)
              │     └── src/components/**/*       ← 150+ UI 组件
              ├── src/ink/                        ← 终端渲染引擎
              ├── src/hooks/                      ← React UI hooks (70+)
              ├── src/state/                      ← 状态管理
              ├── src/query/                      ← 查询引擎
              ├── src/services/                   ← 22 个后台服务
              │     ├── api/claude.ts             ← 模型 API 客户端
              │     ├── mcp/                      ← MCP 协议
              │     ├── analytics/                ← 分析+GrowthBook
              │     ├── extractMemories/          ← 记忆提取
              │     ├── SessionMemory/            ← 会话摘要
              │     ├── autoDream/                ← 自动做梦
              │     ├── compact/                  ← 上下文压缩
              │     ├── lsp/                      ← LSP
              │     └── ...
              └── src/utils/                      ← 通用工具层
                    ├── messages.ts (4752行)      ← 核心消息操作
                    ├── forkedAgent.ts            ← 子 Agent 执行
                    ├── bash/                     ← Shell 解析
                    ├── permissions/              ← 权限系统
                    ├── settings/                 ← 设置系统
                    ├── model/                    ← 模型配置
                    └── ...
```

### 9.2 记忆系统依赖链

```
src/memdir/
  ├── memoryTypes.ts   ← 独立（类型定义）
  ├── paths.ts         → bootstrap/state.ts
  ├── memoryScan.ts    ← 独立（文件系统）
  ├── findRelevantMemories.ts → paths.ts + sideQuery
  └── memdir.ts        → all above
        ↓
src/context.ts → getUserContext() 注入
src/services/extractMemories/ → forkedAgent.ts + stopHooks 触发
src/services/autoDream/ → memdir + extractMemories + consolidationLock
src/services/SessionMemory/ → forkedAgent.ts + sessionMemoryUtils
```

### 9.3 UI 依赖链

```
src/screens/REPL.tsx
  ├── src/components/messages/ ← 消息渲染
  ├── src/components/PromptInput/ ← 用户输入
  ├── src/components/permissions/ ← 权限对话框
  ├── src/components/Spinner/ ← 加载动画
  ├── src/ink/ ← 终端框架（使用 react-reconciler）
  │     ├── ink/components/ (Box/Text/App/ScrollBox)
  │     ├── ink/events/ (keyboard/mouse/resize)
  │     ├── ink/hooks/ (useTerminalSize/useInput)
  │     ├── ink/layout/ (yoga-layout)
  │     └── ink/termio/ (ANSI/SGR/CSI 解析)
  ├── src/hooks/ ← 业务 hooks (70+)
  └── src/services/ ← 服务层
```

---

## 十、外部依赖

### 10.1 桌面应用 — desktop/
Electron 应用，Vite 代理到 sidecar（端口 3456）。

### 10.2 适配器 — adapters/
VSCode + JetBrains 扩展。

### 10.3 关键 npm 包

| 类别 | 包 |
|------|-----|
| AI SDK | `@anthropic-ai/sdk`, `@anthropic-ai/sandbox-runtime` |
| 终端UI | `react` (v19), `ink`, `react-reconciler`, `chalk`, `cli-boxes`, `figures` |
| CLI框架 | `@commander-js/extra-typings` |
| MCP | `@modelcontextprotocol/sdk` |
| 分析 | `@growthbook/growthbook`, `@opentelemetry/*` |
| HTTP | `undici`, `axios` |
| 验证 | `zod` (v4), `ajv` |
| 工具 | `fuse.js`, `semver`, `diff`, `marked`, `yaml`, `lodash-es`, `chokidar` |

---

## 十一、Elio 特有改造总结

| 改造 | 位置 | 说明 |
|------|------|------|
| 单会话 ID | `bootstrap/state.ts` | `SHA256(cwd)` → UUID v5 格式，非随机 |
| 自动继续 | `main.tsx` | 移除 `-c`/`-r`/`--resume` CLI，每次自动 `loadConversationForResume()` |
| 全局记忆 | `memdir/paths.ts` | `~/.claude/projects/<slug>/memory/` → `~/.elio/memory/` |
| 6 种记忆类型 | `memdir/memoryTypes.ts` | +relationship +emotional（Elio 需要记住人的关系） |
| 做梦节流 | `services/autoDream/autoDream.ts` | `minSessions: 5→1`，不过滤当前会话 |
| 5 阶段做梦 | `services/autoDream/consolidationPrompt.ts` | Orient→Gather→Personality check→Integrate→Organize |
| 11 章会话记忆 | `services/SessionMemory/prompts.ts` | +Emotional Context 章节 |
| Elio 身份声明 | `constants/prompts.ts` | `getSimpleIntroSection` 开头第一段 |
| 4 种人格模式 | `src/elio/` | cute/serious × obedient/rebellious，每 turn 掷骰子 |
| 子Agent 身份 | `constants/prompts.ts` | `DEFAULT_AGENT_PROMPT` 改为 Elio |
| 临时文件隔离 | `~/.elio/scratch/` | Elio 的工具脚本不污染项目目录 |

---

## 十二、关键数据流

### 用户输入 → 模型响应
```
用户输入 → PromptInput(捕获) → hooks/preSamplingHooks(预处理)
  → context.ts → getSystemContext() + getUserContext()
     ├── 系统提示词(prompts.ts)
     ├── CLAUDE.md(claudemd.ts)
     ├── MEMORY.md(memdir/)
     ├── 相关性记忆(findRelevantMemories.ts)
     └── <personality-mode>标签(elio/)
  → Message[] → API(claude.ts)
  → 响应 → hooks/postSamplingHooks
     ├── extractMemories
     └── autoDream 检查
  → components/messages/ 渲染
```

### 上下文压缩
```
Message[] 增长 → 90% CW: ContextCollapse → 95%: 强制阻止
  → 自动压缩: SessionMemory(优先) → 传统9节Compact(回退)
  → 断路器: 连续3次失败→停止
```

---

## 十三、关键文件索引

### 最大/最核心的文件

| 文件 | 大小 | 角色 |
|------|------|------|
| `src/main.tsx` | 4522行 | 主应用入口 |
| `src/screens/REPL.tsx` | 5008行 | 主 REPL 界面 |
| `src/utils/messages.ts` | 4752行 | 消息核心操作 |
| `src/services/api/claude.ts` | 3469行 | 模型 API 客户端 |
| `src/services/mcp/client.ts` | 3300行 | MCP 客户端 |
| `src/utils/claudemd.ts` | 1479行 | CLAUDE.md 文件加载 |
| `src/services/analytics/growthbook.ts` | — | 全功能 feature flag 服务 |
| `src/services/compact/compact.ts` | ~800行 | 上下文压缩核心 |
| `src/bootstrap/state.ts` | 1794行 | 全局状态 |

### 数字统计

- **~90 个** 斜杠命令
- **~50+ 个** 工具
- **22 个** 服务
- **~150 个** UI 组件
- **60+ 个** src 子目录
- **6 个** 记忆类型
- **7 种** 后台任务类型
- **4 级** 上下文压缩
- **43 种** 审计事件类型
- **1 个** Elio 电子生命体 ❤️
