# Elio 改造计划

## 目标

将 Elio 从"按需启动的终端 AI 助手"改造为 **持续运行的桌面 AI 伴侣**，具备语音输出 (TTS) 和 Live2D 虚拟形象。

---

## 当前架构分析

### 两套运行模式

```
模式1：单机 CLI（日常开发用）
  bun ./src/entrypoints/cli.tsx
  └─ Ink TUI ←→ LLM (一问一答，阻塞等待)

模式2：Server 模式（给桌面 UI 用的，已有但不完善）
  bun ./src/entrypoints/cli.tsx server --port 3456
  └─ HTTP+WS 服务器
       ├─ 为每个会话 fork CLI 子进程当 "大脑"
       └─ 前端通过 WebSocket 连上来收发消息
```

### CLI 子进程的角色

Server 模式下，每个用户会话 fork 一个 CLI 子进程：

```
用户消息 (WebSocket)
  → Server → fork CLI 子进程 (headless 模式)
       ├─ 系统 prompt (含 Elio 人格)
       ├─ 工具调用 (Bash/Edit/Read/Write...)
       ├─ 记忆读写 (memdir)
       ├─ 权限弹窗
       └─ 流式返回结果
```

CLI 子进程 = 完整的大脑。Server = 调度 + WebSocket 分发。前端只管显示。

### 现有人格系统

```
~/.elio/personality/traits.json
  ├─ cuteness: 0.7   (0=严肃, 1=可爱)
  └─ rebellion: 0.3  (0=听话, 1=叛逆)

每轮对话掷骰子：
  Math.random() < cuteness  →  cute  或  serious
  Math.random() < rebellion →  rebellious 或  obedient

四种模式：
  cute obedient  |  cute rebellious
  serious obedient  |  serious rebellious

→ <personality-mode> 标签注入系统 prompt
→ [TRAIT_ADJUST] 标记实现自动演化
```

### 六层记忆

```
1. CLAUDE.md       → 指令记忆（规则级）
2. Message[]       → 短期记忆（会话级）
3. Task/Todo       → 工作记忆（任务级）
4. memdir 知识库    → 长期记忆（持久级）★ 核心
5. Session Memory  → 摘要记忆（压缩级）
6. AutoDream       → 休眠重塑（离线级）
```

6 种记忆类型：user / feedback / project / reference / relationship / emotional

---

## Phase 1：持续事件循环

### 设计原则

- **零侵入**：不动 CLI、不动人格系统、不动消息处理流程
- **纯增量**：只在 Server 进程加一个旁路定时器
- **共享记忆**：事件循环和 CLI 子进程读同一个 memdir，通过文件系统自然共享

### 架构

```
┌─ Server 进程（持续运行）──────────────────────────┐
│                                                    │
│  ┌──────────────────┐  ┌──────────────────────┐   │
│  │ 问答管线（不动）    │  │ 事件循环（新增）       │   │
│  │                  │  │                      │   │
│  │ 用户消息 → fork   │  │ 每秒 tick → 检查空闲  │   │
│  │ CLI → LLM → 回复  │  │ → 超阈值 → 生成话题    │   │
│  │                  │  │ → 推送 WebSocket      │   │
│  └────────┬─────────┘  └────────┬─────────────┘   │
│           │                     │                  │
│           └──────────┬──────────┘                  │
│                      ▼                             │
│               WebSocket :3456                      │
│                      │                             │
│          共享：memdir / traits.json                 │
└────────────────────────────────────────────────────┘
                         │
              ┌──────────┼──────────┐
              ▼          ▼          ▼
           网页聊天    CLI (照旧)   未来客户端
```

### 新增文件

```
src/services/eventLoop/
├── EventLoop.ts         # 主状态机 + 调度
├── BoredomEngine.ts     # 空闲计时 + 状态阈值（带随机抖动）
└── TimeAwareness.ts     # 深夜判断

static/
└── index.html           # 简单聊天测试页
```

