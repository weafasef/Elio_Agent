import type { Edge, EventNode } from './types.js'
import type { GraphStore } from './GraphStore.js'
import type { InvertedIndex } from './InvertedIndex.js'
import { traverse, sortByTimestamp } from './Traversal.js'
import { appendEdges } from './DiskIO.js'
import {
  buildNarrativePrompt,
  type NarrativeContextEvent,
} from './prompts/narrative.js'
import { buildCausalityPrompt, type EdgeInferenceTarget } from './prompts/causality.js'
import { getRetryDelay } from '../../services/api/withRetry.js'
import { logForDebugging } from '../../utils/debug.js'

// ── Types ────────────────────────────────────────────────────────────────

export interface SlowPathConfig {
  /** Interval in ms between processing ticks. Default: 30_000 */
  intervalMs: number
  /** Max events to process per tick. Default: 1 */
  batchSize: number
  /** Minimum edge confidence to accept. Default: 0.7 */
  edgeThreshold: number
  /** Max retries per event before giving up. Default: 3 */
  maxRetries: number
  /** Called when Slow Path needs an LLM. Returns the response text. */
  callLLM: (prompt: string) => Promise<string>
  /** Called on non-fatal errors. */
  onError?: (error: Error, eventId: string) => void
  /** Called after each tick with stats. */
  onTick?: (stats: { processed: number; failed: number; queueLength: number }) => void
  /** Called after each tick so caller can trigger a full save. */
  onPersist?: () => void
  /** InvertedIndex to update when narratives are enriched. */
  index?: InvertedIndex
  /** Disk directory for incremental edge persistence. */
  diskDir?: string
}

// ── SlowPath ─────────────────────────────────────────────────────────────

export class SlowPath {
  private graph: GraphStore
  private config: SlowPathConfig
  private queue: string[] = []
  private timer: ReturnType<typeof setInterval> | null = null
  private running = false
  private processed = new Set<string>()
  private retryCount = new Map<string, number>()
  private llmCache = new Map<string, { response: string; ts: number }>()

  constructor(graph: GraphStore, config: SlowPathConfig) {
    this.graph = graph
    this.config = {
      intervalMs: 30_000,
      batchSize: 1,
      edgeThreshold: 0.7,
      maxRetries: 3,
      ...config,
    }
  }

  // ── Lifecycle ────────────────────────────────────────────────────────

  start(): void {
    if (this.running) return
    this.running = true
    this.timer = setInterval(() => {
      void this.tick()
    }, this.config.intervalMs)
  }

  stop(): void {
    this.running = false
    if (this.timer) {
      clearInterval(this.timer)
      this.timer = null
    }
  }

  get isRunning(): boolean {
    return this.running
  }

  get queueLength(): number {
    return this.queue.length
  }

  // ── Enqueue ──────────────────────────────────────────────────────────

  /** Add an event ID to the processing queue. Called by FastPath after event creation. */
  enqueue(eventId: string): void {
    if (!this.processed.has(eventId)) {
      this.queue.push(eventId)
    }
  }

  /** Enqueue all unprocessed events from the graph. */
  enqueueAll(): void {
    for (const event of this.graph.getAllEvents()) {
      if (!this.processed.has(event.id) && !this.queue.includes(event.id)) {
        this.queue.push(event.id)
      }
    }
  }

  // ── Tick ─────────────────────────────────────────────────────────────

  /** Manually trigger one processing tick. Public for testing and forced flush. */
  async forceTick(): Promise<{ processed: number; failed: number }> {
    return this.tick()
  }

  private async tick(): Promise<{ processed: number; failed: number }> {
    if (this.queue.length === 0) return { processed: 0, failed: 0 }

    let processed = 0
    let failed = 0

    for (let i = 0; i < this.config.batchSize && this.queue.length > 0; i++) {
      const eventId = this.queue.shift()!

      // Check retry budget
      const attempts = this.retryCount.get(eventId) ?? 0
      if (attempts >= this.config.maxRetries) {
        this.processed.add(eventId)
        this.retryCount.delete(eventId)
        failed++
        if (this.config.onError) {
          this.config.onError(
            new Error(`Max retries (${this.config.maxRetries}) exceeded`),
            eventId,
          )
        }
        continue
      }

      // Exponential backoff with jitter (project standard)
      if (attempts > 0) {
        const delay = getRetryDelay(attempts)
        await new Promise(resolve => setTimeout(resolve, delay))
      }

      try {
        await this.processEvent(eventId)
        this.processed.add(eventId)
        this.retryCount.delete(eventId)
        processed++
      } catch (err) {
        failed++
        const count = (this.retryCount.get(eventId) ?? 0) + 1
        this.retryCount.set(eventId, count)

        // Re-queue at end for retry (unless max reached)
        if (count < this.config.maxRetries) {
          this.queue.push(eventId)
        } else {
          this.processed.add(eventId)
          this.retryCount.delete(eventId)
        }

        if (this.config.onError) {
          this.config.onError(
            err instanceof Error ? err : new Error(String(err)),
            eventId,
          )
        }
      }
    }

    if (processed > 0 && this.config.onPersist) {
      this.config.onPersist()
    }

    if (this.config.onTick) {
      this.config.onTick({ processed, failed, queueLength: this.queue.length })
    }

    return { processed, failed }
  }

  // ── LLM Call with Cache ──────────────────────────────────────────────

  private static readonly CACHE_MAX = 500

