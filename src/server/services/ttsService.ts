/**
 * TTS Service — calls GPT-SoVITS API to synthesize Elio's speech.
 *
 * Auto-discovers voice models from D:\VS_python\TTS\ on startup.
 * Active voice controlled by voice.json in the Elio project root.
 *
 * GPT-SoVITS api_v2.py must be running on port 9880.
 * Start it with:
 *   cd D:\VS_python\TTS\GPT-SoVITS-1007-cu124
 *   runtime\python.exe api_v2.py -a 127.0.0.1 -p 9880 -c GPT_SoVITS/configs/tts_infer.yaml
 *
 * Switching voices:
 *   1. Edit voice.json → change activeVoice
 *   2. Restart Elio Server (updates tts_infer.yaml automatically)
 *   3. Restart GPT-SoVITS API to load new weights
 */

import { join } from 'node:path'
import {
  readdirSync,
  existsSync,
  readFileSync,
  writeFileSync,
  mkdirSync,
} from 'node:fs'
import { homedir } from 'node:os'

// ── Constants ─────────────────────────────────────────────────────────────

const TTS_API = 'http://127.0.0.1:9880/tts'
const AUDIO_DIR = join(homedir(), '.elio', 'audio')
const TTS_ROOT = 'D:/VS_python/TTS'
const ENGINE_ROOT = join(TTS_ROOT, 'GPT-SoVITS-1007-cu124')
const VOICE_CONFIG_PATH = 'D:/VS_python/Elio_Agent/voice.json'
const TTS_INFER_YAML = join(
  ENGINE_ROOT,
  'GPT_SoVITS/configs/tts_infer.yaml',
)

try {
  mkdirSync(AUDIO_DIR, { recursive: true })
} catch {
  /* already exists */
}

// ── Types ─────────────────────────────────────────────────────────────────

interface RefAudio {
  path: string
  text: string
}

interface VoiceProfile {
  name: string // directory name, e.g. "纳西妲_ZH"
  version: string // "v2ProPlus" | "v4"
  lang: string // prompt_lang for API call ("zh" | "ja")
  t2sWeightsRel: string // relative path from engine root for tts_infer.yaml
  vitsWeightsRel: string
  refAudios: Record<string, RefAudio> // emotion key → RefAudio
}

// ── State ─────────────────────────────────────────────────────────────────

let voices: VoiceProfile[] = []
let activeVoice: VoiceProfile | null = null

// ── Emotion labels ───────────────────────────────────────────────────────

/** Map Chinese emotion labels (from 【...】 in filenames) to internal keys. */
const CN_EMOTION_MAP: Record<string, string> = {
  '开心': 'happy',
  '难过': 'sad',
  '吃惊': 'surprise',
  '恐惧': 'fear',
  '厌恶': 'disgust',
  '生气': 'angry',
  '中立': 'neutral',
  '默认': 'default',
}

/** Map personality mode to emotion key. */
const MODE_EMOTION: Record<string, string> = {
  'cute obedient': 'happy',
  'cute rebellious': 'happy',
  'serious obedient': 'happy',
  'serious rebellious': 'disgust',
}

// ── Voice auto-discovery ───────────────────────────────────────────────────

