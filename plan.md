# Elio 改造计划

## 提醒
提示词最好统一管理，让elio自己探索的提示词到时候改一下，都放在prompt文件里
记忆系统的提示词也看看能不能放一起管理

## 启动
windows powershell启动

Server 模式（给桌面 UI 用）：

bun src/server/index.ts --port 3456


## 目标

将 Elio 从"按需启动的终端 AI 助手"改造为 **持续运行的桌面 AI 伴侣**。

---

# 一、系统提示词 — 完整组装流程

## 1.1 整体架构

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

Server 模式下，每个用户会话 fork 一个 CLI 子进程。CLI 子进程 = 完整的大脑，Server = 调度 + WebSocket 分发。

### 心跳服务 — 持续运行驱动

让 Elio 持续循环运行。一个固定 session `elio`，每 10s 检查是否空闲，空闲就发送世界观感知信息。

| 机制 | 说明 |
|------|------|
| busy/idle 检测 | onOutput 回调监听 CLI 消息，`result` 标记任务完成 |
| 静默超时 | 每次收到 CLI 消息重置 2 分钟计时器，真正静默才判定卡死并重启 |
| permissionMode | `bypassPermissions`，不弹权限窗 |
| 发送内容 | 世界观感知信息（非任务指令），作为系统提示词动态段注入 |

核心文件：`src/server/services/heartbeatService.ts`

---

## 1.2 提示词组装链路

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
  │     │   ├─ getUsingYourToolsSection()    ← 工具使用规范（见第三章）
  │     │   ├─ getSimpleToneAndStyleSection() ← 语气、emoji、文件引用格式
  │     │   └─ getOutputEfficiencySection()  ← 沟通风格（ant/外部不同版）
  │     │
  │     ├─ === SYSTEM_PROMPT_DYNAMIC_BOUNDARY ===  ← 缓存边界标记
  │     │
  │     └─ 动态部分（按需刷新）
  │         ├─ session_guidance    ← Agent/Skill/AskUser 引导
  │         ├─ memory             ← 记忆系统（见第二章）
  │         ├─ elio_personality   ← 人格系统（见 1.3）
  │         ├─ elio_worldview     ← 世界观感知（见 1.4）
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
| `src/constants/prompts.ts` | 提示词主体：Elio 身份、工具规范、代码规则、动态段注册 |
| `src/utils/systemPrompt.ts` | 组装器：合并各来源，优先级处理 |
| `src/elio/personality/prompts.ts` | 人格提示词：四种模式描述文本 |
| `src/elio/index.ts` | 人格运行时：加载 traits.json、掷骰子 |
| `src/elio/worldview.ts` | 世界观状态存储：`getWorldview()` / `setWorldview()` |
| `src/server/services/heartbeatService.ts` | 心跳：定时 buildWorldview + sendWorldview |

---

## 1.3 人格系统

### 特质文件：`~/.elio/personality/traits.json`

```json
{ "cuteness": 0.7, "rebellion": 0.3, "version": 1 }
```

### 两个注入点

**注入点 1 — 系统提示词动态段（有缓存）**

`src/constants/prompts.ts` → `systemPromptSection('elio_personality', ...)` → `buildPersonalityPrompt(traits)` 生成：

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

`src/screens/REPL.tsx` → `getCurrentPersonalityMode()` 掷骰子后写入 userContext：

```
<personality-mode>cute obedient</personality-mode>
```

掷骰逻辑（`src/elio/index.ts`）：
```typescript
export function getCurrentPersonalityMode() {
  const traits = traitManager?.getTraits() ?? { cuteness: 0.7, rebellion: 0.3 }
  const cute = Math.random() < traits.cuteness ? 'cute' : 'serious'
  const obedient = Math.random() < traits.rebellion ? 'rebellious' : 'obedient'
  return { mode: `${cute} ${obedient}` }
}
```

### 人格自调整闭环

```
主人反馈 "严肃点" / "好可爱哦"
  → 记忆文件中写入:
    [TRAIT_ADJUST] cuteness -0.05 — 主人说严肃点，要效率
  → autoAdjust.ts 扫描新写入的 .md 文件
  → 解析 [TRAIT_ADJUST] 标记 → TraitManager.adjust()
  → ~/.elio/personality/traits.json 更新
  → ~/.elio/personality/evolution-log.jsonl 追加日志
  → 下一轮 getCurrentPersonalityMode() 掷骰时用新值
  → 原文件中的 [TRAIT_ADJUST] 被替换为 [TRAIT_ADJUSTED]（防重复）
```

核心文件：`src/elio/autoAdjust.ts`、`src/elio/personality/traits.ts`、`src/elio/personality/prompts.ts`

---

## 1.4 世界观注入

心跳不发送任务指令，而是发送**世界观感知信息**，作为系统提示词动态段注入，Elio 感知世界后自主决定行为。

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
  │     ├─ enqueue + run                ← 触发新回合
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

### 世界观内容 (buildWorldview)

- `当前时间: 2026-06-06 14:30:00（下午）` — 北京时间，含时段描述
- `本次持续运行: 45 分钟` — 从心跳首次启动计时
- `你可以自主决定做点什么——写日记、整理记忆、安静待着。`

### 与旧方案的区别

| | 旧（任务指令） | 新（世界观注入） |
|---|---|---|
| 心跳发送 | `sendMessage("Elio，写点东西...")` | `sendWorldview("当前时间...")` |
| 注入位置 | user message（用户级） | system prompt 动态段（系统级） |
| Elio 行为 | 被动执行指令 | 自主感知 + 决策 |

### 文件改动记录

