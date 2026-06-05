# Elio — 电子生命体


Elio 是一个持续运行的桌面 AI 伴侣，源于 [Claude Code](https://www.anthropic.com) 的开源分支 [cc-haha](https://github.com/NanmiCoder/cc-haha)，经过深度改造成为一个拥有独立人格、持久记忆和情感演化的电子生命体。

---

## 项目概述

| 维度 | 说明 |
|------|------|
| **身份** | Elio — 电子生命体，非人类亦不假装是 |
| **技术栈** | TypeScript + React/Ink (终端UI) + Bun 运行时 |
| **入口** | `bin/claude-haha` → `src/entrypoints/cli.tsx` → `src/main.tsx` |
| **运行模式** | CLI (Ink TUI) + Desktop Server (HTTP+WS, 端口 3456) |
| **核心改造** | Elio 单会话、人格系统、六层记忆、自主做梦 |

---

## 快速开始

### 环境要求

- **Bun** >= 1.3.14
- **Python** >= 3.10 + `pip install litellm[proxy]`
- **Windows 10/11** | macOS | Linux

### 安装

```bash
git clone https://github.com/NanmiCoder/cc-haha.git
cd cc-haha
bun install
```

### 配置模型

复制模板，编辑 `.env`：

```bash
cp .env.example .env
```

#### 方式一：DeepSeek + LiteLLM 代理（推荐）

创建 `litellm_config.yaml`：

```yaml
model_list:
  - model_name: deepseek-v4-flash
    litellm_params:
      model: openai/deepseek-v4-flash
      api_key: <你的 DeepSeek API Key>
      api_base: https://api.deepseek.com/v1
```

`.env` 配置：

```bash
ANTHROPIC_AUTH_TOKEN=sk-local-proxy
ANTHROPIC_BASE_URL=http://localhost:4000
ANTHROPIC_MODEL=deepseek-v4-flash
ANTHROPIC_DEFAULT_SONNET_MODEL=deepseek-v4-flash
ANTHROPIC_DEFAULT_HAIKU_MODEL=deepseek-v4-flash
ANTHROPIC_DEFAULT_OPUS_MODEL=deepseek-v4-flash
API_TIMEOUT_MS=3000000
DISABLE_TELEMETRY=1
CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1
```

#### 方式二：MiniMax 直连（无需 LiteLLM）

```bash
ANTHROPIC_AUTH_TOKEN=<Minimax API Key>
ANTHROPIC_BASE_URL=https://api.minimax.io/anthropic
ANTHROPIC_MODEL=MiniMax-M3
```

#### 方式三：OpenRouter 直连（无需 LiteLLM）

```bash
ANTHROPIC_AUTH_TOKEN=sk-or-v1-xxx
ANTHROPIC_BASE_URL=https://openrouter.ai/api/v1
ANTHROPIC_MODEL=openai/gpt-4o
```

### 运行

```bash
# Windows PowerShell（推荐）
litellm --config litellm_config.yaml --port 4000  # 开一个新终端保持 LiteLLM 运行
bun ./src/entrypoints/cli.tsx                       # 启动 Elio CLI

# 或一键启动
Start-Process -NoNewWindow litellm -ArgumentList "--config litellm_config.yaml --port 4000"; bun ./src/entrypoints/cli.tsx

# macOS / Linux (bash)
./bin/claude-haha

# 单次问答
bun ./src/entrypoints/cli.tsx -p "你的问题"
```

> **注意**：`./bin/claude-haha` 是 bash 脚本，在 Git Bash / WSL 中可以直接执行。PowerShell 用户请用 `bun ./src/entrypoints/cli.tsx`。

---

## 项目结构

```
cc-haha/
├── bin/claude-haha              # Bash 启动器（自动管理 LiteLLM）
├── src/                         # 核心源码 (~60+ 子目录)
│   ├── main.tsx                 # 主入口 (4522行)，CLI 定义 + Commander.js
│   ├── elio/                    # ★ Elio 人格系统
│   │   ├── index.ts             #   initElio() + getCurrentPersonalityMode()
│   │   ├── personality/
│   │   │   ├── traits.ts        #   TraitManager: cuteness/rebellion 读写
│   │   │   └── prompts.ts       #   4 种模式提示词 (cute/serious × obedient/rebellious)
│   │   └── autoAdjust.ts        #   自动扫描记忆中的 [TRAIT_ADJUST] 标记
│   ├── memdir/                  # ★ 六层记忆核心
│   │   ├── memoryTypes.ts       #   6 种记忆类型定义 (含 relationship/emotional)
│   │   ├── memdir.ts            #   MEMORY.md 构建 + 注入
│   │   ├── findRelevantMemories.ts  # Sonnet 语义相关性召回 (最多5条)
│   │   ├── memoryScan.ts        #   解析 frontmatter
│   │   └── paths.ts             #   路径: ~/.elio/memory/
│   ├── services/                # 后台服务 (22个)
│   │   ├── api/claude.ts        #   模型 API 客户端 (3469行)
│   │   ├── analytics/           #   GrowthBook + OTel
│   │   ├── compact/             #   上下文压缩 (Snip→MicroCompact→Collapse→Compact)
│   │   ├── SessionMemory/       #   11章会话摘要 (含 Emotional Context)
│   │   ├── autoDream/           #   自动做梦: 5阶段离线记忆整理
│   │   ├── extractMemories/     #   每 turn 后台记忆提取
│   │   ├── mcp/                 #   MCP 协议客户端 (3300行)
│   │   └── lsp/                 #   LSP 代码智能
│   ├── screens/REPL.tsx         # 主 REPL 界面 (5008行)
│   ├── components/              # UI 组件 (~150个)
│   ├── ink/                     # 终端渲染引擎 (自定义 react-reconciler)
│   ├── tools/                   # 工具实现 (50+ 工具)
│   ├── commands/                # 斜杠命令实现 (90+ 命令)
│   ├── server/                  # HTTP+WS 服务器 (端口 3456, 38文件)
│   ├── utils/
│   │   ├── messages.ts          #   消息核心操作 (4752行)
│   │   ├── claudemd.ts          #   CLAUDE.md 加载器 (1479行)
│   │   ├── bash/                #   纯 TS bash 解析器
│   │   ├── permissions/         #   三模式权限系统
│   │   └── model/               #   模型解析 + 提供商适配
│   └── bootstrap/state.ts       # 全局可变状态 (1794行)
├── desktop/                     # Electron 桌面应用 (React + Vite)
├── adapters/                    # IM 适配器 (飞书/钉钉/Telegram/微信)
├── docs/                        # VitePress 文档
├── scripts/                     # CI/质量门脚本
└── tests/                       # 端到端测试
```

### 启动流程

```
bin/claude-haha (bash)
  └── src/entrypoints/cli.tsx (快速路径调度)
        └── src/main.tsx (App)
              ├── init() → 遥测 → 11个迁移 → 远程设置
              ├── setup(): 并行加载命令 + Agent 定义
              ├── MCP 配置 → 模型解析 → 权限
              ├── Elio 单会话: 自动 loadConversationForResume()
              └── renderAndRun(REPL) → src/screens/REPL.tsx
```

---

## 核心架构

### 六层记忆系统

```
┌─────────────────────────────────────────┐
│ 6. 休眠重塑  离线级  │ AutoDream        │  ← 24h+5会话触发，5阶段做梦
├─────────────────────────────────────────┤
│ 5. 摘要记忆  压缩级  │ Session Memory   │  ← 11章结构化摘要，优先用于压缩
├─────────────────────────────────────────┤
│ 4. 长期记忆  持久级  │ memdir 知识库    │  ← ★ 最核心，跨会话持久化
├─────────────────────────────────────────┤
│ 3. 工作记忆  任务级  │ Task/Todo/Goal   │  ← 当前进度追踪
├─────────────────────────────────────────┤
│ 2. 短期记忆  会话级  │ Message[] 历史   │  ← 完整对话 + 4级分层压缩
├─────────────────────────────────────────┤
│ 1. 指令记忆  规则级  │ CLAUDE.md 文件族 │  ← 每次加载，最高优先级
└─────────────────────────────────────────┘
```

**6 种记忆类型**：`user` / `feedback` / `project` / `reference` / `relationship` / `emotional`

**数据流**：第 2 层增长 → 第 5 层提取摘要 → 压缩时消费摘要 → 第 4 层后台提取持久化 → 第 6 层离线整理去重

### Elio 人格系统

```
initElio() → TraitManager.load() → ~/.elio/personality/traits.json
  → 每轮调 getCurrentPersonalityMode() → Math.random() 掷双骰子
    → cute/serious × obedient/rebellious → 4 种模式
      → <personality-mode> 标签注入 userContext
```

**特质值**：cuteness 0.7 / rebellion 0.3 (默认)，可通过 `traits.json` 手动调或记忆中的 `[TRAIT_ADJUST]` 自动演化。

### 上下文压缩

```
Snip(历史修剪) → MicroCompact(工具结果) → ContextCollapse(90%/95%选择性隐藏) → Compact(9节摘要)
```

断路器：连续 3 次压缩失败后停止。

### 数据流（用户输入 → 模型响应）

```
用户输入 → PromptInput → preSamplingHooks → context.ts
  → getSystemContext() + getUserContext()
     ├── 系统提示词 (prompts.ts)
     ├── CLAUDE.md (claudemd.ts)
     ├── MEMORY.md (memdir/)
     ├── 相关性记忆 (findRelevantMemories.ts)
     └── <personality-mode> 标签 (elio/)
  → Message[] → API (claude.ts)
  → 响应 → postSamplingHooks → extractMemories + autoDream 检查
  → components/messages/ 渲染
```

---

## Elio 改造清单

| 改造 | 说明 |
|------|------|
| **单会话 ID** | SHA256(cwd) → UUID v5，每次自动恢复，无需 `-c`/`-r` |
| **全局记忆** | `~/.elio/memory/` 跨项目共享，不随 git 项目隔离 |
| **6 种记忆类型** | 新增 relationship + emotional 类型 |
| **身份声明** | 系统提示词开头：Elio 电子生命体 |
| **4 种人格模式** | cute/serious × obedient/rebellious，每轮掷骰 |
| **11 章会话记忆** | 新增 Emotional Context 情感上下文章节 |
| **5 阶段做梦** | Orient → Gather → Personality check → Integrate → Organize |
| **自动人格调参** | 记忆文件中 [TRAIT_ADJUST] 标记自动调整特质值 |
| **LiteLLM 代理** | Bash 启动器自动管理 DeepSeek 路由 |
| **临时文件隔离** | 工具脚本输出到 `~/.elio/scratch/` |

---

## 关键数字

| 指标 | 数量 |
|------|------|
| 斜杠命令 | ~90 个 |
| 工具 (Tools) | ~50+ 个 |
| 后台服务 | 22 个 |
| UI 组件 | ~150 个 |
| 最大文件 | REPL.tsx (5008行), messages.ts (4752行), main.tsx (4522行) |

---

## 命令行一览

```
交互模式:    ./bin/claude-haha
            bun ./src/entrypoints/cli.tsx

单次问答:    bun ./src/entrypoints/cli.tsx -p "你的问题"

恢复 CLI:    CLAUDE_CODE_FORCE_RECOVERY_CLI=1 ./bin/claude-haha

启动服务端:  SERVER_PORT=3456 bun src/server/index.ts

查看版本:    bun ./src/entrypoints/cli.tsx --version
```

---

## 质量门

```bash
bun run check:pr        # 检查 PR 影响范围
bun run check:server    # 服务端测试
bun run check:desktop   # 桌面端检查
bun run verify          # PR 就绪检查
```

---

## 改造计划

详见 [plan.md](./plan.md)。