function scanVoices(): void {
  voices = []

  let entries: ReturnType<typeof readdirSync>
  try {
    entries = readdirSync(TTS_ROOT, { withFileTypes: true })
  } catch {
    console.error(`[TTS] Cannot read TTS root: ${TTS_ROOT}`)
    return
  }

  for (const entry of entries) {
    if (!entry.isDirectory()) continue
    const charName = entry.name
    if (charName === 'GPT-SoVITS-1007-cu124') continue

    const charDir = join(TTS_ROOT, charName)

    // Find version directory (the first subdirectory starting with 'v')
    let verEntries: ReturnType<typeof readdirSync>
    try {
      verEntries = readdirSync(charDir, { withFileTypes: true })
    } catch {
      continue
    }
    const verDir = verEntries.find(
      (d) => d.isDirectory() && d.name.startsWith('v'),
    )
    if (!verDir) {
      console.warn(`[TTS] ${charName}: no version directory, skipping`)
      continue
    }
    const version = verDir.name

    // Navigate into: {charDir}/{version}/{charName}/
    const modelDir = join(charDir, version, charName)
    if (!existsSync(modelDir)) {
      console.warn(`[TTS] ${charName}: no model dir at ${modelDir}, skipping`)
      continue
    }

    // Find weight files
    let modelFiles: string[]
    try {
      modelFiles = readdirSync(modelDir)
    } catch {
      continue
    }
    const ckptFile = modelFiles.find((f) => f.endsWith('.ckpt'))
    const pthFile = modelFiles.find((f) => f.endsWith('.pth'))
    if (!ckptFile || !pthFile) {
      console.warn(`[TTS] ${charName}: missing weight files, skipping`)
      continue
    }

    // Relative paths for tts_infer.yaml (engine root as base)
    const t2sWeightsRel = `GPT_weights_${version}/${ckptFile}`
    const vitsWeightsRel = `SoVITS_weights_${version}/${pthFile}`

    // Scan reference audio directory
    const refAudioDir = join(modelDir, 'reference_audios')
    let langDirName = ''
    let lang = 'ja' // default

    if (existsSync(refAudioDir)) {
      let langDirs: string[]
      try {
        langDirs = readdirSync(refAudioDir, { withFileTypes: true })
          .filter((d) => d.isDirectory())
          .map((d) => d.name)
      } catch {
        langDirs = []
      }

      // Pick the first language directory
      langDirName = langDirs[0] || ''
      if (langDirName && langDirName.startsWith('中文')) {
        lang = 'zh'
      }
    }

    // Scan emotion audio files
    const refAudios: Record<string, RefAudio> = {}
    if (langDirName) {
      const emotionsDir = join(refAudioDir, langDirName, 'emotions')
      if (existsSync(emotionsDir)) {
        let wavFiles: string[]
        try {
          wavFiles = readdirSync(emotionsDir)
        } catch {
          wavFiles = []
        }
        for (const wavFile of wavFiles) {
          if (!wavFile.endsWith('.wav')) continue
          // Parse: 【情绪】text.wav
          const match = wavFile.match(/^【(.+?)】(.*)\.wav$/)
          if (match) {
            const cnEmotion = match[1]
            const text = match[2]
            const key = CN_EMOTION_MAP[cnEmotion] || cnEmotion
            refAudios[key] = {
              path: join(emotionsDir, wavFile),
              text,
            }
          }
        }
      }
    }

    if (Object.keys(refAudios).length === 0) {
      console.warn(
        `[TTS] ${charName}: no reference audios found, skipping`,
      )
      continue
    }

    voices.push({
      name: charName,
      version,
      lang,
      t2sWeightsRel,
      vitsWeightsRel,
      refAudios,
    })

    const emotionList = Object.keys(refAudios).join(', ')
    console.log(
      `[TTS] Discovered: ${charName} | ${version} | ${lang} | ${Object.keys(refAudios).length} emotions (${emotionList})`,
    )
  }

  console.log(`[TTS] Found ${voices.length} voice(s) total`)
}

// ── Voice config (voice.json) ──────────────────────────────────────────────

function loadVoiceConfig(): string | null {
  try {
    if (existsSync(VOICE_CONFIG_PATH)) {
      const raw = readFileSync(VOICE_CONFIG_PATH, 'utf-8')
      const config = JSON.parse(raw)
      return typeof config.activeVoice === 'string'
        ? config.activeVoice
        : null
    }
  } catch (e) {
    console.warn('[TTS] Failed to read voice.json:', e)
  }
  return null
}

function activateVoice(name: string | null): void {
  if (name) {
    activeVoice = voices.find((v) => v.name === name) ?? null
    if (!activeVoice) {
      console.warn(
        `[TTS] Voice "${name}" not in discovered voices, falling back`,
      )
    }
  }

  if (!activeVoice) {
    activeVoice = voices[0] ?? null
    if (activeVoice) {
      console.log(`[TTS] Using first available voice: ${activeVoice.name}`)
    }
  }

  if (activeVoice) {
    console.log(
      `[TTS] Active voice: ${activeVoice.name} (${activeVoice.version}, ${activeVoice.lang})`,
    )
    writeTtsInferYaml(activeVoice)
  } else {
    console.error('[TTS] No voices available — TTS will fail')
  }
}

// ── Write tts_infer.yaml ───────────────────────────────────────────────────