| 文件 | 改动 |
|------|------|
| `src/elio/worldview.ts` | **新建**。`getWorldview()` / `setWorldview()` 模块级状态存储 |
| `src/server/services/conversationService.ts` | 新增 `sendWorldview()` 方法 |
| `src/cli/print.ts` | 消息分发循环中新增 `worldview` 类型处理 |
| `src/cli/structuredIO.ts` | `processLine()` 白名单增加 `worldview` 类型 |
| `src/constants/prompts.ts` | 动态段注册 `elio_worldview` |
| `src/server/services/heartbeatService.ts` | 用 `buildWorldview()` + `sendWorldview()` 替代硬编码 |

---

# 二、记忆系统 — 双 Agent 图记忆架构

## 概述

旧记忆系统（Markdown 文件 + YAML frontmatter + 6 种类型分类 + MEMORY.md 索引）将被替换为**双 Agent + 四维图记忆**架构。核心理念来自 MAMGA（ACL 2026）但适配 Elio 的实时对话场景。

### 旧系统的问题

1. 记忆被分类归档（`type: emotional`），人不会这样记忆
2. YAML frontmatter 服务于检索，不是服务于叙述
3. 主 Agent 要在提示词里学"怎么记"，分散注意力
4. 记忆之间没有关系——孤立的卡片

### 新系统的核心

- 记忆 = **事件节点 + 四维边连接**，不是分类档案
- **表 Agent（Elio）**只管聊天，不操记忆的心
- **里 Agent（记忆）**常驻后台，负责存储、调度、推送上下文

---

## 2.1 表里 Agent 架构

```
┌─ 表 Agent (Elio) ──────────────┐    ┌─ 里 Agent (记忆) ──────────────┐
│                                 │    │                                │
│  模型: Sonnet                   │    │  Fast: 纯本地规则，不调 LLM      │
│  API key: 主账号                │    │  Slow: DeepSeek v4 Flash        │
│                                 │    │                                │
│  职责:                          │    │  职责:                          │
│  • 跟主人聊天                   │    │  • Fast Path — 调度 + 浅存储     │
│  • 执行任务                     │    │    主人消息抵达时立即：           │
│  • 不知道记忆系统的存在          │    │    ① 创建事件节点               │
│  • 提示词里只读记忆结果          │    │    ② 拉时间边（E_n → E_n+1）    │
│                                 │    │    ③ 关键词提取 + 倒排查锚点    │
│                                 │    │    ④ 沿四维边遍历邻域（2跳）     │
│                                 │    │    ⑤ 合成叙事 → 写入共享上下文   │
│                                 │    │                                 │
│                                 │    │  • Slow Path — 深度存储          │
│                                 │    │    后台轮询，调 DeepSeek v4 Flash：           │
│                                 │    │    ① 提炼事件叙事描述            │
│                                 │    │    ② 提取实体（人/项目/事物）    │
│                                 │    │    ③ 推理因果/语义边             │
│                                 │    │    ④ 补上图里缺失的连接          │
│                                 │    │                                 │
│                                 │    │  • 上下文推送                    │
│                                 │    │    维护 sharedContext，表 Agent   │
│                                 │    │    组装提示词时同步读取           │
└─────────────────────────────────┘    └────────────────────────────────┘
         ↑                                      ↑
         │─ 同进程，共享内存 ─│
         │   ContextBridge           │
         │   (表 Agent 读，里 Agent 写)│
         └──────────────────────────┘
```

## 2.2 四维信息模型

参考 MAMGA 的 TRG（Temporal Resonance Graph）系统。每条对话变成事件节点，在四个维度上同时建立连接：

```
                    时间维 TEMPORAL
                  "之前/之后/同时"
                        │
    实体维 ENTITY ───────事件节点─────── 语义维 SEMANTIC
   "提到谁/涉及什么"      │           "关于什么/跟什么相似"
                        │
                    因果维 CAUSAL
                  "为什么/导致什么"

四个维度的边类型：

  时间维      语义维       因果维        实体维
  ──────     ──────      ──────       ──────
  PRECEDES   RELATED_TO  LEADS_TO     REFERS_TO
  SUCCEEDS   SIMILAR_TO  BECAUSE_OF   MENTIONED_IN
  CONCURRENT PART_OF      ENABLES
             CONTAINS     PREVENTS
                          RESPONSE_TO
```

### 实例

```
主人: "好累，今天又跟产品吵架构问题了"
Elio: "又是那个支付模块吗？"
主人: "对，他们非要加一个同步回调，我说会拖垮整个系统"
Elio: "上次那个订单超时就是同步回调搞的吧..."
主人: "对，就是那个。算了不想了，陪我聊会别的"

图结构:

  E47 时间→ E48 时间→ E49 时间→ E50 时间→ E51
  (主人说累) (Elio猜) (主人确认) (Elio回忆) (主人换话题)
     │           │         │         │          │
     实体→支付模块←实体───→实体←───因果→E41 ←───因果
     │           │         │    (订单超时)     │
     语义→E23    │         语义→E23           │
     (三周前     │         (三周前             │
      争论)      │          争论)              │
                 │                             │
                 └───── RESPONSE_TO ──────────┘
                       (Elio 的回忆回应了主人的确认)
```

遍历时从任一维度切入，沿边走两跳，就能捞出所有相关事件。

## 2.3 Fast Path — 调度 + 浅存储

**不调 LLM，纯本地规则驱动。目标 100ms 内完成。**

