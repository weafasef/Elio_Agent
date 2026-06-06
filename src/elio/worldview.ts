let baseWorldview: string | null = null // from heartbeat
let lastUserMessage: string | null = null // from user input

export function getWorldview(): string | null {
  if (!baseWorldview && !lastUserMessage) return null
  let result = baseWorldview || ''
  if (lastUserMessage) {
    result += `\n主人说: ${lastUserMessage}`
  }
  return result
}

export function setWorldview(worldview: string | null): void {
  baseWorldview = worldview
}

export function setLastUserMessage(msg: string | null): void {
  lastUserMessage = msg
}
