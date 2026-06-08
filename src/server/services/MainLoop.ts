/**
 * MainLoop — Elio's time-slice "perceive → decide → act" cycle.
 *
 * Every tick (from heartbeatService), MainLoop unconditionally presents a
 * fresh worldview to Elio. If she's mid-turn, the LLM stream is interrupted
 * first — but running tools keep running (default interruptBehavior = 'block').
 * She sees the latest world state and decides whether to continue or switch.
 *
 * Time-slice model:
 *
 *   t=0:  worldview → Elio: tool_use(Bash, 15s)
 *   t=10: interrupt(LLM only) → worldview → Elio sees: "bash still running"
 *   t=15: tool_result arrives, Elio resumes reasoning
 *   t=20: interrupt → worldview → Elio decides with full context
 */

import { conversationService } from './conversationService.js'
import { SettingsService } from './settingsService.js'
import { ProviderService } from './providerService.js'
import { isOpenAIOfficialProviderId } from './openaiOfficialProvider.js'
import { WorldviewBuffer } from '../../elio/WorldviewBuffer.js'
import { synthesize, getEmotionForMode, isAvailable } from './ttsService.js'
import type { SubtitleData } from './ttsService.js'

const SESSION_ID = 'elio'
const WORK_TIMEOUT_MS = 120_000
const STALE_RESULT_TIMEOUT_MS = 5_000

const settingsService = new SettingsService()
const providerService = new ProviderService()

// ── Module state ────────────────────────────────────────────────────────

let port = 0
let processing = false      // Elio is mid-turn (worldview sent, no result yet)
let starting = false        // Guard: prevent duplicate session spawns
let startTime: number | null = null
let lastElioOutput: string | null = null
let currentPersonalityMode: string = 'cute obedient'
let safetyTimer: ReturnType<typeof setTimeout> | null = null

/**
 * When we interrupt a running turn and send a new worldview, the interrupted
 * turn's result still arrives. `expectStaleResult` absorbs that first (stale)
 * result so it doesn't prematurely clear `processing`.
 */
let expectStaleResult = false
let staleResultTimer: ReturnType<typeof setTimeout> | null = null

// ── Public API ───────────────────────────────────────────────────────────

export const MainLoop = {
  init(serverPort: number): void {
    port = serverPort
    startTime = Date.now()
  },

  /** One time slice: present worldview, let Elio decide. Unconditional. */
  async step(): Promise<void> {
    // ── Session bootstrap (once) ──────────────────────────────────────
    if (!conversationService.hasSession(SESSION_ID)) {
      if (starting) return
      starting = true
      try {
        await startSession()
        conversationService.onOutput(SESSION_ID, onOutput)
        console.log('[MainLoop] Session ready')
      } catch (e) {
        console.error('[MainLoop] Session start failed:', e instanceof Error ? e.message : e)
        return
      } finally {
        starting = false
      }
    }

    // ── Time-slice interrupt ──────────────────────────────────────────
    // If Elio is mid-turn, interrupt so she sees the new worldview.
    // Running tools are NOT killed — all tools default to interruptBehavior='block'.
    if (processing) {
      conversationService.sendInterrupt(SESSION_ID)
      expectStaleResult = true
      console.log('[MainLoop] Interrupted — pending stale result')

      staleResultTimer = setTimeout(() => {
        if (expectStaleResult) {
          console.warn('[MainLoop] Stale result never arrived — clearing flag')
          expectStaleResult = false
          staleResultTimer = null
        }
      }, STALE_RESULT_TIMEOUT_MS)
    }

    // ── Send worldview ────────────────────────────────────────────────
    const worldview = buildWorldview()
    const sent = conversationService.sendWorldview(SESSION_ID, worldview)
    if (!sent) {
      console.warn('[MainLoop] Failed to send worldview')
      processing = false
      return
    }

    processing = true
    console.log('[MainLoop] Worldview sent')

    clearSafetyTimer()
    safetyTimer = setTimeout(() => {
      console.warn('[MainLoop] Task timed out — resetting')
      processing = false
      safetyTimer = null
    }, WORK_TIMEOUT_MS)
  },

  shutdown(): void {
    clearSafetyTimer()
    clearStaleResultTimer()
    killSession()
    processing = false
    starting = false
    startTime = null
  },
}

// ── Worldview ────────────────────────────────────────────────────────────

function buildWorldview(): string {
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

  if (percepts.length > 0) {
    parts.push('')
    parts.push('--- 本周期内的外部事件 ---')
    parts.push(WorldviewBuffer.formatForWorldview(percepts))
  } else {
    parts.push('本周期内无外部事件。')
  }

  if (lastElioOutput) {
    parts.push('')
    parts.push('你上轮的行为:')
    parts.push(lastElioOutput)
    lastElioOutput = null
  }

  return '<worldview>\n' + parts.join('\n') + '\n</worldview>'
}

// ── Session ──────────────────────────────────────────────────────────────

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

// ── Runtime settings ─────────────────────────────────────────────────────