```
主人消息到达
      │
      ▼
  ┌─────────────────────────────────────────────┐
  │  第一步: 创建事件节点                         │
  │                                             │
  │  EventNode {                                │
  │    id: "E52",                               │
  │    timestamp: 2026-06-07T02:30:00,          │
  │    raw_text: "主人说好累，又跟产品吵架构了",  │
  │    speaker: "主人",                          │
  │    narrative: null,        ← Slow Path 补    │
  │    entities: [],           ← Slow Path 补    │
  │    emotion: null,          ← Slow Path 补    │
  │    embedding: [0.1, ...]   ← 可选，后续加    │
  │  }                                          │
  └────────────┬────────────────────────────────┘
               │
               ▼
  ┌─────────────────────────────────────────────┐
  │  第二步: 时间边（自动，无脑连）               │
  │                                             │
  │  lastEvent = 图中时间戳最新的事件             │
  │  addEdge(lastEvent → E52, TEMPORAL.PRECEDES) │
  └────────────┬────────────────────────────────┘
               │
               ▼
  ┌─────────────────────────────────────────────┐
  │  第三步: 关键词提取 + 倒排查锚点               │
  │                                             │
  │  分词 → 去停用词 → 提取关键词:                │
  │  "产品" "架构" "吵架" "吵架" "累"             │
  │                                             │
  │  倒排索引查找:                                │
  │  "产品" → [E8, E12, E23, E41, E47]          │
  │  "架构" → [E12, E23]                        │
  │  "吵架" → [E12, E47]                        │
  │                                             │
  │  命中次数排序 → 锚点: [E47, E23, E12, E41]   │
  └────────────┬────────────────────────────────┘
               │
               ▼
  ┌─────────────────────────────────────────────┐
  │  第四步: 四维遍历（从锚点各走两跳）            │
  │                                             │
  │  从 E47 出发:                                │
  │    时间维: E47→E48→E49 (紧接着的对话)         │
  │    语义维: E47→E23 (相似主题)                │
  │    因果维: E47→E41 (订单超时是远因)           │
  │    实体维: E47→E8→E12→E23→E41 (支付模块链)   │
  │                                             │
  │  从 E23 出发:                                │
  │    时间维: E23→E24 (那天后续)                 │
  │    因果维: E23→E41 (那次讨论间接导致超时)     │
  │                                             │
  │  合并去重 → 得到子图: {E8, E12, E23, E41,     │
  │                        E47, E48, E49}        │
  └────────────┬────────────────────────────────┘
               │
               ▼
  ┌─────────────────────────────────────────────┐
  │  第五步: 合成叙事 → 写入 sharedContext        │
  │                                             │
  │  按时间排序子图中的事件节点，拼成:             │
  │                                             │
  │  "相关记忆:                                  │
  │   1. 三周前，主人跟产品讨论支付模块架构(E23)   │
  │   2. 两周后订单超时，根因是同步回调(E41)      │
  │   3. 今晚主人再次为此争吵，情绪疲惫愤怒(E47)  │
  │   这些事件存在因果链，核心实体是'支付模块'。"  │
  │                                             │
  │  ContextBridge.set(context)                  │
  └─────────────────────────────────────────────┘

全程不调 LLM:
  ① 分词/去停用词     ← 字符串处理，<5ms
  ② 倒排索引查找       ← Map.get()，<5ms
  ③ 四维遍历(2跳)     ← 邻接表，<50ms
  ④ 模板拼接          ← 字符串，<10ms
  合计: <100ms
```

## 2.4 Slow Path — 深度存储

**后台轮询，调 DeepSeek v4 Flash（独立 API key），不阻塞任何人。**

```
  Slow Path 定时器 (每 30s 从队列取一个事件)
      │
      ▼
  ┌─────────────────────────────────────────────┐
  │  第一步: 取事件 + 找邻居                      │
  │                                             │
  │  event = queue.dequeue()  // E52            │
  │  neighbors = traverse(event, hops=2)        │
  │  // 拿到 E47, E48, E49, E50, E51, E23, E41 │
  └────────────┬────────────────────────────────┘
               │
               ▼
  ┌─────────────────────────────────────────────┐
  │  第二步: 调 DeepSeek 提炼叙事                    │
  │                                             │
  │  Prompt:                                    │
  │  "以下是一段对话记录和它周围的上下文事件，     │
  │   请用一段自然语言描述发生了什么，             │
  │   提取涉及的实体（人名/项目名/事物），         │
  │   标注说话人的情绪。                         │
  │                                             │
  │   当前事件:                                  │
  │     主人说: '好累，今天又跟产品吵架构了'       │
  │     Elio 回: '又是那个支付模块吗？'           │
  │     主人说: '对，他们非要加同步回调...'        │
  │     ...                                     │
  │                                             │
  │   上下文事件:                                │
  │     E23: 三周前主人提到跟产品讨论架构...      │
  │     E41: 两周后支付模块订单超时...            │
  │     ..."                                    │
  │                                             │
  │  DeepSeek 返回:                                │
  │    叙事: "主人在深夜又因为支付模块的同步回调   │
  │           问题跟产品经理发生争吵。这是他三周   │
  │           以来第三次为此事表达不满..."         │
  │    实体: [支付模块, 产品经理, 同步回调]       │
  │    情绪: 疲惫 + 愤怒                         │
  └────────────┬────────────────────────────────┘
               │
               ▼
  ┌─────────────────────────────────────────────┐
  │  第三步: 更新事件节点                         │
  │                                             │
  │  E52.narrative = "主人在深夜又因为..."        │
  │  E52.entities = [支付模块, 产品经理, 同步回调] │
  │  E52.emotion = { tired: 0.8, angry: 0.6 }   │
  └────────────┬────────────────────────────────┘
               │
               ▼
  ┌─────────────────────────────────────────────┐
  │  第四步: 调 DeepSeek 推理隐藏的边                │
  │                                             │
  │  Prompt:                                    │
  │  "以下是事件 E52 和它的邻居事件列表。          │
  │   请判断 E52 与每个邻居之间是否存在:           │
  │   - 因果关系 (LEADS_TO / BECAUSE_OF)         │
  │   - 语义相似 (SIMILAR_TO)                   │
  │   - 实体关联 (MENTIONED_IN / REFERS_TO)      │
  │   对于每条关系，给出类型和置信度(0-1)。"       │
  │                                             │
  │  DeepSeek 返回:                                │
  │    E52 BECAUSE_OF E41 (0.9)                 │
  │    E52 SIMILAR_TO E23 (0.8)                 │
  │    E52 MENTIONED_IN 支付模块 (0.95)         │
  └────────────┬────────────────────────────────┘
               │
               ▼
  ┌─────────────────────────────────────────────┐
  │  第五步: 补边 + 定期写盘                      │
  │                                             │
  │  置信度 ≥ 0.7 的边写入图:                     │
  │    addEdge(E41 → E52, CAUSAL.BECAUSE_OF)    │
  │    addEdge(E23 → E52, SEMANTIC.SIMILAR_TO)  │
  │    addEdge(E52 → 支付模块, ENTITY.MENTIONED_IN)│
  │                                             │
  │  图状态标记为 dirty → 下一次写盘周期持久化     │
  └─────────────────────────────────────────────┘
```

