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

---

## 心跳服务（已实现）

让 Elio 持续循环运行。一个固定的 CLI session `elio`，心跳每 10s 检查是否空闲，空闲就递活。

### 架构

```
服务端启动
  └─ heartbeatService.start()
       ├─ 创建固定 session "elio"，fork CLI
       ├─ 读取 runtime config (provider/model/effort/thinking)
       └─ setInterval(10s)
            ├─ busy? → skip
            └─ idle? → sendMessage("写点东西")
```

用户 WebSocket 会话完全独立，不参与心跳。

### 文件

| 文件 | 作用 |
|------|------|
| `src/server/services/heartbeatService.ts` | 心跳核心：10s 定时器、忙/闲检测、session 生命周期 |
| `src/server/index.ts` | 启动/停止心跳，挂载到 `cleanupAllSessions` |

### 关键设计

- **busy/idle 检测**：onOutput 回调监听 CLI 消息，`result` 类型标记任务完成，2 分钟安全超时防死锁
- **permissionMode: 'bypassPermissions'**：心跳递活不弹权限窗
- **Runtime config**：启动 session 前查 `SettingsService` + `ProviderService`，获取当前 provider/model，确保 CLI 有有效 API 凭证

### 不碰的文件

- `handler.ts` — 不改任何逻辑
- `conversationService.ts` — 只调已有 API
- 用户 WebSocket 流程 — 完全不受影响

---

## 现有人格系统

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

---

## 六层记忆

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

## 今后计划

- 心跳任务多样化：整理记忆、回顾对话、主动学习
- 权限审批：心跳 session 目前 bypassPermissions，后续支持 UI 端审批
- 沉默权：Agent 可以决定不回复，作为人格系统的一个维度
- 语音输出 (TTS) 和 Live2D 虚拟形象（远期）