  /** Call LLM with prompt → response caching. Evicts oldest half when full. */
  private async cachedCallLLM(prompt: string): Promise<string> {
    const cached = this.llmCache.get(prompt)
    if (cached) return cached.response

    const response = await this.config.callLLM(prompt)

    if (this.llmCache.size >= SlowPath.CACHE_MAX) {
      const entries = Array.from(this.llmCache.entries())
        .sort((a, b) => a[1].ts - b[1].ts)
      const toRemove = entries.slice(0, Math.floor(SlowPath.CACHE_MAX / 2))
      for (const [k] of toRemove) this.llmCache.delete(k)
    }

    this.llmCache.set(prompt, { response, ts: Date.now() })
    return response
  }

  // ── Process One Event ────────────────────────────────────────────────

  private async processEvent(eventId: string): Promise<void> {
    const event = this.graph.getEvent(eventId)
    if (!event) return

    // Skip narrative extraction if already enriched
    if (event.narrative && event.entities.length > 0) {
      // Only need edge inference on re-process
      const neighbors = this.getNeighborEvents(eventId)
      await this.inferEdges(event, neighbors)
      return
    }

    // Phase 1: Get neighbor context
    const neighbors = this.getNeighborEvents(eventId)

    // Phase 2: Extract narrative + entities + emotion via LLM
    await this.extractNarrative(event, neighbors)

    // Phase 3: Infer edges via LLM
    await this.inferEdges(event, neighbors)
  }

  // ── Phase 2: Narrative Extraction ────────────────────────────────────

  private async extractNarrative(
    event: EventNode,
    neighbors: EventNode[],
  ): Promise<void> {
    const currentEventCtx = eventToCtx(event)
    const neighborCtxs = neighbors.map(eventToCtx)

    const prompt = buildNarrativePrompt(currentEventCtx, neighborCtxs)
    const response = await this.cachedCallLLM(prompt)

    try {
      const parsed = JSON.parse(extractJSON(response))
      if (parsed.narrative && typeof parsed.narrative === 'string') {
        event.narrative = parsed.narrative
        // Re-index with enriched narrative for better keyword matching
        this.config.index?.update(event.id, parsed.narrative)
      }
      if (Array.isArray(parsed.entities)) {
        event.entities = parsed.entities.filter(
          (e: unknown): e is string => typeof e === 'string',
        )
      }
      if (parsed.emotion && typeof parsed.emotion === 'object') {
        event.emotion = parsed.emotion as Record<string, number>
      }
    } catch {
      // If JSON parsing fails, store raw response as narrative
      event.narrative = response.slice(0, 300)
    }
  }

  // ── Phase 3: Edge Inference ──────────────────────────────────────────

  private async inferEdges(
    event: EventNode,
    neighbors: EventNode[],
  ): Promise<void> {
    if (neighbors.length === 0) return

    const target: EdgeInferenceTarget = {
      id: event.id,
      narrative: event.narrative ?? '',
      entities: event.entities,
      rawText: event.rawText,
    }

    const neighborTargets: EdgeInferenceTarget[] = neighbors.slice(0, 8).map(n => ({
      id: n.id,
      narrative: n.narrative ?? '',
      entities: n.entities,
      rawText: n.rawText,
    }))

    const prompt = buildCausalityPrompt(target, neighborTargets)
    if (!prompt) return

    const response = await this.cachedCallLLM(prompt)

    try {
      const edges = JSON.parse(extractJSON(response)) as Edge[]
      if (!Array.isArray(edges)) return

      const newEdges: Edge[] = []

      for (const edge of edges) {
        if (
          edge.sourceId &&
          edge.targetId &&
          edge.type &&
          edge.subtype &&
          (edge.weight ?? 0) >= this.config.edgeThreshold
        ) {
          const newEdge: Edge = {
            sourceId: edge.sourceId,
            targetId: edge.targetId,
            type: edge.type,
            subtype: edge.subtype,
            weight: edge.weight,
            createdBy: 'slow',
          }
          this.graph.addEdge(newEdge)
          newEdges.push(newEdge)
        }
      }

      // Incremental disk persistence
      if (newEdges.length > 0 && this.config.diskDir) {
        appendEdges(newEdges, this.config.diskDir)
      }
    } catch {
      // Edge inference failure is non-fatal
    }
  }

  // ── Neighbor Discovery ───────────────────────────────────────────────

  private getNeighborEvents(eventId: string): EventNode[] {
    // 2-hop traversal from the event itself
    const result = traverse(this.graph, [eventId], 2)
    const events = sortByTimestamp(result.nodes)

    // Exclude the event itself
    return events.filter(e => e.id !== eventId)
  }
}

// ── Helpers ──────────────────────────────────────────────────────────────

function eventToCtx(event: EventNode): NarrativeContextEvent {
  return {
    id: event.id,
    speaker: event.speaker,
    text: event.rawText,
    timestamp: new Date(event.timestamp).toISOString(),
  }
}

/** Extract JSON object/array from a string that may have surrounding text. */
function extractJSON(text: string): string {
  // Find first [ or { and last ] or }
  const start = Math.min(
    text.indexOf('{') === -1 ? Infinity : text.indexOf('{'),
    text.indexOf('[') === -1 ? Infinity : text.indexOf('['),
  )
  const end = Math.max(text.lastIndexOf('}'), text.lastIndexOf(']'))

  if (start === Infinity || end === -1 || start >= end) return text
  return text.slice(start, end + 1)
}