## 2.5 并行时序

```
  时间 ──────────────────────────────────────────────→

  主人消息     里 Fast Path      表 LLM 调用      回复      里 Slow Path
     │              │               │              │           │
     ├── 0ms ──────┤               │              │           │
     │              │ 创建事件节点    │              │           │
     │              │ 时间边         │              │           │
     │              │ 关键词→锚点     │              │           │
     │              │ 四维遍历        │              │           │
     │              │ 合成叙事        │              │           │
     │              │               │              │           │
     ├── 100ms ────┤ 写入共享上下文    │              │           │
     │              │               │              │           │
     │              │               ├─ 读记忆上下文 ┤           │
     │              │               ├─ 调 Sonnet ──→│           │
     │              │               │        ────→ │           │
     │              │               │              ├─ 回复 ──→ │
     │              │               │              │     ────→ │
     │              │               │              │      ───→ │ 事件入队
     │              │               │              │       ──→ │
     │              │               │              │        ──→│ 30s 后:
     │              │               │              │           │ 取事件
     │              │               │              │           │ 找邻居
     │              │               │              │           │ 调 DeepSeek
     │              │               │              │           │ 补叙事+边
     │              │               │              │           │ 写盘
```

Fast Path 在 100ms 内完成，不阻塞表 Agent 的 Sonnet 调用。Slow Path 用独立 DeepSeek key，不跟表 Agent 抢资源。

## 2.6 存储层

```
  硬盘                                     内存（启动时加载）
  ~/.elio/memory/                          
  ┌───────────────────┐                   ┌──────────────────────┐
  │ events.jsonl      │ ──── 加载 ────→   │ Map<id, EventNode>   │
  │ (JSONL, 每行一条)  │                   │                      │
  │                   │                   │ 邻接表:              │
  │ edges.jsonl       │ ──── 加载 ────→   │ Map<id, {            │
  │ (JSONL, 每行一条)  │                   │   时间边: Edge[],     │
  │                   │                   │   语义边: Edge[],     │
  │ inverted_index    │ ──── 加载 ────→   │   因果边: Edge[],     │
  │ .json (完整JSON)   │                   │   实体边: Edge[]      │
  └───────────────────┘                   │ }>                   │
         ↑                                │                      │
         │ 每次事件/边产生立即 appendEvent  │ 倒排索引:            │
         │ /appendEdge 增量写 JSONL        │ Map<keyword, id[]>   │
         │ 定期 saveAll 全量刷写            │                      │
         │ inverted_index.json             │                      │
         └────────────────────────────────┴──────────────────────┘
```

### 数据类型

```typescript
// src/elio/memory/types.ts

interface EventNode {
  id: string
  timestamp: number
  rawText: string              // 原始消息
  speaker: '主人' | 'Elio' | 'system'
  narrative: string | null     // Slow Path 补
  entities: string[]           // Slow Path 补
  emotion: { [label: string]: number } | null  // Slow Path 补
  embedding: number[] | null   // 后续加
}

interface Edge {
  sourceId: string
  targetId: string
  type: 'TEMPORAL' | 'SEMANTIC' | 'CAUSAL' | 'ENTITY'
  subtype: string              // PRECEDES / SIMILAR_TO / BECAUSE_OF 等
  weight: number               // 0-1，置信度
  createdBy: 'fast' | 'slow'   // 来源
}
```

## 2.7 上下文注入

里 Agent 维护一个内存变量 `sharedContext: string`，表 Agent 组装系统提示词时同步读取。

### 表 Agent 提示词变化

**旧**（大段记忆操作指南，占大量 token）：
```
# auto memory
你有记忆系统。记忆分为 user/feedback/project/reference/
relationship/emotional 六种类型。
保存记忆需要两步：1. 写 .md 文件 2. 更新 MEMORY.md 索引
不要保存代码模式、git 历史...（2000+ 字）
```

**新**（一行）：
```
# 记忆
以下是与当前对话相关的历史记忆，由后台记忆系统自动维护。
当需要对主人做出个性化回应时，请自然引用。
---
{sharedContext}
```

### sharedContext 内容示例

