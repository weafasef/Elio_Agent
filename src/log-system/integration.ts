/**
 * log-system 集成 — 将审计日志挂接到 Agent 生命周期
 *
 * 在入口文件 (cli.tsx 或 main.tsx) 中调用 initLogSystem()
 * 即可自动捕获所有 Agent 操作。
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
  // Post-sampling hook: 在每个 model turn 之后触发
  // 这是核心钩子，能捕获：
  //   1. assistant 文本响应 → api.response
  //   2. thinking 内容 → api.thinking
  //   3. tool_use 调用 → tool.invoke
  //   4. token 用量 → api.usage
  //   5. 系统上下文信息
  registerPostSamplingHook(async (context) => {
    const audit = getAuditLogger()
    await audit.nextTurn()

    const { messages, systemPrompt, userContext, systemContext } = context

    // ── 系统上下文（每个 session 只记录第一次） ──────────
    if (audit.currentTurn <= 2) {
      // 记录系统提示词概要
      const sysSections = Array.isArray(systemPrompt)
        ? systemPrompt
        : [String(systemPrompt)]
      const sysTotal = sysSections.reduce((acc, s) => acc + s.length, 0)
      await audit.systemContext(sysSections, sysTotal)

      // 记录环境
      await audit.event('context.environment', {
        userContext,
        systemContext,
      })
    }

    // ── 分析本轮消息 ──────────────────────────────────
    const latestMsgs = messages.slice(-10) // 只看最近 10 条

    for (const msg of latestMsgs) {
      if (msg.type === 'assistant') {
        const content = msg.message.content

        // 处理数组内容 (正常情况)
        if (Array.isArray(content)) {
          for (const block of content) {
            switch (block.type) {
              case 'text':
                // 记录文本响应
                await audit.apiResponse(
                  block.text,
                  msg.message.stop_reason || 'unknown',
                  msg.requestId,
                )
                break

              case 'thinking':
                // 记录思考过程
                if ('thinking' in block && block.thinking) {
                  await audit.thinking(
                    block.thinking,
                    msg.requestId,
                  )
                }
                break

              case 'tool_use':
                // 记录工具调用
                await audit.toolInvoke(
                  block.name,
                  (block.input || {}) as Record<string, unknown>,
                  false, // isDestructive 信息需要从 tool registry 获取
                )
                break
            }
          }

          // 记录 token 用量
          if (msg.message.usage) {
            await audit.apiUsage({
              inputTokens: msg.message.usage.input_tokens || 0,
              outputTokens: msg.message.usage.output_tokens || 0,
              cacheReadTokens: msg.message.usage.cache_read_input_tokens,
              cacheWriteTokens: msg.message.usage.cache_creation_input_tokens,
              costUSD: 0,
              requestId: msg.requestId,
            })
          }
        } else if (typeof content === 'string') {
          // 纯文本响应
          await audit.apiResponse(
            content,
            msg.message.stop_reason || 'unknown',
            msg.requestId,
          )
        }
      } else if (msg.type === 'user') {
        // 用户消息 (跳过第一条 system 消息)
        const content = msg.message.content
        if (typeof content === 'string') {
          await audit.userMessage(content)
        } else if (Array.isArray(content)) {
          const textBlocks = content
            .filter((b) => b.type === 'text')
            .map((b) => ('text' in b ? b.text : ''))
            .join('\n')
          if (textBlocks) {
            await audit.userMessage(textBlocks)
          }
        }
      }
    }

    // ── API 请求摘要 ──────────────────────────────────
    // 从 toolUseContext 中获取工具信息
    try {
      const tuc = context.toolUseContext
      const tools = tuc.options?.tools || []
      await audit.apiRequest({
        model: tuc.options?.model || 'unknown',
        messageCount: messages.length,
        toolCount: tools.length,
        toolNames: tools.map((t) => t.name).slice(0, 50),
        thinkingEnabled: true,
        maxOutputTokens: 0,
        systemPromptChars: Array.isArray(systemPrompt)
          ? systemPrompt.reduce((a, s) => a + s.length, 0)
          : String(systemPrompt).length,
        totalInputChars: messages.reduce(
          (a, m) => a + JSON.stringify(m).length,
          0,
        ),
      })
    } catch {
      // toolUseContext 可能在 hook 中不可用
    }
  })
}