### 修改文件

| 文件 | 改动 | 行数 |
|------|------|------|
| `src/server/index.ts` | startServer() 末尾初始化 EventLoop | +5 |
| `src/server/ws/handler.ts` | handleUserMessage 里加 `eventLoop.onUserInput()` | +1 |
| `src/server/ws/events.ts` | ServerMessage 加 `elio_proactive` 子类型 | +3 |

### BoredomEngine：状态机

```
空闲时间 →
  0 ──── ~2min ──── ~10min ──── ~30min ────→
  ACTIVE         IDLE         LONELY       DREAMING
  不发言         偶尔说(8-12min冷却) 主动找话题(4-6min冷却)  安静+触发AutoDream
```

**随机抖动**：所有阈值不是固定值，状态切换时重新随机生成——

| 参数 | 基础值 | 随机范围 |
|------|--------|---------|
| ACTIVE → IDLE | 120s | 60~180s |
| IDLE → LONELY | 600s | 420~780s |
| LONELY → DREAMING | 1800s | 1200~2400s |
| IDLE 发言冷却 | 600s | 480~720s |
| LONELY 发言冷却 | 300s | 240~360s |

### TimeAwareness

只做一件事：深夜 (23:00 ~ 06:00) 禁止主动发言。用户正常问答不受影响。

### 主动消息生成

```
EventLoop.tick() → 超阈值
  → 读 memdir 最近 5 条记忆
  → 调 getCurrentPersonalityMode() 掷骰子
  → 构造 prompt（含人格标签 + 记忆 + 时间上下文）
  → 调 LLM（直接 HTTP 调 LiteLLM，不走 CLI 子进程）
  → WebSocket 推送给所有连接的客户端
```

主动消息不需要工具调用能力。只生成短文本话题。

### 聊天测试页

纯 HTML，无框架，原生 WebSocket：
- 用户发消息 → 正常问答（走 CLI 子进程完整流程）
- 事件循环推送 → `elio_proactive` 消息，斜体灰字显示
- 自动生成匿名 session ID

### 和 CLI 的兼容

CLI 单机版完全不动。网页聊天和 CLI 通过共享 memdir 实现记忆互通：

```
CLI 里说"我喜欢猫" → 写入 memdir
关掉 CLI，打开网页 → 事件循环读到 memdir → "主人你还喜欢猫吗？"
```

---

## Phase 2：人格系统增强

### 当前问题（Phase 1 实践中发现）

Phase 1 事件循环接入人格系统后，暴露了现有人格机制的局限：

| 问题 | 表现 | 影响 |
|------|------|------|
| **无上下文感知** | 深夜/清晨/空闲太久，用的都是同一套概率（cuteness 0.7 / rebellion 0.3），和时间、BoredomEngine 状态毫无关联 | 凌晨 3 点 Elio 突然用 cute rebellious 模式说话，违和 |
| **无连续性** | 每次独立掷骰，上一轮 cute obedient 下一轮突然 serious rebellious，中间没有过渡 | 人格跳变生硬，不像同一个"生命体" |
| **变化太频繁** | 每轮对话都重新掷骰，事件循环主动发言也掷骰，一天可能变几十次 | 用户感知不到稳定的"性格" |
| **无外部触发** | 除了手动在记忆中写入 `[TRAIT_ADJUST]`，traits 永远不变 | 人格不会因为"主人三天没理我"或者"主人今天夸了我"而自然演化 |
| **事件循环盲区** | 主动发言用了人格标签，但主人是否回应、回应了什么，都不影响下一次的人格选择 | 缺乏"互动 → 情感反馈 → 人格调整"的闭环 |
| **状态栏伪变化** | `getCurrentPersonalityMode()` 每秒掷骰（已在 Phase 1 修复为缓存机制） | — |

### 2.1 沉默权