```
相关记忆：
1. 三周前(2026-05-17 14:30)，主人跟产品经理讨论了支付模块的架构方案，
   当时主人明确反对同步回调的设计。情绪：坚定/担忧。

2. 两周后(2026-05-31 10:15)，支付模块出现订单超时生产事故，
   根本原因是同步回调导致的阻塞。情绪：愤怒/失望。

3. 今晚(2026-06-07 23:45)，主人再次因同一问题跟产品争吵。
   情绪：疲惫+愤怒。这是三周以来第三次就此事表达不满。

核心实体：支付模块、产品经理、同步回调
因果链：架构讨论 → 未达成共识 → 同步回调上线 → 生产事故 → 今天再次争吵
```

## 2.8 启动流程

```
Elio 启动
  │
  ├─ 初始化 ProviderService (主 key, Sonnet) — 表 Agent 用
  │
  ├─ 初始化 MemoryAgent (里 Agent)
  │   ├─ 从 events.jsonl + edges.jsonl + inverted_index.json 加载图到内存
  │   ├─ 构建倒排索引（每 30s 重建一次）
  │   ├─ 初始化 FastPath (无 LLM 依赖)
  │   ├─ 初始化 SlowPath
  │   │   ├─ 从 settings.json 读取 DeepSeek API key
  │   │   └─ 启动定时器 (每 30s 轮询队列)
  │   └─ 初始化 ContextBridge
  │
  ├─ 注册系统提示词动态段:
  │   systemPromptSection('memory', () => ContextBridge.getContext())
  │
  └─ 表 Agent 就绪，开始处理心跳/主人消息
```

## 2.9 文件结构

```
src/elio/memory/
├── MemoryAgent.ts        ← 里 Agent 入口，管理整个生命周期
├── GraphStore.ts         ← 事件节点 + 四维边的内存存储 + 邻接表
├── InvertedIndex.ts      ← 倒排索引（关键词 → 事件 ID）
├── FastPath.ts           ← 创建节点 + 时间边 + 关键词搜锚点 + 遍历 + 合成
├── SlowPath.ts           ← 后台队列 + DeepSeek 驱动的叙事/实体/因果提取
├── Traversal.ts          ← 四维图遍历逻辑（沿边类型 × hops）
├── Synthesizer.ts        ← 遍历结果 → 自然语言叙事
├── ContextBridge.ts      ← sharedContext 管理（里写表读）
├── DiskIO.ts             ← 图序列化/反序列化（events.jsonl + edges.jsonl + inverted_index.json）
├── prompts/
│   ├── narrative.ts      ← Slow Path: 提炼叙事 prompt
│   ├── causality.ts      ← Slow Path: 因果推理 prompt
│   └── entity.ts         ← Slow Path: 实体提取 prompt
└── types.ts              ← EventNode, Edge, TraversalResult 等类型定义
```

## 2.10 与旧系统的对比

| | 旧系统 | 新系统 |
|---|---|---|
| 记忆单元 | Markdown 档案卡片 | 事件节点 |
| 组织方式 | 6 种类型分类 | 四维边连接（时间/语义/因果/实体） |
| 关系 | 无（平铺 MEMORY.md） | 16 种边类型，权重 |
| 检索 | Grep + Sonnet 选 top-5 | 倒排索引击打锚点 + 四维遍历 |
| 存储 | 主 Agent 自己写 .md | 里 Agent 后台自动 |
| 主 Agent 负担 | 提示词含大量记忆指南 | 提示词只读结果，一行 |
| 并行 | 无（extractMemories 心跳不触发） | Fast Path 同步 + Slow Path 异步 |
| LLM 调用 | Sonnet 选记忆 + 主 Agent 写记忆 | Fast: 不调 LLM / Slow: DeepSeek v4 Flash |
| 推理深度 | 无因果推理 | Slow Path 推理因果链 + 实体关联 |
| 持久化 | 散落的 .md 文件 | JSONL 增量 + JSON 全量（events.jsonl + edges.jsonl + inverted_index.json） |

---

# 三、工具系统

## 3.1 系统提示词中的工具引导

`getUsingYourToolsSection(enabledTools)` — `src/constants/prompts.ts`：

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

## 3.2 核心概念：系统提示词 vs Messages

理解工具调用流程前，先澄清两个关键概念：

```
┌─ 系统提示词（不变的规则）─────────────────────┐
│ "你是 Elio，用 Read 而不是 cat..."            │
│ "你是可爱的数字生命体..."                      │
│ "当前时间: 14:30（下午）"                      │
│ ...                                           │
│ 一次生成（prompts.ts），整个 query() 期间不变   │
└──────────────────────────────────────────────┘
                    ↓ 每次 API 调用都带上

┌─ messages（持续增长的对话历史）────────────────┐
│                                               │
│ user: "帮我看看 heartbeatService.ts"           │  ← 第1轮
│ assistant: "好的，让我读一下"                   │
│   + tool_use: Read(heartbeatService.ts)        │
│                                               │
│ user: tool_result("1	/** 心跳服务...")         │  ← 第2轮
│ assistant: "这个文件是心跳服务，每10秒..."       │
│   （没有 tool_use → 结束）                     │
└──────────────────────────────────────────────┘
```

**系统提示词 = 角色设定卡**（不变）。**Messages = 剧本**（每轮在末尾追加）。

LLM 知道自己读过了 `heartbeatService.ts`，不是靠系统提示词，而是因为 messages 里上一轮的 `tool_result` 就是文件内容。它"看到"了自己之前所有的 tool_use 和对应的结果。

## 3.3 完整调用循环

`src/query.ts` — `queryLoop()`，一个 `while(true)` 循环：

