/**
 * 最简单的 Elio WebSocket 测试客户端
 *
 * 用法: bun test-client.ts [server端口 默认3456]
 *
 * 输入消息回车发送，观察 server 端日志确认 Elio 是否感知到。
 */

const serverPort = parseInt(process.argv[2] || '3456', 10)
const SESSION_ID = `test-${Date.now()}`
const WS_URL = `ws://127.0.0.1:${serverPort}/ws/${SESSION_ID}`

let seq = 0

function send(ws: WebSocket, type: string, payload: Record<string, unknown> = {}) {
  const msg = JSON.stringify({ type, ...payload })
  ws.send(msg)
}

console.log(`[Client] 连接 ${WS_URL}`)
const ws = new WebSocket(WS_URL)

ws.onopen = () => {
  console.log('[Client] 已连接。输入消息回车发送:\n')

  const stdin = Bun.stdin.stream()
  const reader = stdin.getReader()
  const decoder = new TextDecoder()
  async function readInput() {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      const text = decoder.decode(value).trim()
      if (!text) continue
      console.log(`[Client] 发送: "${text}"`)
      send(ws, 'user_message', { content: text })
    }
  }
  readInput()
}

ws.onmessage = (event) => {
  try {
    const msg = JSON.parse(event.data as string)
    const time = new Date().toLocaleTimeString()
    if (msg.type === 'status') {
      console.log(`[${time}] Server: ${msg.state} ${msg.verb || ''}`)
    } else {
      const preview = JSON.stringify(msg).slice(0, 200)
      console.log(`[${time}] ${msg.type}: ${preview}`)
    }
  } catch {}
}

ws.onclose = () => {
  console.log('[Client] 断开')
  process.exit(0)
}

ws.onerror = (err) => {
  console.error('[Client] WebSocket 错误:', (err as any)?.message || err)
}
