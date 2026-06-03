/**
 * log-system 集成 — 将审计日志挂接到 Agent 生命周期
 *
 * 在入口文件 (cli.tsx 或 main.tsx) 中调用 initLogSystem()
 * 即可自动捕获所有 Agent 操作。
 *
 * 注意：api.request / api.thinking / api.response / api.usage
 * 已在 src/services/api/claude.ts 中直接埋点（精确时间戳）。
 * 本 hook 只负责捕获 API 层无法感知的上下文事件。
 */

import { registerPostSamplingHook } from '../utils/hooks/postSamplingHooks.js'
import { getAuditLogger, initAuditLogger } from './index.js'
import type { AuditLogConfig } from './types.js'

// ── 入口 ──────────────────────────────────────────────

/**
 * 初始化日志系统并注册所有 Hook。
 * 在 session 启动时调用一次即可。
 */
export async function initLogSystem(
  config?: Partial<AuditLogConfig>,
): Promise<void> {
  const audit = initAuditLogger(config)
  await audit.init()
  registerHooks()
}

// ── Hook 注册 ─────────────────────────────────────────

function registerHooks(): void {
  // Post-sampling hook: 捕获上下文、用户消息和工具调用
  // api.request / api.thinking / api.response / api.usage 在 claude.ts 中直接埋点
  registerPostSamplingHook(async (context) => {
    const audit = getAuditLogger()
    await audit.nextTurn()

    const { messages, systemPrompt, userContext, systemContext } = context

    // ── 系统上下文（每个 session 只记录一次） ──────────
    if (audit.currentTurn <= 2) {
      const sysSections = Array.isArray(systemPrompt)
        ? systemPrompt
        : [String(systemPrompt)]
      const sysTotal = sysSections.reduce((acc, s) => acc + s.length, 0)
      await audit.systemContext(sysSections, sysTotal)

      await audit.event('context.environment', {
        userContext,
        systemContext,
      })
    }

    // ── 捕获用户消息和工具调用 ──────────────────────────
    const latestMsgs = messages.slice(-10)

    for (const msg of latestMsgs) {
      if (msg.type === 'assistant') {
        const content = msg.message.content

        if (Array.isArray(content)) {
          for (const block of content) {
            if (block.type === 'tool_use') {
              // 工具调用（api 层无法感知）
              void audit.toolInvoke(
                block.name,
                (block.input || {}) as Record<string, unknown>,
                false,
              )
            }
          }
        }
      } else if (msg.type === 'user') {
        // 用户消息
        const content = msg.message.content
        if (typeof content === 'string') {
          void audit.userMessage(content)
        } else if (Array.isArray(content)) {
          const textBlocks = content
            .filter((b) => b.type === 'text')
            .map((b) => ('text' in b ? b.text : ''))
            .join('\n')
          if (textBlocks) {
            void audit.userMessage(textBlocks)
          }
        }
      }
    }
  })
}