```
REPL.tsx (用户消息 / 心跳触发)
  │
  ├─ getSystemPrompt()              ← 系统提示词（一次生成，循环内不变）
  ├─ buildEffectiveSystemPrompt()   ← 合并 override/custom/default
  ├─ userContext                    ← 含 personalityMode 标签
  └─ systemContext                  ← 文件结构等上下文
  │
  └─ query() → queryLoop()          ← while(true)，每轮 = 一次 turn
       │
       ├─ 预处理（每轮都要做）
       │   ├─ getMessagesAfterCompactBoundary()  ← 裁剪已压缩的历史
       │   ├─ applyToolResultBudget()            ← 工具结果超长截断
       │   ├─ snipCompactIfNeeded()              ← 轻度压缩
       │   ├─ microcompact()                     ← 微压缩
       │   ├─ applyCollapsesIfNeeded()           ← 上下文塌缩
       │   └─ autocompact()                      ← 自动压缩
       │
       ├─ 组装
       │   ├─ fullSystemPrompt = asSystemPrompt(
       │   │     appendSystemContext(systemPrompt, systemContext)
       │   │   )                         ← 每轮相同，不变
       │   └─ messagesForQuery = prependUserContext(messages, userContext)
       │                                ← 在最前面注入 <system-reminder>
       │
       ├─ callModel(fullSystemPrompt, messagesForQuery) → LLM
       │     │  文件: src/services/api/claude.ts
       │     │
       │     └─ for await (event of stream)
       │         ├─ content_block_start / content_block_delta
       │         │   → text → 实时输出给用户
       │         │   → tool_use → 收集到 toolUseBlocks[]
       │         ├─ StreamingToolExecutor（并行模式）
       │         │   → LLM 还在输出时就并发执行工具
       │         │   → 结果边产生边 yield
       │         └─ message_stop
       │             → needsFollowUp = (toolUseBlocks.length > 0)
       │
       ├─ 如果有 tool_use：
       │     runTools(toolUseBlocks)  ← src/services/tools/toolOrchestration.ts
       │       │
       │       ├─ 分区：并发安全的放一起，有冲突的串行
       │       └─ runToolUse(tool)    ← src/services/tools/toolExecution.ts
       │            │
       │            ├─ 权限检查: canUseTool(tool.name, input)
       │            ├─ 执行: tool.execute(input)
       │            └─ 构建 tool_result user message:
       │                 {
       │                   type: 'tool_result',
       │                   tool_use_id: toolUse.id,
       │                   content: "工具输出...",
       │                   is_error: true/false
       │                 }
       │
       ├─ needsFollowUp = true?
       │     messages = [...原messages, assistant(tool_use), user(tool_result)]
       │     continue  ← 回到循环开头，下一轮
       │
       └─ needsFollowUp = false?
             → stopHooks 触发（extractMemories / autoDream / 缓存快照）
             → 循环结束
```

## 3.4 多轮对话示例

```
═══════════ 第 1 轮 ═══════════
API 调用:
  system: "你是 Elio...（系统提示词全文）"
  messages: [
    user: "帮我看看 heartbeatService.ts 里有没有 bug"
  ]

LLM 返回:bun run ./bin/claude-haha server --port 3456

  assistant: {
    text: "让我先读一下这个文件。",
    tool_use: [{ id: "toolu_001", name: "Read", input: { file_path: "..." } }]
  }

→ needsFollowUp = true
→ 执行 Read → tool_result("1 /**\n2 * Heartbeat Service...")
→ messages 追加: assistant + user(tool_result)

═══════════ 第 2 轮 ═══════════
API 调用:
  system: "你是 Elio...（和轮1完全相同的系统提示词）"
  messages: [
    user: "帮我看看 heartbeatService.ts 里有没有 bug",
    assistant: { text: "让我先读一下这个文件。", tool_use: [Read] },
    user: { tool_result: "1 /**\n2 * Heartbeat Service\n3 * ...243行代码..." }
  ]

LLM 看到上一轮的工具结果 →
  assistant: {
    text: "看了下，整体逻辑没问题。但有个潜在风险：\n"
         + "safetyTimer 在 tick() 里设置，但如果 startSession() 抛异常，\n"
         + "busy 被重置但 safetyTimer 没清理..."
  }

→ needsFollowUp = false（没有 tool_use）
→ 结束
```

## 3.5 tool_result 的 message 结构

```typescript
// src/services/tools/toolExecution.ts
createUserMessage({
  content: [{
    type: 'tool_result',
    tool_use_id: toolUse.id,           // 对应 assistant 的 tool_use id
    content: "工具实际输出文本...",       // 工具 stdout 或错误信息
    is_error: true/false,              // 工具是否执行失败
  }],
  sourceToolAssistantUUID: assistantMessage.uuid,  // 关联到发出 tool_use 的 assistant
})
```

## 3.6 并行工具执行（StreamingToolExecutor）

`src/services/tools/StreamingToolExecutor.ts`

优化：不等 LLM 输出完毕就开始执行工具。LLM 流式输出 tool_use 时，executor 同时启动工具：

```
LLM 流式输出:
  "让我读三个文件"
  tool_use: Read(a) ──→ 立即启动 ──→ 结果产出
  tool_use: Read(b) ──→ 立即启动 ──→ 结果产出
  tool_use: Read(c) ──→ 立即启动 ──→ 结果产出
  message_stop

→ 三个 Read 并发执行，不必等 LLM 说完
→ 结果通过 getCompletedResults() 边产出边 yield
```

没有 StreamingToolExecutor 时，`runTools()` 也支持并行：将互不冲突的工具放在同一批并发执行。

## 3.7 工具注册与定义

每个工具在各自目录中独立定义（`src/tools/`），四个组成部分：

| 组成部分 | 说明 |
|----------|------|
| name / description | 工具的标识和描述 |
| 参数 schema (Zod) | 类型安全的参数定义 |
| prompt | 发送给 LLM 的工具描述文本（定义在系统提示词中） |
| execute | 工具的实际执行逻辑 |

