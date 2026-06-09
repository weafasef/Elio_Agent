/**
 * Elio 终端客户端
 * 用法: bun client.ts
 *
 * 连接到 Elio Server WebSocket，收发消息，自动播放 TTS 语音。
 */

import { createInterface } from 'node:readline'
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { homedir } from 'node:os'
import { spawn } from 'node:child_process'
import { Buffer } from 'node:buffer'

// ── Config ──────────────────────────────────────────────────────────────

const HOST = '127.0.0.1:3456'
const WS_URL = `ws://${HOST}/ws/elio`
const HTTP_BASE = `http://${HOST}`

// ── Colors ──────────────────────────────────────────────────────────────

const C = {
  reset: '\x1b[0m',
  dim: '\x1b[2m',
  cyan: '\x1b[36m',
  yellow: '\x1b[33m',
  magenta: '\x1b[35m',
  red: '\x1b[31m',
  green: '\x1b[32m',
}

// ── Audio playback ──────────────────────────────────────────────────────

let audioQueue: Array<{ url: string; zh: string }> = []
let audioPlaying = false

function playAudioFile(url: string): void {
  const fullUrl = HTTP_BASE + url
  if (process.platform === 'win32') {
    // Fetch and save to temp, then play
    fetch(fullUrl).then(r => r.arrayBuffer()).then(buf => {
      const tmp = join(homedir(), '.elio', 'audio', '_playback.wav')
      writeFileSync(tmp, Buffer.from(buf))
      spawn('powershell', [
        '-c',
        `(New-Object Media.SoundPlayer '${tmp}').PlaySync()`,
      ], { stdio: 'ignore' }).on('exit', () => playNextInQueue())
    }).catch(() => playNextInQueue())
  } else {
    spawn('ffplay', ['-nodisp', '-autoexit', fullUrl], { stdio: 'ignore' })
      .on('exit', () => playNextInQueue())
  }
}

function playNextInQueue(): void {
  audioPlaying = false
  if (audioQueue.length === 0) return
  const next = audioQueue.shift()!
  audioPlaying = true
  playAudioFile(next.url)
}

// ── Parse speech blocks ─────────────────────────────────────────────────

interface ParsedSpeech {
  thinks: string[]   // <think> contents — internal thoughts (NOT spoken)
  ja: string         // <ja> blocks joined — TTS source
  zh: string         // <zh> blocks joined — main display text
}

function parseSpeech(text: string): ParsedSpeech | null {
  // Extract all blocks (support multiple of each, interleaved)
  const thinkBlocks = [...text.matchAll(/<think>([\s\S]*?)<\/think>/g)].map(m => m[1].trim())
  const jaBlocks = [...text.matchAll(/<ja>([\s\S]*?)<\/ja>/g)].map(m => m[1].trim())
  const zhBlocks = [...text.matchAll(/<zh>([\s\S]*?)<\/zh>/g)].map(m => m[1].trim())

  const ja = jaBlocks.join('')
  const zh = zhBlocks.join('')

  if (ja || thinkBlocks.length > 0) {
    return { thinks: thinkBlocks, ja, zh }
  }

  // Fallback: bare Japanese text without tags
  const stripped = text
    .replace(/\[调用工具:[^\]]*\]/g, '')
    .replace(/<personality-mode>[^<]*<\/personality-mode>/g, '')
    .trim()
  if (/[぀-ゟ゠-ヿ]/.test(stripped)) return { thinks: [], ja: stripped, zh: '' }
  return null
}

// ── Connect ──────────────────────────────────────────────────────────────

let ws: WebSocket | null = null
let elioBuffer = ''
let connected = false

function connect(): void {
  ws = new WebSocket(WS_URL)

  ws.onopen = () => {
    connected = true
    console.log(C.green + '✓ 已连接到 Elio' + C.reset)
    promptLine()
  }

  ws.onclose = () => {
    connected = false
    console.log(C.red + '\n✗ 断开连接，3秒后重连...' + C.reset)
    setTimeout(connect, 3000)
  }

  ws.onerror = () => {}

  ws.onmessage = (event: MessageEvent) => {
    let msg: any
    try { msg = JSON.parse(event.data) } catch { return }

    switch (msg.type) {
      case 'connected':
        break

      case 'content_start':
        if (msg.blockType === 'text') elioBuffer = ''
        break

      case 'content_delta':
        if (msg.text) elioBuffer += msg.text
        break

      case 'tool_use_complete':
        process.stdout.write(C.dim + `\n  🔧 ${msg.toolName}...` + C.reset)
        break

      case 'message_complete':
        if (elioBuffer) {
          const speech = parseSpeech(elioBuffer)
          if (speech) {
            // Display thinks first (dim, with thought bubble)
            for (const t of speech.thinks) {
              process.stdout.write(`\n${C.dim}💭 ${t}${C.reset}`)
            }
            // zh as main display text (what master reads)
            if (speech.zh) {
              process.stdout.write(`\n${C.cyan}Elio${C.reset}: ${speech.zh}`)
            }
            // ja as subtitle (what's actually spoken)
            if (speech.ja) {
              process.stdout.write(`\n${C.dim}🎵 ${speech.ja}${C.reset}`)
            }
            process.stdout.write('\n')
            // Audio will arrive via system_notification/tts_ready
          } else if (!elioBuffer.includes('[调用工具') && !elioBuffer.includes('<personality-mode')) {
            process.stdout.write(`\n${C.cyan}Elio${C.reset}: ${elioBuffer}\n`)
          }
        }
        elioBuffer = ''
        promptLine()
        break

      case 'system_notification':
        if (msg.subtype === 'tts_ready' && msg.data) {
          console.log(C.dim + `\n🔊 播放中...` + C.reset)
          console.log()
          // Queue or play immediately
          if (audioPlaying) {
            audioQueue.push({ url: msg.data.audioUrl, zh: '' })
          } else {
            audioPlaying = true
            playAudioFile(msg.data.audioUrl)
          }
        }
        break

      case 'error':
        console.log(C.red + `\n❌ ${msg.message || '错误'}` + C.reset)
        promptLine()
        break
    }
  }
}

// ── Send ─────────────────────────────────────────────────────────────────

function send(message: string): void {
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    console.log(C.red + '未连接' + C.reset)
    promptLine()
    return
  }
  ws.send(JSON.stringify({ type: 'user_message', content: message }))
}

// ── Input loop ───────────────────────────────────────────────────────────

const rl = createInterface({ input: process.stdin, output: process.stdout })

function promptLine(): void {
  if (connected) {
    process.stdout.write(C.yellow + 'master> ' + C.reset)
  }
}

rl.on('line', (line: string) => {
  const text = line.trim()
  if (!text) {
    promptLine()
    return
  }
  if (text === '/quit' || text === '/exit') {
    console.log('再见~')
    if (ws) ws.close()
    rl.close()
    process.exit(0)
  }
  send(text)
})

rl.on('SIGINT', () => {
  console.log('\n再见~')
  if (ws) ws.close()
  rl.close()
  process.exit(0)
})

// ── Start ────────────────────────────────────────────────────────────────

console.log(C.magenta + '╔══════════════════╗' + C.reset)
console.log(C.magenta + '║   Elio 客户端     ║' + C.reset)
console.log(C.magenta + '╚══════════════════╝' + C.reset)
console.log(C.dim + `服务器: ${WS_URL}` + C.reset)
console.log(C.dim + '输入 /quit 退出' + C.reset)
console.log()

connect()
