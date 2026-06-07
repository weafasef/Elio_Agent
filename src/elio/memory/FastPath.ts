import type { Edge, EventNode } from './types.js'
import { GraphStore } from './GraphStore.js'
import { InvertedIndex } from './InvertedIndex.js'
import { traverse } from './Traversal.js'
import { synthesize } from './Synthesizer.js'
import { ContextBridge } from './ContextBridge.js'
import { appendEvent, appendEdge as appendEdgeToDisk } from './DiskIO.js'

// ── FastPath ──────────────────────────────────────────────────────────────

/**
 * Fast Path — no LLM, pure local rules. Target: <100ms.
 *
 * Called immediately when a new message arrives (before the outer agent's
 * Sonnet call). Handles:
 *   1. Event node creation
 *   2. Temporal linking
 *   3. Keyword extraction + inverted index search → anchors
 *   4. 4D traversal from anchors (2 hops each dimension)
 *   5. Narrative synthesis → ContextBridge
 */
export class FastPath {
  private graph: GraphStore
  private index: InvertedIndex
  private eventCounter = 0
  private lastEventId: string | null = null
  private diskDir: string | null

  constructor(graph: GraphStore, index: InvertedIndex, diskDir?: string) {
    this.graph = graph
    this.index = index
    this.eventCounter = graph.eventCount
    this.diskDir = diskDir ?? null
    const latest = graph.getLatestEvent()
    if (latest) this.lastEventId = latest.id
  }

  /**
   * Process a new message through the Fast Path.
   * Returns the time taken in milliseconds (for monitoring).
   */
  process(params: {
    speaker: EventNode['speaker']
    text: string
    timestamp?: number
  }): { eventId: string; durationMs: number; anchorCount: number; resultSize: number } {
    const t0 = Date.now()

    const timestamp = params.timestamp ?? Date.now()

    // ── Step 1: Create event node ──────────────────────────────────────
    const eventId = this.nextId()
    const event: EventNode = {
      id: eventId,
      timestamp,
      rawText: params.text,
      speaker: params.speaker,
      narrative: null,
      entities: [],
      emotion: null,
      embedding: null,
    }
    this.graph.addEvent(event)
    if (this.diskDir) appendEvent(event, this.diskDir)

    // ── Step 2: Temporal edge ──────────────────────────────────────────
    if (this.lastEventId && this.lastEventId !== eventId) {
      const temporalEdge: Edge = {
        sourceId: this.lastEventId,
        targetId: eventId,
        type: 'TEMPORAL',
        subtype: 'PRECEDES',
        weight: 1.0,
        createdBy: 'fast',
      }
      this.graph.addEdge(temporalEdge)
      if (this.diskDir) appendEdgeToDisk(temporalEdge, this.diskDir)
    }
    this.lastEventId = eventId

    // ── Step 3: Search anchors (before indexing current event) ─────────
    const anchors = this.index.search(params.text, 5)

    // Supplement sparse anchors with recent temporal neighbors
    const supplementedAnchors = anchors.length < 3
      ? dedupe([...anchors, ...this.getRecentEventIds(3 - anchors.length)])
      : anchors

    // Index current event AFTER search so it doesn't match itself
    this.index.add(eventId, params.text)

    // ── Step 4: 4D traversal ───────────────────────────────────────────
    const result = traverse(this.graph, supplementedAnchors)

    // ── Step 5: Synthesize → ContextBridge ─────────────────────────────
    const narrative = synthesize(result, timestamp)
    ContextBridge.set(narrative, { anchorIds: anchors })

    const durationMs = Date.now() - t0
    return { eventId, durationMs, anchorCount: anchors.length, resultSize: result.nodes.size }
  }

  /** Process an Elio response (assistant turn). */
  processElioResponse(text: string, timestamp?: number): void {
    this.process({ speaker: 'Elio', text, timestamp })
  }

  /** Process a system-level event. */
  processSystemEvent(text: string, timestamp?: number): void {
    this.process({ speaker: 'system', text, timestamp })
  }

  private nextId(): string {
    this.eventCounter++
    return `E${this.eventCounter}`
  }

  /** Get the N most recent PAST event IDs via temporal reverse walk (excludes current). */
  private getRecentEventIds(count: number): string[] {
    const ids: string[] = []
    // Start from the previous event, not current
    const incoming = this.lastEventId
      ? this.graph.getIncomingEdges(this.lastEventId, 'TEMPORAL')
      : []
    let cursor = incoming.length > 0 ? incoming[0].sourceId : null
    while (cursor && ids.length < count) {
      ids.push(cursor)
      const prev = this.graph.getIncomingEdges(cursor, 'TEMPORAL')
      cursor = prev.length > 0 ? prev[0].sourceId : null
    }
    return ids
  }
}

function dedupe<T>(items: T[]): T[] {
  return [...new Set(items)]
}