function writeTtsInferYaml(voice: VoiceProfile): void {
  let yaml: string
  try {
    yaml = readFileSync(TTS_INFER_YAML, 'utf-8')
  } catch {
    console.error(`[TTS] Cannot read tts_infer.yaml: ${TTS_INFER_YAML}`)
    return
  }

  const lines = yaml.split('\n')

  // Find the `custom:` section boundaries
  let customStart = -1
  let customEnd = lines.length

  for (let i = 0; i < lines.length; i++) {
    const trimmed = lines[i].trim()
    if (customStart < 0 && trimmed === 'custom:') {
      customStart = i
    } else if (
      customStart >= 0 &&
      trimmed.length > 0 &&
      !trimmed.startsWith('#') &&
      /^[a-zA-Z]/.test(lines[i]) // unindented = top-level key
    ) {
      customEnd = i
      break
    }
  }

  if (customStart < 0) {
    console.error('[TTS] custom: section not found in tts_infer.yaml')
    return
  }

  // Replace only within the custom section
  for (let i = customStart + 1; i < customEnd; i++) {
    const trimmed = lines[i].trimStart()
    if (trimmed.startsWith('t2s_weights_path:')) {
      lines[i] = `  t2s_weights_path: "${voice.t2sWeightsRel}"`
    } else if (trimmed.startsWith('version:')) {
      lines[i] = `  version: ${voice.version}`
    } else if (trimmed.startsWith('vits_weights_path:')) {
      lines[i] = `  vits_weights_path: "${voice.vitsWeightsRel}"`
    }
  }

  try {
    writeFileSync(TTS_INFER_YAML, lines.join('\n'), 'utf-8')
  } catch (e) {
    console.error('[TTS] Failed to write tts_infer.yaml:', e)
    return
  }

  console.log(
    `[TTS] Updated tts_infer.yaml → t2s: ${voice.t2sWeightsRel} | vits: ${voice.vitsWeightsRel} | version: ${voice.version}`,
  )
  console.log('[TTS] ⚠  Restart GPT-SoVITS API to load new weights')
}

// ── Reference audio resolution ─────────────────────────────────────────────

function getRefAudio(emotion: string): RefAudio | null {
  if (!activeVoice) return null

  const refs = activeVoice.refAudios

  // Direct match
  if (refs[emotion]) return refs[emotion]

  // Fallback chain: default → neutral → happy → first available
  for (const fallback of ['default', 'neutral', 'happy']) {
    if (refs[fallback]) return refs[fallback]
  }

  // Last resort: first available emotion
  const first = Object.values(refs)[0]
  return first ?? null
}

// ── Public API ─────────────────────────────────────────────────────────────

export interface SubtitleData {
  ja: string
  zh: string
}

export interface TTSResult {
  audioPath: string       // first chunk path (backward compat)
  chunkPaths: string[]    // all chunk paths in order
  subtitle: SubtitleData
}

export interface TTSChunkResult {
  audioPath: string
  chunkIndex: number
}

/** Write a single sentence as a valid WAV file given the shared header template. */
function writeChunkWav(wavHeader: Buffer, sampleRate: number, pcm: Buffer, path: string): void {
  const h = Buffer.from(wavHeader)
  h.writeUInt32LE(36 + pcm.length, 4)   // RIFF chunk size = 36 + PCM bytes
  h.writeUInt32LE(pcm.length, 40)       // data subchunk size
  writeFileSync(path, Buffer.concat([h, pcm]))
}

/**
 * Synthesize speech with streaming — each sentence becomes a standalone WAV
 * as soon as GPT-SoVITS produces it, cutting perceived latency from ~11s to ~2s.
 *
 * `onChunk` fires immediately when a sentence's audio file is ready.
 * Returns the full result once all sentences have been streamed.
 */
