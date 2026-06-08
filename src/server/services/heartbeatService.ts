/**
 * Heartbeat Service — keeps Elio continuously working.
 *
 * Maintains a single, persistent CLI session. Every 10 seconds checks
 * whether Elio is idle. If she is, sends her a small task. Idle detection
 * is guarded by a safety timeout so a stuck CLI doesn't freeze the loop.
 */

import { conversationService } from './conversationService.js'
import { SettingsService } from './settingsService.js'
import { ProviderService } from './providerService.js'
import { isOpenAIOfficialProviderId } from './openaiOfficialProvider.js'
import { WorldviewBuffer } from '../../elio/WorldviewBuffer.js'

const SESSION_ID = 'elio'
const INTERVAL_MS = 10_000
const WORK_TIMEOUT_MS = 120_000

const settingsService = new SettingsService()
const providerService = new ProviderService()

let intervalId: ReturnType<typeof setInterval> | null = null
let busy = false
let port = 0
let safetyTimer: ReturnType<typeof setTimeout> | null = null
let sessionReady = false
let startTime: number | null = null
let lastElioOutput: string | null = null

export function startHeartbeat(serverPort: number): void {
  if (intervalId) return
  port = serverPort
  startTime = Date.now()
  intervalId = setInterval(tick, INTERVAL_MS)
  console.log('[Heartbeat] Started')
}

export function stopHeartbeat(): void {
  stopTimer()
  killSession()
  busy = false
  sessionReady = false
  startTime = null
  console.log('[Heartbeat] Stopped')
}

// ── Internal ────────────────────────────────────────────────────────────

function buildWorldview(): string {
  // 取出本周期内所有的外部感知事件
  const percepts = WorldviewBuffer.drain()

  const now = new Date()
  const timeStr = now.toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' })
  const hour = now.getHours()
  let timeOfDay: string
  if (hour >= 0 && hour < 6) timeOfDay = '深夜'
  else if (hour >= 6 && hour < 9) timeOfDay = '清晨'
  else if (hour >= 9 && hour < 12) timeOfDay = '上午'
  else if (hour >= 12 && hour < 14) timeOfDay = '午后'
  else if (hour >= 14 && hour < 18) timeOfDay = '下午'
  else if (hour >= 18 && hour < 21) timeOfDay = '傍晚'
  else timeOfDay = '夜晚'

  const elapsedMin = startTime ? Math.floor((Date.now() - startTime) / 60_000) : 0

  const parts = [
    '当前时间: ' + timeStr + '（' + timeOfDay + '）',
    '已持续运行: ' + elapsedMin + ' 分钟',
  ]

  // 外部感知事件
  if (percepts.length > 0) {
    parts.push('')
    parts.push('--- 本周期内的外部事件 ---')
    parts.push(WorldviewBuffer.formatForWorldview(percepts))
  } else {
    parts.push('本周期内无外部事件。')
  }

  // Elio 上轮行为
  if (lastElioOutput) {
    parts.push('')
    parts.push('你上轮的行为:')
    parts.push(lastElioOutput)
    lastElioOutput = null
  }

  // 包裹在 <worldview> 标签中
  return '<worldview>\n' + parts.join('\n') + '\n</worldview>'
}

async function tick(): Promise<void> {
  if (busy) return
  busy = true

  try {
    if (!conversationService.hasSession(SESSION_ID)) {
      await startSession()
      conversationService.onOutput(SESSION_ID, onOutput)
      sessionReady = true
    }

    const worldview = buildWorldview()
    const sent = conversationService.sendWorldview(SESSION_ID, worldview)
    if (!sent) {
      busy = false
      return
    }
    console.log('[Heartbeat] Worldview sent')

    safetyTimer = setTimeout(() => {
      console.warn('[Heartbeat] Task timed out — resetting')
      busy = false
      safetyTimer = null
    }, WORK_TIMEOUT_MS)
  } catch (e) {
    busy = false
    console.error('[Heartbeat] Error:', e instanceof Error ? e.message : e)
  }
}

async function startSession(): Promise<void> {
  const token = crypto.randomUUID()
  const sdkUrl =
    `ws://127.0.0.1:${port}/sdk/${SESSION_ID}` +
    `?token=${encodeURIComponent(token)}`

  const runtime = await getRuntimeSettings()

  await conversationService.startSession(SESSION_ID, process.cwd(), sdkUrl, {
    permissionMode: 'bypassPermissions',
    model: runtime.model,
    providerId: runtime.providerId,
    effort: runtime.effort,
    thinking: runtime.thinking,
  })
}