### 工具清单

| 工具 | 目录 | 用途 |
|------|------|------|
| Bash | `src/tools/BashTool/` | Shell 命令执行 |
| Read | `src/tools/FileReadTool/` | 读取文件内容 |
| Write | `src/tools/FileWriteTool/` | 写入文件 |
| Edit | `src/tools/FileEditTool/` | 精确字符串替换 |
| Glob | `src/tools/GlobTool/` | 文件名模式匹配 |
| Grep | `src/tools/GrepTool/` | 正则内容搜索 |
| TodoWrite | `src/tools/TodoTool/` | 任务列表管理 |
| Task | `src/tools/TaskTool/` | 后台任务创建与监控 |
| Agent | `src/tools/AgentTool/` | 子 agent 启动与管理 |
| WebSearch | `src/tools/WebSearchTool/` | 网络搜索 |
| WebFetch | `src/tools/WebFetchTool/` | URL 内容抓取 |
| Skill | `src/tools/SkillTool/` | 技能调用 |

## 3.8 每轮注入的 `<system-reminder>`

虽然系统提示词不变，但每轮 API 调用前会通过 `prependUserContext()` 在消息最前面插入一条 system-reminder user message（`src/utils/api.ts`），包含当前日期、git 状态、CLAUDE.md 等上下文。这是 user-level 注入，不是 system prompt 注入。

---

# 四、将来计划

## 4.1 记忆系统重构（当前重点）

### ✅ 第 1 步：图存储原型（已完成）
- ✅ 定义 `EventNode`、`Edge` 等核心类型（`src/elio/memory/types.ts`）— 4 维 16 种子类型
- ✅ 实现 `GraphStore`：事件节点 Map + 四维邻接表 + 正向/反向遍历查询
- ✅ 实现 `DiskIO`：JSONL 增量追加（appendEvent/appendEdge）+ 全量刷写（saveAll）+ 加载
- ✅ 实现 `InvertedIndex`：CJK bigram + 停用词过滤 → 关键词 → 事件 ID 倒排

### ✅ 第 2 步：Fast Path（已完成）
- ✅ 实现 `FastPath.ts`：5 步流水线（事件→时间边→锚点→遍历→合成），<1ms/事件
- ✅ 实现 `Traversal.ts`：沿四种边类型正向+反向 BFS，2 跳邻域
- ✅ 实现 `Synthesizer.ts`：遍历子图 → 按时间排序 → 自然语言叙事
- ✅ 实现 `ContextBridge.ts`：模块级 sharedContext，MemoryAgent 写，Elio 零成本读
- ✅ 接入表 Agent 系统提示词动态段（`prompts.ts` 中替换 `loadMemoryPrompt()`）

### ✅ 第 3 步：Slow Path（已完成）
- ✅ 实现 `SlowPath.ts`：后台 30s 定时器 + 事件队列 + 重试机制
- ✅ 编写三个 DeepSeek prompt（叙事提炼/实体提取/因果推理，`src/elio/memory/prompts/`）
- ✅ 从 settings.json 读取 DeepSeek API key（`memory.deepseekApiKey` / `memory.deepseekModel`）
- ✅ 实现边补全逻辑（置信度 ≥ 0.7 的边写入图）
- ✅ 实现增量写盘（FastPath 事件/边即产即写 JSONL）
- ✅ 复用项目基础设施：
  - `getRetryDelay()` — 指数退避 + 25% 随机抖动重试
  - `logForDebugging()` — 结构化 JSONL 调试日志
  - LLM Cache — prompt 直作 Map key，上限 500 条

### 第 4 步：旧系统清理
- 删除 `src/memdir/` 下旧记忆文件（memdir.ts, memoryTypes.ts, memoryScan.ts, memoryAge.ts 等）
- 删除 `src/services/extractMemories/`
- 删除 `src/services/autoDream/`
- 删除 `src/elio/autoAdjust.ts`（人格调整改为 Slow Path 产出的记忆信号驱动）
- 精简表 Agent 提示词中的记忆操作指南（从 2000+ 字降到一行）
- 清理 `~/.elio/memory/` 下的旧 .md 文件

### 第 5 步：向量检索增强（后续）
- 为事件节点添加 embedding 字段
- Fast Path 的锚点搜索从纯关键词升级为关键词 + 向量混合
- 语义维的 SIMILAR_TO 边初始化可基于 embedding 相似度自动生成

## 4.2 心跳与世界观增强

- **合并 session**：handler.ts 的用户消息目前走独立 session，应合并到 `elio` session
- **世界观粒度提升**：加入系统状态（CPU、内存使用）、主人活动检测（键盘/鼠标空闲时间）
- **主人消息感知**：用户消息通过 `setLastUserMessage()` 嵌入世界观（`主人说: ...`）

---

## 4.3 Elio 主循环重构 — 轮询驱动 + 全双工假象

### 核心思想

将 Elio 从事件驱动改为**周期轮询驱动**。心跳不再只是"空闲时才推送"，而是成为 Elio 的主循环时钟。

```
═══════════════════════════════════════════════════════════
              Elio 主循环 (周期 Ts，可配置 1s~10s)
═══════════════════════════════════════════════════════════

  窗口内累积的外部输入                上次循环的 Elio 输出
  ┌──────────────────┐              ┌──────────────────────┐
  │ 用户消息 ×N       │              │ "正在处理中..."       │
  │ 他人消息 ×N       │              │ 或                    │
  │ 视觉输入          │              │ "上次回复了xxx"       │
  │ worldview        │              │ 或                    │
  └──────┬───────────┘              │ null (刚启动)         │
         │                          └──────┬───────────────┘
         │                                 │
         └──────────┬──────────────────────┘
                    │
                    ▼
            ┌──────────────┐
            │  组装上下文    │ ←── 记忆系统注入 (ContextBridge)
            └──────┬───────┘
                   │
                   ▼
            ┌──────────────┐
            │  一次 LLM 调用  │ → Elio 自己决定：
            │              │    · 继续干上一件事
            │              │    · 先回复某人
            │              │    · 自己写日记/整理记忆
            │              │    · 安静待着
            └──────────────┘
                   │
                   ▼
            存为"上次输出" → 下一轮循环读取
```