export async function synthesize(
  jaText: string,
  zhText: string,
  emotion: string = 'happy',
  onChunk?: (chunk: TTSChunkResult) => void,
): Promise<TTSResult | null> {
  const ref = getRefAudio(emotion)
  if (!ref) {
    console.error('[TTS] No reference audio for emotion:', emotion)
    return null
  }

  const promptLang = activeVoice?.lang ?? 'ja'
  const timestamp = Date.now()
  const baseName = `elio_${timestamp}_${emotion}`

  try {
    const t0 = Date.now()
    const res = await fetch(TTS_API, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        text: jaText,
        text_lang: 'ja',
        ref_audio_path: ref.path,
        prompt_lang: promptLang,
        prompt_text: ref.text,
        media_type: 'wav',
        streaming_mode: true,        // ★ stream each sentence as it's generated
        text_split_method: 'cut5',
        batch_size: 1,               // one sentence per batch → first audio at ~2s
        top_k: 5,                    // reduced from 15, negligible quality impact
        top_p: 0.6,
        temperature: 0.6,
        seed: -1,
        parallel_infer: true,
        repetition_penalty: 1.35,
      }),
    })

    if (!res.ok) {
      const err = await res.json().catch(() => ({ message: res.statusText }))
      console.error(`[TTS] API error: ${err.message || err}`)
      return null
    }

    // ── Read streaming response ───────────────────────────────────────────
    // GPT-SoVITS yields: 44-byte WAV header + one raw-PCM chunk per sentence
    const reader = res.body!.getReader()
    let wavHeader: Buffer | null = null
    let sampleRate = 32000
    let chunkIndex = 0
    const chunkPaths: string[] = []

    while (true) {
      const { done, value } = await reader.read()
      if (done) break

      let buf = Buffer.from(value)

      // First read(s): extract WAV header (44 bytes)
      if (!wavHeader) {
        if (buf.length >= 44) {
          wavHeader = buf.subarray(0, 44)
          sampleRate = wavHeader.readUInt32LE(24)
          // Remaining bytes after header = first sentence PCM
          const pcm = buf.subarray(44)
          if (pcm.length > 0) {
            const path = join(AUDIO_DIR, `${baseName}_${chunkIndex}.wav`)
            writeChunkWav(wavHeader, sampleRate, pcm, path)
            chunkPaths.push(path)
            console.log(`[TTS] Chunk ${chunkIndex}: ${pcm.length}B PCM (${((Date.now() - t0) / 1000).toFixed(1)}s)`)
            onChunk?.({ audioPath: path, chunkIndex })
            chunkIndex++
          }
        }
        continue
      }

      // Subsequent reads: one sentence's PCM per read
      if (buf.length > 0) {
        const path = join(AUDIO_DIR, `${baseName}_${chunkIndex}.wav`)
        writeChunkWav(wavHeader, sampleRate, buf, path)
        chunkPaths.push(path)
        console.log(`[TTS] Chunk ${chunkIndex}: ${buf.length}B PCM (${((Date.now() - t0) / 1000).toFixed(1)}s)`)
        onChunk?.({ audioPath: path, chunkIndex })
        chunkIndex++
      }
    }

    const elapsed = ((Date.now() - t0) / 1000).toFixed(1)
    console.log(`[TTS] Done: ${chunkPaths.length} chunks in ${elapsed}s | subtitle: ${truncate(zhText, 40)}`)

    // Save subtitle JSON
    const subtitlePath = join(AUDIO_DIR, `${baseName}.subtitle.json`)
    writeFileSync(
      subtitlePath,
      JSON.stringify({ ja: jaText, zh: zhText }, null, 2),
      'utf-8',
    )

    return {
      audioPath: chunkPaths[0] ?? '',
      chunkPaths,
      subtitle: { ja: jaText, zh: zhText },
    }
  } catch (e) {
    console.error(`[TTS] Failed: ${e instanceof Error ? e.message : e}`)
    return null
  }
}

/** Map personality mode to emotion key for reference audio selection. */
export function getEmotionForMode(personalityMode: string): string {
  return MODE_EMOTION[personalityMode] ?? 'happy'
}

/** Check if GPT-SoVITS API is reachable. */
export async function isAvailable(): Promise<boolean> {
  try {
    await fetch('http://127.0.0.1:9880/tts', {
      method: 'GET',
      signal: AbortSignal.timeout(3000),
    })
    return true
  } catch {
    return false
  }
}

/** Return all discovered voice profiles (for debugging). */
export function getVoices(): VoiceProfile[] {
  return voices
}

/** Return the currently active voice profile. */
export function getActiveVoice(): VoiceProfile | null {
  return activeVoice
}

// ── Init (runs on module import) ───────────────────────────────────────────

scanVoices()
activateVoice(loadVoiceConfig())

// ── Helpers ────────────────────────────────────────────────────────────────

function truncate(s: string, max: number): string {
  return s.length <= max ? s : s.slice(0, max) + '...'
}