// ── Runtime settings lookup (mirrors handler.ts getDefaultRuntimeSettings) ──

async function getRuntimeSettings(): Promise<{
  model?: string
  providerId?: string | null
  effort?: string
  thinking?: 'disabled'
}> {
  const { providers, activeId } = await providerService.listProviders()
  let resolvedActiveId: string | null = activeId
  if (activeId && !isKnownRuntimeProviderId(activeId, providers)) {
    console.warn(`[Heartbeat] Active provider stale, falling back to official: ${activeId}`)
    resolvedActiveId = null
    await providerService.activateOfficial()
  }

  const userSettings = await settingsService.getUserSettings()
  const providerSettings = resolvedActiveId
    ? await providerService.getManagedSettings()
    : undefined
  const modelSettings = providerSettings ?? userSettings
  const modelContext =
    typeof modelSettings.modelContext === 'string' && modelSettings.modelContext.trim()
      ? modelSettings.modelContext
      : undefined
  const effort =
    typeof userSettings.effort === 'string' && userSettings.effort.trim()
      ? userSettings.effort
      : undefined
  const thinking: 'disabled' | undefined =
    userSettings.alwaysThinkingEnabled === false ? 'disabled' : undefined

  let model: string | undefined
  if (resolvedActiveId) {
    const baseModel =
      typeof modelSettings.model === 'string' && modelSettings.model.trim()
        ? modelSettings.model
        : ''
    if (baseModel) {
      model = baseModel
      if (modelContext) model += `:${modelContext}`
    }
  } else {
    const baseModel =
      typeof userSettings.model === 'string' && userSettings.model.trim()
        ? userSettings.model
        : undefined
    model = baseModel ? (modelContext ? `${baseModel}:${modelContext}` : baseModel) : undefined
  }

  console.log(`[Heartbeat] Runtime: provider=${resolvedActiveId ?? 'official'}, model=${model ?? 'default'}`)

  return {
    model,
    providerId: resolvedActiveId,
    effort,
    thinking,
  }
}

function isKnownRuntimeProviderId(
  id: string,
  providers: Array<{ id: string }>,
): boolean {
  return (
    isOpenAIOfficialProviderId(id) ||
    providers.some((p) => p.id === id)
  )
}

function onOutput(msg: any): void {
  const content = extractContent(msg)

  if (msg?.type === 'result') {
    busy = false
    if (safetyTimer) {
      clearTimeout(safetyTimer)
      safetyTimer = null
    }
    console.log(`[Heartbeat] result: is_error=${msg.is_error}, tokens=${msg.usage?.input_tokens ?? 0}+${msg.usage?.output_tokens ?? 0}`)
  } else if (msg?.type === 'assistant') {
    if (content) {
      lastElioOutput = content
      console.log(`[Heartbeat] Elio: ${truncate(content)}`)
    }
  } else if (msg?.type === 'stream_event') {
    // skip — partial chunks; final text logged by assistant event
  } else if (msg?.type === 'user') {
    // skip — worldview echo from CLI SDK, not real user messages
  } else {
    const subtype = msg?.subtype || '-'
    const c = content ? ` — ${truncate(content)}` : ''
    console.log(`[Heartbeat] msg: type=${msg?.type}, subtype=${subtype}${c}`)
  }
}

function extractContent(msg: any): string | null {
  if (msg?.event?.content_block?.text) return msg.event.content_block.text
  if (msg?.message?.content) {
    const blocks = Array.isArray(msg.message.content) ? msg.message.content : [msg.message.content]
    return blocks.map((b: any) => {
      if (typeof b === 'string') return b
      if (b?.text) return b.text
      if (b?.type === 'tool_use') return `[调用工具: ${b.name}]`
      return null
    }).filter(Boolean).join('')
  }
  if (typeof msg?.result === 'string') return msg.result
  return null
}

function truncate(s: string, max = 200): string {
  return s.length <= max ? s : s.slice(0, max) + '...'
}

function stopTimer(): void {
  if (intervalId) {
    clearInterval(intervalId)
    intervalId = null
  }
  if (safetyTimer) {
    clearTimeout(safetyTimer)
    safetyTimer = null
  }
}

function killSession(): void {
  if (conversationService.hasSession(SESSION_ID)) {
    conversationService.stopSession(SESSION_ID)
  }
}
