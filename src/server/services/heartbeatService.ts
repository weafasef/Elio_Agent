/**
 * Heartbeat Service — thin timer that triggers MainLoop on a fixed interval.
 *
 * Owns only the setInterval / clearInterval lifecycle. All core logic
 * (worldview assembly, session management, output handling) lives in MainLoop.
 */

import { MainLoop } from './MainLoop.js'

const INTERVAL_MS = 30_000

let intervalId: ReturnType<typeof setInterval> | null = null

export function startHeartbeat(serverPort: number): void {
  if (intervalId) return
  MainLoop.init(serverPort)
  intervalId = setInterval(() => MainLoop.step(), INTERVAL_MS)
  console.log('[Heartbeat] Started')
}

export function stopHeartbeat(): void {
  if (intervalId) {
    clearInterval(intervalId)
    intervalId = null
  }
  MainLoop.shutdown()
  console.log('[Heartbeat] Stopped')
}
