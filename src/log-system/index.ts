/**
 * Agent 审计日志引擎
 *
 * 使用方式：
 *   const audit = getAuditLogger()
 *   await audit.init()
 *   audit.userMessage('hello')
 *   audit.toolInvoke('Bash', { command: 'ls' })
 *   await audit.shutdown()
 *
 * 日志写入 <项目>/logs/YYYY-MM-DD.jsonl
 * 一天一个文件，同一天多次启动会续写到同一文件。
 * 采用缓冲写入 + 定时刷新，避免阻塞主循环。
 */

import { appendFile, mkdir } from 'fs/promises'
import { join } from 'path'
import { randomUUID } from 'crypto'
import type {
  AuditEvent,
  AuditEventType,
  AuditLogConfig,
} from './types.js'
import { DEFAULT_AUDIT_CONFIG } from './types.js'

// ── 全局单例 ──────────────────────────────────────────

let _instance: AuditLogger | null = null

export function getAuditLogger(): AuditLogger {
  if (!_instance) {
    _instance = new AuditLogger()
  }
  return _instance
}

export function initAuditLogger(config?: Partial<AuditLogConfig>): AuditLogger {
  _instance = new AuditLogger(config)
  return _instance
}

// ── 日期工具 ──────────────────────────────────────────

/** 生成按天的日志文件名，如 2026-06-04.jsonl */
function dailyLogFileName(now: Date): string {
  const y = now.getFullYear()
  const m = String(now.getMonth() + 1).padStart(2, '0')
  const d = String(now.getDate()).padStart(2, '0')
  return `${y}-${m}-${d}.jsonl`
}

// ── 日志引擎 ──────────────────────────────────────────

export class AuditLogger {
  private config: AuditLogConfig
  private sessionId: string
  private startedAt: string
  private turn = 0
  private logFilePath = ''
  private buffer: string[] = []
  private flushTimer: ReturnType<typeof setInterval> | null = null
  private initialized = false

  constructor(config?: Partial<AuditLogConfig>) {
    this.config = { ...DEFAULT_AUDIT_CONFIG, ...config }
    this.sessionId = randomUUID()
    this.startedAt = new Date().toISOString()
  }

  // ── 初始化 ──────────────────────────────────────────

  /** 初始化日志文件。必须在 session 开始前调用。一天一个文件，同一天续写。 */
  async init(sessionId?: string): Promise<void> {
    if (this.initialized) return

    if (sessionId) this.sessionId = sessionId

    const logDir =
      this.config.logDir || join(process.cwd(), 'logs')
    await mkdir(logDir, { recursive: true })

    // 按天命名文件：如 logs/2026-06-04.jsonl
    const fileName = dailyLogFileName(new Date())
    this.logFilePath = join(logDir, fileName)
    this.initialized = true

    // 启动定时刷新
    this.flushTimer = setInterval(() => {
      this.flush().catch(() => {})
    }, this.config.flushIntervalMs)

    // 注册进程退出钩子，确保退出前刷盘（-p 模式进程直接退出，不调 shutdown）
    const gracefulFlush = () => {
      // 同步写入，确保在进程退出前完成
      if (this.buffer.length === 0) return
      const lines = this.buffer.splice(0)
      try {
        const { appendFileSync } = require('fs')
        appendFileSync(this.logFilePath, lines.join('\n') + '\n', 'utf8')
      } catch {}
    }
    process.on('exit', gracefulFlush)
    process.on('SIGINT', () => { gracefulFlush(); process.exit(0) })
    process.on('SIGTERM', () => { gracefulFlush(); process.exit(0) })

    // 写入 session 开始事件，并立即刷盘确保文件创建
    await this.event('session.start', {
      sessionId: this.sessionId,
      startedAt: this.startedAt,
      logFile: fileName,
    })
    await this.flush()
  }

  // ── 核心 API ──────────────────────────────────────────

  /** 记录一个事件 */
  async event(
    type: AuditEventType,
    payload: Record<string, unknown> = {},
  ): Promise<void> {
    if (!this.initialized) return

    const entry: AuditEvent = {
      timestamp: new Date().toISOString(),
      type,
      sessionId: this.sessionId,
      turn: this.turn || 1,
      payload,
    }

    // 序列化为一行 JSON
    const line = JSON.stringify(entry)

    // 同步输出到 stderr（实时调试用）
    if (process.env.LOG_SYSTEM_DEBUG) {
      // biome-ignore lint/suspicious/noConsole: 审计日志调试输出
      console.error(`[audit] ${type} ${line.slice(0, 200)}`)
    }

    this.buffer.push(line)

    // buffer 满了立即刷新
    if (this.buffer.length >= this.config.flushBufferSize) {
      await this.flush()
    }
  }

  /** 刷新缓冲区到文件 */
  async flush(): Promise<void> {
    if (this.buffer.length === 0) return
    const lines = this.buffer.splice(0)
    try {
      await appendFile(this.logFilePath, lines.join('\n') + '\n', 'utf8')
    } catch (err) {
      // 静默失败，不阻塞主循环
      if (process.env.LOG_SYSTEM_DEBUG) {
        // biome-ignore lint/suspicious/noConsole: 审计日志错误
        console.error('[audit] flush error:', err)
      }
    }
  }

