/**
 * TTS Service — calls GPT-SoVITS API to synthesize Elio's speech.
 *
 * GPT-SoVITS api_v2.py must be running on port 9880.
 * Start it with:
 *   cd D:\VS_python\TTS\GPT-SoVITS-1007-cu124
 *   runtime\python.exe api_v2.py -a 127.0.0.1 -p 9880 -c GPT_SoVITS/configs/tts_infer.yaml
 */

import { join } from 'node:path'
import { mkdirSync, writeFileSync, readdirSync, existsSync } from 'node:fs'
import { homedir } from 'node:os'

const TTS_API = 'http://127.0.0.1:9880/tts'
const AUDIO_DIR = join(homedir(), '.elio', 'audio')

// Ensure audio directory exists
try { mkdirSync(AUDIO_DIR, { recursive: true }) } catch { /* already exists */ }

// ── Emotion → reference audio (auto-scanned from disk) ──────────────────

interface RefAudio {
  path: string
  text: string // what the reference audio says (transcribed manually)
}

const REF_BASE =
  'D:/VS_python/TTS/花火/v2ProPlus/花火/reference_audios/日语/emotions'

// Emotion keywords used to match files by filename
const EMOTION_KEYWORDS: Record<string, string> = {
  happy: '开心',
  sad: '难过',
  surprise: '吃惊',
  fear: '恐惧',
  disgust: '厌恶',
}

// Manual transcriptions (can't extract these from filename)
const EMOTION_TEXTS: Record<string, string> = {
  happy: 'このプレゼントを受け取って、ゲストちゃん。これはファミリーが君のために特別に用意したものなんだよ〜',
  sad: 'メモキーパーちゃん…無事にここから出られると思う？',
  surprise: 'はいは〜い、もう行くってば〜…でもロビンちゃん、もう一度よく考えたほうがいいんじゃない？',
  fear: '花火、寝ちゃってたの？',
  disgust: '待って待って、どこ行くの？行かないでよ〜！',
}

const REF_AUDIOS: Record<string, RefAudio> = {}

/** Scan the reference audio directory and build the emotion → file mapping. */
function scanRefAudios(): void {
  if (!existsSync(REF_BASE)) {
    console.warn(`[TTS] Reference audio directory not found: ${REF_BASE}`)
    return
  }

  const files = readdirSync(REF_BASE)
  console.log(`[TTS] Scanning reference audios: ${files.length} files in ${REF_BASE}`)

  for (const [emotion, keyword] of Object.entries(EMOTION_KEYWORDS)) {
    const match = files.find(f => f.includes(keyword) && f.endsWith('.wav'))
    if (match) {
      REF_AUDIOS[emotion] = {
        path: join(REF_BASE, match),
        text: EMOTION_TEXTS[emotion] || '',
      }
      console.log(`[TTS]   ${emotion}: ${match}`)
    } else {
      console.warn(`[TTS]   ${emotion}: no match for "${keyword}"`)
    }
  }
}

// Scan immediately on module load
scanRefAudios()

// ── Personality mode → emotion mapping ─────────────────────────────────

const MODE_EMOTION: Record<string, string> = {
  'cute obedient': 'happy',
  'cute rebellious': 'happy',
  'serious obedient': 'happy',
  'serious rebellious': 'disgust',
}

// ── Public API ────────────────────────────────────────────────────────

export interface SubtitleData {
  ja: string
  zh: string
}

export interface TTSResult {
  audioPath: string
  subtitle: SubtitleData
}

/**
 * Synthesize speech from Japanese text, save audio + subtitle to disk.
 * Returns paths or null on failure.
 */
export async function synthesize(
  jaText: string,
  zhText: string,
  emotion: string = 'happy',
): Promise<TTSResult | null> {
  const ref = REF_AUDIOS[emotion] ?? REF_AUDIOS.happy
  if (!ref) {
    console.error('[TTS] No reference audio available for emotion:', emotion)
    return null
  }

  try {
    const res = await fetch(TTS_API, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        text: jaText,
        text_lang: 'ja',
        ref_audio_path: ref.path,
        prompt_lang: 'ja',
        prompt_text: ref.text,
        media_type: 'wav',
        streaming_mode: false,
      }),
    })

    if (!res.ok) {
      const err = await res.json().catch(() => ({ message: res.statusText }))
      console.error(`[TTS] API error: ${err.message || err}`)
      return null
    }

    const buffer = Buffer.from(await res.arrayBuffer())
    const timestamp = Date.now()
    const baseName = `elio_${timestamp}_${emotion}`
    const audioPath = join(AUDIO_DIR, `${baseName}.wav`)
    const subtitlePath = join(AUDIO_DIR, `${baseName}.subtitle.json`)

    writeFileSync(audioPath, buffer)
    writeFileSync(
      subtitlePath,
      JSON.stringify({ ja: jaText, zh: zhText }, null, 2),
      'utf-8',
    )

    console.log(
      `[TTS] Saved: ${audioPath} (${buffer.length} bytes) | subtitle: ${truncate(zhText, 40)}`,
    )
    return { audioPath, subtitle: { ja: jaText, zh: zhText } }
  } catch (e) {
    console.error(`[TTS] Failed: ${e instanceof Error ? e.message : e}`)
    return null
  }
}

/** Map personality mode to emotion for reference audio selection. */
export function getEmotionForMode(personalityMode: string): string {
  return MODE_EMOTION[personalityMode] ?? 'happy'
}

/** Check if TTS API is reachable. */
export async function isAvailable(): Promise<boolean> {
  try {
    const res = await fetch('http://127.0.0.1:9880/tts', {
      method: 'GET',
      signal: AbortSignal.timeout(3000),
    })
    return true
  } catch {
    return false
  }
}

// ── Helpers ───────────────────────────────────────────────────────────

function truncate(s: string, max: number): string {
  return s.length <= max ? s : s.slice(0, max) + '...'
}