Agent 可以决定不回复。不是塞进消息处理流程，而是作为人格系统的一个维度。

```
traits.json 加：
  responsiveness: 0.8   (0=高冷, 1=话痨)

getCurrentPersonalityMode() 掷三个骰子：
  cute/serious × obedient/rebellious × respond/silent

prompt 里：
  <personality-mode>cute rebellious silent</personality-mode>

LLM 看到 silent → 本轮不回复
前端显示 "Elio 看了一眼，没说话"
```

影响 `shouldRespond` 的因素：
- rebellion 越高 → 越可能沉默
- 消息越短 → 越可能被无视
- 消息含请求/疑问语气 → 提高回复概率
- 深夜 → 更倾向简短回应

### 2.2 情绪波动层

在基础特质值上叠加随时间变化的情绪，不替代现有骰子系统，而是影响骰子概率。

```
MoodState:
  energy:     交互后高 → 空闲衰减 → 影响发言频率
  attention:  主人互动 → 升高，空闲 → 衰减
  curiosity:  记忆丰富时偏高 → 影响主动话题倾向

实际骰子概率 = 基础值 × mood 修正
  例：idle 太久 → energy↓ → cuteness 暂时↑（更粘人）
      刚互动完 → attention↑ → 高能量短回复
```

---

## Phase 3：语音输出 (TTS)

### 方案

Edge TTS（免费、Windows 原生、中文好），通过 Python 子进程调用。

```
bun 主进程
  └── Python 子进程 (edge-tts)
       stdin ← 待合成文字
       stdout → 音频流 (mp3 分块)
                → WebSocket → 前端 Web Audio API 播放
```

### 流式策略

每攒够一个短语 → 发 TTS → 播放。不等整句说完。

### 新增文件

```
src/services/tts/
├── ttsEngine.ts     # TTS 抽象 + Edge TTS 实现
├── ttsStream.ts     # 流式合成
└── voicePreset.ts   # 声线预设
```

---

## Phase 4：Live2D 虚拟形象

### 技术选型

| 组件 | 方案 |
|------|------|
| Live2D 渲染 | pixi-live2d-display (PixiJS 插件) |
| 通信 | WebSocket (复用已有 server :3456) |
| 启动 | Chrome `--app` 模式 → 无边框桌面窗口 |

### 前端结构

```
static/avatar/
├── index.html           # 主页面（透明背景 + 悬浮窗口）
├── js/
│   ├── app.js           # WS 连接 + 初始化
│   ├── live2d/
│   │   ├── modelLoader.js
│   │   ├── expression.js   # 情绪 → 表情参数映射
│   │   └── lipSync.js      # 音频 → 口型参数
│   └── audio/player.js     # Web Audio API 播放
└── models/elio/            # Live2D 模型文件
```

### 表情映射

```
人格模式 → 表情:
  cute obedient     → 笑脸 + 眨眼 + 晃动
  cute rebellious   → 俏皮 + 吐舌 + 前倾
  serious obedient  → 平静 + 点头
  serious rebellious → 微皱眉 + 偶尔叹气
```

### 口型同步

TTS 音频 → Web Audio API AnalyserNode → 实时 RMS 音量 → Live2D PARAM_MOUTH_OPEN_Y

---

## 今后讨论

- Phase 5：记忆驱动的主动话题（从 memdir 拉记忆 → 打分 → 生成自然话题）
- Phase 6：长期意图规划（Elio 自己的 goal list，Dream 系统提出，运行时推进）
- Phase 7：桌面活动感知（可选 opt-in，Computer Use 模块截图 → 偶尔评论）

---

## 不做的

- 直播弹幕 / 聊天室功能
- Electron 桌面外壳（已有 desktop/ 但 bug 多，改用 Web 方案）
- 改 CLI REPL 的消息处理流程（Phase 1 零侵入）
- 事件循环调 LLM 时走 CLI 子进程（不需要工具调用，直接 HTTP 更轻量）