  // ── Turn 管理 ──────────────────────────────────────────

  /** 开始一个新的 turn */
  async nextTurn(): Promise<void> {
    this.turn++
    await this.event('system.info', {
      message: `Turn ${this.turn} started`,
      turn: this.turn,
    })
  }

  /** 获取当前 turn 序号 */
  get currentTurn(): number {
    return this.turn
  }

  // ── 便捷方法 ──────────────────────────────────────────

  /** 记录用户消息 */
  async userMessage(content: string, isCommand = false): Promise<void> {
    await this.event('user.message', {
      content,
      isCommand,
      messageLength: content.length,
    })
  }

  /** 记录系统上下文注入 */
  async systemContext(
    sections: string[],
    totalChars: number,
  ): Promise<void> {
    await this.event('context.system_prompt', {
      sections,
      totalLength: totalChars,
      hash: this.hashString(JSON.stringify(sections)),
    })
  }

  /** 记录加载的记忆 */
  async memoryLoaded(files: string[], totalChars: number): Promise<void> {
    await this.event('context.memory_loaded', {
      count: files.length,
      files,
      totalChars,
    })
  }

  /** 记录 API 请求 */
  async apiRequest(params: {
    model: string
    messageCount: number
    toolCount: number
    toolNames: string[]
    thinkingEnabled: boolean
    maxOutputTokens: number
    systemPromptChars: number
    totalInputChars: number
    requestId?: string
  }): Promise<void> {
    await this.event('api.request', params)
  }

  /** 记录模型思考 */
  async thinking(content: string, requestId?: string): Promise<void> {
    if (!this.config.captureThinking) return
    await this.event('api.thinking', {
      content,
      charCount: content.length,
      requestId,
    })
  }

  /** 记录模型响应 */
  async apiResponse(
    content: string,
    stopReason: string,
    requestId?: string,
  ): Promise<void> {
    await this.event('api.response', {
      content,
      charCount: content.length,
      stopReason,
      requestId,
    })
  }

  /** 记录 token 用量 */
  async apiUsage(params: {
    inputTokens: number
    outputTokens: number
    cacheReadTokens?: number
    cacheWriteTokens?: number
    costUSD: number
    requestId?: string
  }): Promise<void> {
    await this.event('api.usage', params)
  }

  /** 记录工具调用 */
  async toolInvoke(
    toolName: string,
    input: Record<string, unknown>,
    isDestructive: boolean,
  ): Promise<void> {
    await this.event('tool.invoke', {
      toolName,
      input,
      isDestructive,
      startTime: new Date().toISOString(),
    })
  }

  /** 记录工具结果 */
  async toolResult(
    toolName: string,
    success: boolean,
    output: string,
    durationMs: number,
    error?: string,
  ): Promise<void> {
    const truncated =
      this.config.toolResultMaxChars > 0 &&
      output.length > this.config.toolResultMaxChars
        ? output.slice(0, this.config.toolResultMaxChars) +
          `... [truncated, ${output.length} total chars]`
        : output

    await this.event('tool.result', {
      toolName,
      success,
      outputSummary: truncated,
      outputCharCount: output.length,
      durationMs,
      ...(error && { error }),
    })
  }

  /** 记录记忆保存 */
  async memorySaved(
    files: Array<{ name: string; type: string; description: string }>,
  ): Promise<void> {
    await this.event('memory.save', {
      count: files.length,
      files,
    })
  }

  /** 记录人格快照 */
  async personalitySnapshot(
    traits: Record<string, number>,
    version: number,
  ): Promise<void> {
    await this.event('personality.snapshot', {
      traits,
      version,
      sinceLastChange: new Date().toISOString(),
    })
  }

  /** 记录情感快照 */
  async emotionSnapshot(vad: {
    valence: number
    arousal: number
    dominance: number
  }, labels: string[]): Promise<void> {
    await this.event('emotion.snapshot', { ...vad, labels })
  }

  // ── 生命周期 ──────────────────────────────────────────

  /** 关闭日志系统 */
  async shutdown(): Promise<void> {
    await this.event('session.end', {
      totalTurns: this.turn,
      endedAt: new Date().toISOString(),
    })
    await this.flush()
    if (this.flushTimer) {
      clearInterval(this.flushTimer)
      this.flushTimer = null
    }
    this.initialized = false
  }

  /** 获取日志文件路径 */
  get logPath(): string {
    return this.logFilePath
  }

  /** 获取当前 session ID */
  get currentSessionId(): string {
    return this.sessionId
  }

  // ── 内部工具 ──────────────────────────────────────────

  private hashString(s: string): string {
    // djb2 hash
    let hash = 5381
    for (let i = 0; i < s.length; i++) {
      hash = ((hash << 5) + hash + s.charCodeAt(i)) & 0xffffffff
    }
    return hash.toString(16)
  }
}
