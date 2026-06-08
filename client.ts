/**
 * Elio 终端客户端
 * 用法: bun client.ts
 *
 * 连接到 Elio Server WebSocket，收发消息，自动播放 TTS 语音。
 */

import { createInterface } from 'node:readline'
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { homedir } from 'node:os'
import { spawn } from 'node:child_process'

// ── Config ──────────────────────────────────────────────────────────────

const WS_URL = 'ws://127.0.0.1:3456/ws/elio'
const AUDIO_DIR = join(homedir(), '.elio', 'audio')

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

let lastPlayedFile: string | null = null
let audioPollTimer: ReturnType<typeof setInterval> | null = null

function playAudio(filePath: string): void {
  // Windows: use PowerShell to play wav
  if (process.platform === 'win32') {
    spawn('powershell', [
      '-c',
      `(New-Object Media.SoundPlayer '${filePath}').PlaySync()`,
    ], { stdio: 'ignore' }).on('error', () => {})
  } else {
    spawn('ffplay', ['-nodisp', '-autoexit', filePath], { stdio: 'ignore' })
      .on('error', () => {})
  }
}

function findLatestAudio(): string | null {
  if (!existsSync(AUDIO_DIR)) return null
  const files = readdirSync(AUDIO_DIR).filter((f: string) => f.endsWith('.wav'))
  if (files.length === 0) return null

  let newest = files[0]
  let newestMtime = 0
  for (const f of files) {
    try {
      const mtime = statSync(join(AUDIO_DIR, f)).mtimeMs
      if (mtime > newestMtime) { newestMtime = mtime; newest = f }
    } catch {}
  }
  return join(AUDIO_DIR, newest)
}

function pollAndPlayAudio(): void {
  const latest = findLatestAudio()
  if (latest && latest !== lastPlayedFile) {
    lastPlayedFile = latest
    console.log(C.dim + `\n🔊 播放中...` + C.reset)
    playAudio(latest)

    // Try to read subtitle
    const subFile = latest.replace(/\.wav$/, '.subtitle.json')
    if (existsSync(subFile)) {
      try {
        const sub = JSON.parse(readFileSync(subFile, 'utf-8'))
        if (sub.zh) {
          console.log(C.dim + `📝 字幕: ${sub.zh}` + C.reset)
        }
      } catch {}
    }
    console.log() // newline after playback
  }
}

// ── Parse speech blocks ─────────────────────────────────────────────────

function parseSpeech(text: string): { ja: string; zh: string } | null {
  const jaMatch = text.match(/<ja>([\s\S]*?)<\/ja>/)
  const zhMatch = text.match(/<zh>([\s\S]*?)<\/zh>/)
  if (jaMatch && zhMatch) return { ja: jaMatch[1].trim(), zh: zhMatch[1].trim() }
  const stripped = text
    .replace(/\[调用工具:[^\]]*\]/g, '')
    .replace(/<personality-mode>[^<]*<\/personality-mode>/g, '')
    .trim()
  if (/[぀-ゟ゠-ヿ]/.test(stripped)) return { ja: stripped, zh: '' }
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
            process.stdout.write(`\n${C.cyan}Elio${C.reset}: ${speech.ja}`)
            if (speech.zh) {
              process.stdout.write(`\n${C.dim}📝 ${speech.zh}${C.reset}`)
            }
            process.stdout.write('\n')
            setTimeout(pollAndPlayAudio, 600)
          } else if (!elioBuffer.includes('[调用工具') && !elioBuffer.includes('<personality-mode')) {
            process.stdout.write(`\n${C.cyan}Elio${C.reset}: ${elioBuffer}\n`)
          }
        }
        elioBuffer = ''
        promptLine()
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
