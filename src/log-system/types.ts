/**
 * Agent Audit Log — 完整记录 Agent 的所有操作
 *
 * 事件类型覆盖 Agent 生命周期的每一个环节，
 * 每条日志包含毫秒级时间戳、事件类型和完整上下文。
 */

// ── 基础类型 ──────────────────────────────────────────

export type AuditEventType =
  // 会话
  | 'session.start'
  | 'session.end'
  | 'session.idle'
  // 用户交互
  | 'user.message'
  | 'user.command'
  // 上下文注入
  | 'context.system_prompt'
  | 'context.memory_loaded'
  | 'context.claude_md'
  | 'context.git_status'
  | 'context.environment'
  // API 调用
  | 'api.request'
  | 'api.thinking'
  | 'api.response'
  | 'api.stream_chunk'
  | 'api.error'
  | 'api.usage'
  // 工具调用
  | 'tool.invoke'
  | 'tool.result'
  | 'tool.error'
  // 记忆操作
  | 'memory.recall'
  | 'memory.save'
  | 'memory.consolidate'
  // 人格 & 情感
  | 'personality.snapshot'
  | 'personality.evolution'
  | 'emotion.snapshot'
  | 'emotion.trigger'
  // 反思 & 梦境
  | 'dream.start'
  | 'dream.end'
  | 'dream.insight'
  // 系统
  | 'system.error'
  | 'system.warning'
  | 'system.info'

// ── 事件结构 ──────────────────────────────────────────

export interface AuditEvent {
  /** ISO-8601 时间戳 (毫秒精度) */
  timestamp: string
  /** 事件类型 */
  type: AuditEventType
  /** 会话 ID */
  sessionId: string
  /** turn 序号 (从 1 开始) */
  turn: number
  /** 事件载荷 (因 type 而异) */
  payload: Record<string, unknown>
}

// ── 具体载荷类型 ──────────────────────────────────────

export interface SessionStartPayload {
  model: string
  provider: string
  cwd: string
  platform: string
  hostname: string
  personality_traits?: Record<string, number>
  emotional_state?: string
}

export interface UserMessagePayload {
  content: string
  isCommand: boolean
  messageLength: number
  attachments?: string[]
}

export interface SystemPromptPayload {
  totalLength: number
  sections: string[]
  hash: string
}

export interface MemoryLoadedPayload {
  count: number
  files: string[]
  totalChars: number
}

export interface ApiRequestPayload {
  model: string
  messageCount: number
  toolCount: number
  toolNames: string[]
  thinkingEnabled: boolean
  maxOutputTokens: number
  systemPromptChars: number
  totalInputChars: number
  requestId?: string
}

export interface ApiThinkingPayload {
  content: string
  charCount: number
  requestId?: string
}

export interface ApiResponsePayload {
  content: string
  charCount: number
  stopReason: string
  requestId?: string
}

export interface ApiUsagePayload {
  inputTokens: number
  outputTokens: number
  cacheReadTokens?: number
  cacheWriteTokens?: number
  costUSD: number
  requestId?: string
}

export interface ToolInvokePayload {
  toolName: string
  input: Record<string, unknown>
  isDestructive: boolean
  startTime: string
}

export interface ToolResultPayload {
  toolName: string
  success: boolean
  outputSummary: string
  outputCharCount: number
  durationMs: number
  error?: string
}

export interface MemorySavePayload {
  count: number
  files: Array<{
    name: string
    type: string
    description: string
  }>
}

export interface PersonalitySnapshotPayload {
  traits: Record<string, number>
  version: number
  sinceLastChange: string
}

export interface EmotionSnapshotPayload {
  valence: number
  arousal: number
  dominance: number
  labels: string[]
}

// ── 配置 ──────────────────────────────────────────────

export interface AuditLogConfig {
  /** 日志根目录 (默认 ~/.claude/log-system/) */
  logDir: string
  /** 是否记录 thinking 内容 (可能很长) */
  captureThinking: boolean
  /** 是否记录完整的 API 请求 (可能包含敏感信息) */
  captureFullApiRequest: boolean
  /** 工具结果的截断长度 (0 = 不截断) */
  toolResultMaxChars: number
  /** 写入缓冲大小 (行数) */
  flushBufferSize: number
  /** 自动刷新间隔 (毫秒) */
  flushIntervalMs: number
}

export const DEFAULT_AUDIT_CONFIG: AuditLogConfig = {
  logDir: '',
  captureThinking: true,
  captureFullApiRequest: false,
  toolResultMaxChars: 5000,
  flushBufferSize: 10,
  flushIntervalMs: 5000,
}