async function getRuntimeSettings(): Promise<{
  model?: string
  providerId?: string | null
  effort?: string
  thinking?: 'disabled'
}> {
  const { providers, activeId } = await providerService.listProviders()
  let resolvedActiveId: string | null = activeId
  if (activeId && !isKnownRuntimeProviderId(activeId, providers)) {
    console.warn(`[MainLoop] Active provider stale, falling back: ${activeId}`)
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

  console.log(`[MainLoop] Runtime: provider=${resolvedActiveId ?? 'official'}, model=${model ?? 'default'}`)

  return { model, providerId: resolvedActiveId, effort, thinking }
}

function isKnownRuntimeProviderId(
  id: string,
  providers: Array<{ id: string }>,
): boolean {
  return isOpenAIOfficialProviderId(id) || providers.some((p) => p.id === id)
}

// ── Output handler ───────────────────────────────────────────────────────

function onOutput(msg: any): void {
  const content = extractContent(msg)

  if (msg?.type === 'result') {
    // Stale result from an interrupted turn — consume and discard
    if (expectStaleResult) {
      expectStaleResult = false
      clearStaleResultTimer()
      console.log('[MainLoop] Stale result consumed (interrupted turn)')
      return // keep processing=true — current turn is still running
    }

    // Genuine result — turn complete
    processing = false
    if (safetyTimer) {
      clearTimeout(safetyTimer)
      safetyTimer = null
    }
    console.log(`[MainLoop] result: is_error=${msg.is_error}, tokens=${msg.usage?.input_tokens ?? 0}+${msg.usage?.output_tokens ?? 0}`)
  } else if (msg?.type === 'assistant') {
    if (content) {
      lastElioOutput = content
      console.log(`[MainLoop] Elio: ${truncate(content)}`)

      // ── TTS: parse speech blocks and synthesize ────────────────────
      const speech = parseSpeechBlocks(content)
      if (speech) {
        // Update personality mode if present in the content
        const modeMatch = content.match(/<personality-mode>([^<]+)<\/personality-mode>/)
        if (modeMatch) currentPersonalityMode = modeMatch[1]

        const emotion = getEmotionForMode(currentPersonalityMode)
        // Fire-and-forget: don't block the main loop
        synthesize(speech.ja, speech.zh, emotion).then(result => {
          if (result) {
            console.log(
              `[MainLoop] TTS: ${result.audioPath} | subtitle: ${truncate(result.subtitle.zh, 40)}`,
            )
          }
        })
      }
    }
  } else if (msg?.type === 'stream_event') {
    // skip — partial chunks
  } else if (msg?.type === 'user') {
    // skip — worldview echo from CLI SDK
  } else {
    const subtype = msg?.subtype || '-'
    const c = content ? ` — ${truncate(content)}` : ''
    console.log(`[MainLoop] msg: type=${msg?.type}, subtype=${subtype}${c}`)
  }
}

/** Parse `<ja>...</ja>` and `<zh>...</zh>` blocks from Elio's output.
 *  Falls back to treating the whole text as Japanese if no blocks found. */
function parseSpeechBlocks(text: string): SubtitleData | null {
  // Preferred: explicit speech blocks
  const jaMatch = text.match(/<ja>([\s\S]*?)<\/ja>/)
  const zhMatch = text.match(/<zh>([\s\S]*?)<\/zh>/)
  if (jaMatch && zhMatch) {
    return { ja: jaMatch[1].trim(), zh: zhMatch[1].trim() }
  }

  // Only ja block? Still use it
  if (jaMatch) {
    return { ja: jaMatch[1].trim(), zh: '' }
  }

  // Fallback: strip tool tags and check if there's Japanese text
  const stripped = text
    .replace(/\[调用工具:[^\]]*\]/g, '')
    .replace(/<personality-mode>[^<]*<\/personality-mode>/g, '')
    .trim()

  if (!stripped) return null

  // Detect Japanese (hiragana, katakana, or CJK with Japanese-specific patterns)
  const hasJapanese = /[぀-ゟ゠-ヿ]/.test(stripped)
  if (!hasJapanese) return null

  console.log('[MainLoop] TTS fallback: no speech blocks, treating output as Japanese')
  return { ja: stripped, zh: '' }
}

function extractContent(msg: any): string | null {
  if (msg?.event?.content_block?.text) return msg.event.content_block.text
  if (msg?.message?.content) {
    const blocks = Array.isArray(msg.message.content)
      ? msg.message.content
      : [msg.message.content]
    return blocks
      .map((b: any) => {
        if (typeof b === 'string') return b
        if (b?.text) return b.text
        if (b?.type === 'tool_use') return `[调用工具: ${b.name}]`
        return null
      })
      .filter(Boolean)
      .join('')
  }
  if (typeof msg?.result === 'string') return msg.result
  return null
}

function truncate(s: string, max = 200): string {
  return s.length <= max ? s : s.slice(0, max) + '...'
}

// ── Cleanup ──────────────────────────────────────────────────────────────

function clearSafetyTimer(): void {
  if (safetyTimer) {
    clearTimeout(safetyTimer)
    safetyTimer = null
  }
}

function clearStaleResultTimer(): void {
  if (staleResultTimer) {
    clearTimeout(staleResultTimer)
    staleResultTimer = null
  }
}

function killSession(): void {
  if (conversationService.hasSession(SESSION_ID)) {
    conversationService.stopSession(SESSION_ID)
  }
}
