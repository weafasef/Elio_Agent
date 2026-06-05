/**
 * Heartbeat Service — keeps Elio continuously working.
 *
 * Maintains a single, persistent CLI session. Every 10 seconds checks
 * whether Elio is idle. If she is, sends her a small task. Idle detection
 * is guarded by a safety timeout so a stuck CLI doesn't freeze the loop.
 */

import { conversationService } from './conversationService.js'
import * as os from 'node:os'
import { SettingsService } from './settingsService.js'
import { ProviderService } from './providerService.js'
import { isOpenAIOfficialProviderId } from './openaiOfficialProvider.js'

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

export function startHeartbeat(serverPort: number): void {
  if (intervalId) return
  port = serverPort
  intervalId = setInterval(tick, INTERVAL_MS)
  console.log('[Heartbeat] Started')
}

export function stopHeartbeat(): void {
  stopTimer()
  killSession()
  busy = false
  sessionReady = false
  console.log('[Heartbeat] Stopped')
}

// ── Internal ────────────────────────────────────────────────────────────

async function tick(): Promise<void> {
  if (busy) return
  busy = true

  try {
    if (!conversationService.hasSession(SESSION_ID)) {
      await startSession()
      conversationService.onOutput(SESSION_ID, onOutput)
      sessionReady = true
    }

    const sent = await conversationService.sendMessage(
      SESSION_ID,
      'Elio，在空闲时间里，随便写点东西到 D:\\VS_python\\Elio_Agent\\work.md，什么都行，不用太长。',
    )
    if (!sent) {
      busy = false
      return
    }
    console.log('[Heartbeat] Task submitted')

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

  await conversationService.startSession(SESSION_ID, os.homedir(), sdkUrl, {
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

let streamEventLogged = false

function onOutput(msg: any): void {
  if (msg?.type === 'result') {
    busy = false
    streamEventLogged = false
    if (safetyTimer) {
      clearTimeout(safetyTimer)
      safetyTimer = null
    }
    console.log(`[Heartbeat] result: is_error=${msg.is_error}`)
  } else if (msg?.type === 'stream_event') {
    if (!streamEventLogged) {
      streamEventLogged = true
      console.log('[Heartbeat] msg: type=stream_event (suppressing further)')
    }
  } else {
    console.log(`[Heartbeat] msg: type=${msg?.type}, subtype=${msg?.subtype || '-'}`)
  }
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
