/**
 * Bounded ring buffer for UUID deduplication.
 *
 * Originally in bridge/bridgeMessaging.ts — moved here when bridge was deleted.
 * Used by useRemoteSession.ts to filter out echoed user messages from the WebSocket
 * stream, capping memory at maxSize entries via FIFO eviction.
 */
export class BoundedUUIDSet {
  private map = new Map<string, number>()
  private order: string[] = []
  private maxSize: number

  constructor(maxSize = 500) {
    this.maxSize = maxSize
  }

  add(id: string): boolean {
    if (this.map.has(id)) return false
    this.map.set(id, this.order.length)
    this.order.push(id)
    if (this.order.length > this.maxSize) {
      const evicted = this.order.shift()!
      this.map.delete(evicted)
    }
    return true
  }

  has(id: string): boolean {
    return this.map.has(id)
  }

  delete(id: string): boolean {
    if (!this.map.has(id)) return false
    this.map.delete(id)
    return true
  }

  clear(): void {
    this.map.clear()
    this.order = []
  }

  get size(): number {
    return this.map.size
  }
}