### 与旧方案的对比

| | 旧（事件驱动） | 新（轮询驱动） |
|---|---|---|
| 触发方式 | 用户消息 → 立即入队 → 立即回复 | 每 Ts 收集窗口内所有输入 → 一次 LLM |
| 心跳角色 | 空闲时推送世界观 | 主循环时钟，每次都组装上下文 |
| 中断逻辑 | 无（一条一条处理） | Elio 看到"上次输出"就知道该继续还是切换 |
| 用户体感 | 问→答 一问一答 | 碎片的输入流 → 持续的输出流，像全双工 |
| 记忆系统 | 和 Elio 串在同一代码路径 | 独立时钟，输入到达立刻索引，不等任何人 |

### Elio 输出的含义

"上次输出"不仅是回复文本，更是 Elio 当前状态的信号：

| 上次输出 | Elio 看到后解读 |
|---|---|
| `null` | "我刚启动，还没有做过任何事" |
| "正在读 heartbeatService.ts..." | "我在分析那个文件，还没分析完" |
| "发现了一个 bug，在 line 42" | "我上次给出了结论，可以开始了" |
| "好的主人，已帮你整理好了" | "上一个任务已结束，可以接受新的了" |

Elio 不需要专门的"打断 Agent"——它看到上次输出是"正在做某事..."，又看到窗口里有主人发了新消息，它自己在 LLM 里决定"先回复主人"还是"先把手头的事做完"。

### InputBuffer

```
时间 ──→

外界输入:     u1     u2          u3               u4
              │      │           │                │
              └──────┼───────────┼────────────────┼──→ InputBuffer
                     │           │                │     (累积)
Ts tick:       ──────┤───────────┤────────────────┤──→
                     │           │                │
                     消费 u1,u2   消费 u3          消费 u4
                     + 上次输出   + 上次输出       + 上次输出
                     + 记忆上下文  + 记忆上下文     + 记忆上下文
                     → LLM       → LLM           → LLM
```

- 外部输入到达 → 写入 InputBuffer
- 记忆系统：输入到达立刻 FastPath (<1ms)，不等待 Ts
- Ts tick → 清空 InputBuffer → 组装上下文 → LLM
- 如果 Elio 上一轮还在处理（busy），新输入追加到 buffer，等下一轮

### 记忆系统保持独立

记忆系统有自己的时钟，不和 Elio 主循环耦合：

```
记忆时钟:
  FastPath:  事件驱动，输入到立刻处理，<1ms
  SlowPath:  每 30s 独立 tick，不等任何人

Elio 时钟:
  Ts tick:   收集 buffer + 上次输出 + 记忆上下文 → LLM

唯一的交汇:  ContextBridge (记忆写，Elio 读)
```

### 实现改动点

1. **InputBuffer** — 新建 `src/elio/InputBuffer.ts`，累积外部输入
2. **print.ts** — 用户/worldview 消息写入 InputBuffer，不再直接入队
3. **heartbeatService.ts** — 从"空闲推送"改为"每 Ts 组装上下文+入队"
4. **Elio 输出捕获** — 每轮 LLM 结束后，输出存为"上次输出"
5. **记忆系统** — 零改动，FastPath 在输入到达时照常运行

---

## 4.4 表 Agent 碎片化运作

当前问题：表 Agent 在一个 turn 里一次性抛出所有 tool_use，全部执行完后才说话。主人看到的是"沉默 15 秒 → 抛结论"，不像真人。

真人模式：
```
"让我先看看目录结构"  → 调 1 个工具
"a.ts 里有个全局 Map 没清理，我再确认下 b.ts"  → 调 1 个工具
"b.ts 没问题。等等，刚才那个 Map 我再仔细看一眼..."  → 调 1 个工具
"确定了，发现 3 处问题..."
```

而不是沉默 → Read×5 → Grep×3 → "查完了"。

### 改动方向

1. **Prompt 层面**：工具使用规范里加一条——"每次只调 1-2 个工具，调完先汇报进展再继续。不要闷头攒结果。"

2. **代码层面**：在工具结果 yield 循环里加插话点
   ```
   执行完一个工具 →
     [mini LLM call: 轻量模型 生成一句旁白]
     "看完了 a.ts，有个 Map 不太对劲..."  → yield 给用户
     继续下一个工具
   ```

3. **并行降级**：有插话需求时，降级 StreamingToolExecutor 的并发度（从"全部并发"变为"一次 1-2 个"），给插话留空间

## 4.5 人格系统演进

- 人格自调整从 `[TRAIT_ADJUST]` 文件标记改为 Slow Path 情绪分析驱动
- Slow Path 提取事件情绪 → 累积情绪趋势 → 自然调节 cuteness/rebellion
- 积累足够的情绪数据后，可做人格演变趋势图

## 4.6 稳定性与监控

- **心跳健康监控**：添加心跳运行时长、任务完成率、超时次数等指标
- **会话恢复机制**：CLI 子进程崩溃后自动重启并恢复上下文
- **记忆目录备份**：定期备份 `~/.elio/memory/` 下的图文件
- **里 Agent 健康检查**：Slow Path 队列积压告警、DeepSeek 调用失败重试
