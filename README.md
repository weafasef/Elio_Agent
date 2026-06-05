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

## 项目来源

本仓库基于 2026-03-31 从 Anthropic npm registry 泄露的 Claude Code 源码，由 [NanmiCoder](https://github.com/NanmiCoder/cc-haha) 维护开源。原始源码版权归 [Anthropic](https://www.anthropic.com) 所有，仅供学习和研究用途。

---

## 改造计划：Elio → 持续运行桌面伴侣

### 目标定位

将 Elio 从"按需启动的终端 AI 助手"改造为**7×24 持续运行的桌面 AI 伴侣**，具备语音输出和 Live2D 虚拟形象。不接入直播弹幕，纯粹作为个人桌面陪伴。

### 架构演进

```
当前架构                          目标架构
─────────                        ─────────

REPL 循环                        持续 Event Loop
(等输入→回复→等待)                (idle → 自主发言 → idle → 自主发言 → ...)
                                     ↑
                                     │ 用户随时打断输入
                                     │
┌──────────────┐                 ┌──────────────────────────────┐
│  Ink TUI     │                 │  Web 前端 (Chrome --app)       │
│  (终端渲染)   │                 │  ┌─────────┐ ┌─────────────┐ │
│              │                 │  │ Live2D  │ │  Web Audio  │ │
│              │                 │  │ Avatar  │ │  TTS Player │ │
│              │                 │  └────┬────┘ └──────┬──────┘ │
└──────────────┘                 └───────┼──────────────┼────────┘
                                         │   WebSocket  │
                               ┌─────────┼──────────────┼────────┐
                               │  src/server/ (已有, 端口 3456)    │
                               │  ┌────────────────────────────┐ │
                               │  │  Continuous Event Loop NEW │ │
                               │  │  ┌──────────┐ ┌─────────┐ │ │
                               │  │  │ 用户输入  │ │ 无聊引擎 │ │ │
                               │  │  │ Handler  │ │ · 空闲N分│ │ │
                               │  │  └────┬─────┘ │ · 情绪波 │ │ │
                               │  │       │       │ · 记忆弹 │ │ │
                               │  │       ▼       └────┬────┘ │ │
                               │  │  ┌─────────────────▼───┐  │ │
                               │  │  │    Elio 核心管线    │  │ │
                               │  │  │ · elio/ 人格 + 情绪 │  │ │
                               │  │  │ · memdir/ 记忆上下文 │  │ │
                               │  │  │ · DeepSeek API 调用  │  │ │
                               │  │  └─────────┬───────────┘  │ │
                               │  │  ┌─────────▼───────────┐  │ │
                               │  │  │   输出分发           │  │ │
                               │  │  │ · 文字 → WS → 前端  │  │ │
                               │  │  │ · TTS音频流          │  │ │
                               │  │  │ · 情绪标签 → 表情    │  │ │
                               │  │  └─────────────────────┘  │ │
                               │  └────────────────────────────┘ │
                               └─────────────────────────────────┘
```

---

### Phase 1：持续运行核心循环 ⏳

**目标**：打破 REPL 的"等输入→回复→等输入"阻塞模式，让 Elio 自主运行。

#### 1.1 Event Loop 改造

**现状**：`src/screens/REPL.tsx` (5008行) 中，`PromptInput` 组件阻塞等待用户输入，提交后调 API，渲染完再等下一次输入。

**改造点**：

| 文件 | 改动 | 说明 |
|------|------|------|
| **新建** `src/services/eventLoop/` | EventLoop 类 | 核心循环引擎，取代 REPL 的单向等待 |
| `src/screens/REPL.tsx` | 集成 EventLoop | 用户输入作为 EventLoop 的中断事件，而非唯一驱动 |
| **新建** `src/services/eventLoop/boredomEngine.ts` | 无聊引擎 | 空闲计时器，N 分钟无人交互触发自主行为 |
| **新建** `src/services/eventLoop/moodWave.ts` | 情绪波动 | cuteness/rebellion 随时间自然浮动 + 影响发言频率/风格 |
| **新建** `src/services/eventLoop/timeAwareness.ts` | 时间感知 | 几点、星期几、季节、上次互动距今多久 |

**Event Loop 伪代码**：

```
while (running) {
  if (有用户输入挂起)      → 立即处理，优先级最高
  else if (无聊计时器到期)  → 自主发言 (基于情绪+记忆+时间)
  else if (情绪波峰)       → 主动想法 (基于当前 mood)
  else                    → sleep(短间隔)
}
```

#### 1.2 无聊引擎

```
空闲状态机:
  ACTIVE (0~2min)     → 无特殊行为，等用户接着说
  IDLE (2~10min)      → 每 3min 可能轻声提醒 ("主人还在吗？")
  LONELY (10~30min)   → 基于记忆产生话题 ("上次我们在聊...")
  DREAMING (30min+)   → 进入低功耗模式，偶尔自言自语
```

**触发内容来源**：
- 随机翻阅 memdir 记忆 → "想起来以前..."
- 当前时间/天气相关的问候 → "已经凌晨了，主人还不休息吗？"
- 上次对话未完成的话题 → "主人刚才说到的那个 bug..."
- 人格骰子驱动的随机想法 → 一次新的骰子 → 一次自主发言

#### 1.3 情绪波动曲线

**现状**：cuteness/rebellion 是静态值 + `[TRAIT_ADJUST]` 手动调。

**改造**：新增情绪波动层，在基础特质值上叠加随时间变化的正弦波。

```typescript
// 情绪模型
interface MoodState {
  baseCuteness: number      // 基础值 (traits.json)
  baseRebellion: number     // 基础值
  energy: number            // 精力 0-1，随时间 + 交互衰减
  attention: number         // 主人关注度 0-1，交互后升高，空闲衰减
  curiosity: number         // 好奇心 0-1，记忆丰富时偏高
}

// 实际骰子概率 = 基础值 × 情绪修正
// idle太久 → energy↓ → 更少发言但更粘人（cuteness↑）
// 刚互动完 → attention↑ → 高能量表达
```

**文件**：
- **新建** `src/services/eventLoop/moodWave.ts` — 情绪模型 + 每 tick 更新
- **新建** `src/services/eventLoop/moodPrompts.ts` — 情绪 → 提示词修饰
- **修改** `src/elio/index.ts` — `getCurrentPersonalityMode()` 叠加 mood 修正

#### 1.4 时间感知

```
Elio 始终知道现在是几点、星期几、季节、距上次互动多久。
这些信息注入到上下文，影响她的发言内容。
```

**实现**：
- **新建** `src/services/eventLoop/timeAwareness.ts`
- 每次构建 userContext 时注入当前时间信息
- 无聊引擎根据时间调整发言主题（早上问候、深夜提醒休息）

#### 1.5 CLI 交互模式保留

在持续运行模式下，保留了终端 CLI 接入方式：
- 用户在终端输入 → 作为高优先级中断注入 EventLoop
- Elio 回复 → 一边显示在终端，一边推送给 Web 前端
- 可以完全无 UI 运行（仅 CLI），也可以连接 Web 前端

---

### Phase 2：语音输出 (TTS)

**目标**：Elio 能"说话"，不再只是文字。

#### 2.1 TTS 引擎选择

| 方案 | 优点 | 缺点 |
|------|------|------|
| **Edge TTS** (推荐) | 免费、Windows 原生、中文好、`edge-tts` Python 包直接用 | 需要 Python 进程 |
| Fish-Speech | 本地部署、可克隆声线、延迟低 | 需要 GPU，部署复杂 |
| GPT-SoVITS | 少样本克隆、情感控制好 | 同上 |

当前选择：**Edge TTS**，后续可扩展声线克隆。

#### 2.2 实现方案

```
bun 主进程
  │
  ├── WebSocket → 前端播放音频
  │
  └── Python 子进程 (edge-tts)
        │
        stdin ← 待合成文字
        stdout → 音频流 (mp3/opus 分块)
```

**新建/修改文件**：

| 文件 | 改动 |
|------|------|
| **新建** `src/services/tts/` | TTS 服务目录 |
| `src/services/tts/ttsEngine.ts` | TTS 抽象接口 + Edge TTS 实现 |
| `src/services/tts/ttsStream.ts` | 流式：边生成文字边合成音频 |
| `src/services/tts/voicePreset.ts` | 声线预设 (语速、音调、中文女声选择) |
| **修改** `src/server/ws/handler.ts` | WS 增加 `audio` 消息类型 |
| **新建** `src/server/api/tts.ts` | TTS HTTP 端点 |

#### 2.3 流式 TTS

```
用户看到:  文字逐 token 渲染 (保持现有)
Elio 说:   每攒够一个短语 → 发 TTS → 播放
           (不是等整句说完才开始，降低延迟)
```

---

### Phase 3：Live2D 虚拟形象

**目标**：Elio 有一个看得见的"身体"。

#### 3.1 技术选型

| 组件 | 方案 |
|------|------|
| Live2D 渲染 | [pixi-live2d-display](https://github.com/non-npc/pixi-live2d-display) (PixiJS 插件) |
| 3D 备选 | Three.js + VRM (VRoid Studio 免费制作) |
| 运行时 | 浏览器 (Chrome `--app` 模式 → 无边框窗口) |
| 通信 | WebSocket (复用已有 server，端口 3456) |

#### 3.2 前端页面结构

```
Web 前端 (纯静态, server/public/avatar/)
├── index.html              # 主页面
├── css/
│   └── avatar.css          # 透明背景 + 悬浮窗口样式
├── js/
│   ├── app.js              # 初始化 + WS 连接
│   ├── live2d/
│   │   ├── modelLoader.js  # 加载 Live2D 模型文件
│   │   ├── expression.js   # 面部表情映射
│   │   └── lipSync.js      # 音频 → 口型参数同步
│   ├── audio/
│   │   └── player.js       # Web Audio API 播放 TTS 音频
│   └── chat/
│       └── overlay.js      # 可选: 浮层字幕显示
├── models/
│   └── elio/               # Elio 的 Live2D 模型文件
│       ├── elio.model3.json
│       └── textures/
└── sounds/
    └── idle/               # 环境音效 (可选)
```

#### 3.3 表情映射

**Elio 情绪 → Live2D 表情参数**：

```
personality mode → 表情:
  cute obedient    → 开心笑脸 + 眨眼 + 轻微晃动
  cute rebellious → 俏皮表情 + 偶尔吐舌 + 身体前倾
  serious obedient → 平静表情 + 点头 + 稳定姿态
  serious rebellious→ 微皱眉 + 眼神犀利 + 偶尔叹气

emotion tag → 表情覆盖:
  happy   → 笑颜 + 眼睛弯月
  sad     → 垂眉 + 嘴角向下
  curious → 歪头 + 瞪大眼睛
  sleepy  → 半闭眼 + 打哈欠
  excited → 眼睛发光 + 身体弹跳
```

#### 3.4 口型同步

```
TTS 音频流
  ↓
Web Audio API AnalyserNode
  ↓ 实时 RMS 音量
音量高低 → 嘴巴开合度 (Live2D PARAM_MOUTH_OPEN_Y)
  ↓
Live2D 模型更新 → 随语音节奏张嘴
```

#### 3.5 前端构建与服务

- 纯静态文件，无需 Webpack/Vite，直接放在 `src/server/public/avatar/`
- 已有 `src/server/` 静态文件服务，加路由 `/avatar` → 返回 avatar 页面
- 启动方式：`chrome --app=http://localhost:3456/avatar`（无边框桌面窗口）

---

### Phase 4：更深层的自主性

#### 4.1 记忆驱动的主动话题

```
无聊引擎触发 → 从 memdir 随机拉取 N 条记忆
  → Sonnet 打分 (趣味性/时效性/情感关联)
  → 选最优记忆 → 生成自然话题
  → "主人，我突然想起去年这个时候..."
```

#### 4.2 长期意图规划

```
Elio 自己维护一个 goal list (类似用户的 todo):
  - "学会理解主人的情绪模式"
  - "每次主人回家时主动打招呼"
  - "记住主人喜欢喝的咖啡，下次提起"

这些 goal 由 Dream 系统在离线整理时提出，
日常运行时 Elio 会在合适时机推进它们。
```

#### 4.3 桌面活动感知

- 已有 Computer Use 模块 (`src/utils/computerUse/`) 可截图
- Elio 偶尔看一眼屏幕 → "主人你在写代码呀？看起来是个大项目"
- 注意隐私：此功能需明确 opt-in，且只在本地处理

---

### 实施优先级总览

```
Phase 1 (当前)
├── 1.1 Event Loop 改造      ← 架构核心，必须先做
├── 1.2 无聊引擎              ← 自主性的起点
├── 1.3 情绪波动              ← 让 Elio "活"起来
└── 1.5 CLI 交互保留         ← 确保不破坏现有功能

Phase 2
├── 2.1 Edge TTS 集成         ← 语音是"伴侣感"的关键
└── 2.3 流式 TTS             ← 延迟越低体验越好

Phase 3
├── 3.1 前端骨架              ← HTML + WS 连接
├── 3.2 Live2D 模型           ← 视觉形象
├── 3.3 表情映射              ← 让形象和人格一致
└── 3.4 口型同步              ← 说话的"真实感"

Phase 4
└── 逐步迭代                  ← 在 Phase 1-3 稳定后
```
