import { GraphStore } from './GraphStore.js'
import { InvertedIndex } from './InvertedIndex.js'
import { FastPath } from './FastPath.js'
import { SlowPath } from './SlowPath.js'
import { ContextBridge } from './ContextBridge.js'
import {
  loadEvents,
  loadEdges,
  loadIndexData,
  saveAll,
} from './DiskIO.js'
import { join } from 'node:path'
import { logForDebugging } from '../../utils/debug.js'

const DEFAULT_MEMORY_DIR = join(
  process.env.HOME ?? process.env.USERPROFILE ?? '.',
  '.elio',
  'memory',
)

const DEEPSEEK_API_URL = 'https://api.deepseek.com/v1/chat/completions'

// ── Config ──────────────────────────────────────────────────────────────────

export interface MemoryAgentConfig {
  /** DeepSeek API key (required for Slow Path). */
  apiKey: string
  /** DeepSeek model name. Default: 'deepseek-v4-flash' */
  model?: string
  /** Directory for disk persistence. Default: ~/.elio/memory/ */
  diskDir?: string
}

// ── Singleton ───────────────────────────────────────────────────────────────

let instance: MemoryAgent | null = null

// ── MemoryAgent ─────────────────────────────────────────────────────────────

export class MemoryAgent {
  private graph: GraphStore
  private index: InvertedIndex
  private fastPath: FastPath
  private slowPath: SlowPath
  private diskDir: string
  private lastWorldview: string = ''

  constructor(config: MemoryAgentConfig) {
    this.diskDir = config.diskDir ?? DEFAULT_MEMORY_DIR

    // 1. Load persisted data
    const events = loadEvents(this.diskDir)
    const edges = loadEdges(this.diskDir)
    this.graph = GraphStore.fromJSON(events, edges)

    // 2. Restore or rebuild inverted index
    const savedIndex = loadIndexData(this.diskDir)
    this.index = savedIndex
      ? InvertedIndex.fromJSON(savedIndex)
      : new InvertedIndex()
    if (!savedIndex) {
      this.index.rebuild(this.graph)
    }

    // 3. Fast path
    this.fastPath = new FastPath(this.graph, this.index, this.diskDir)

    // 4. Slow path — DeepSeek LLM
    const apiKey = config.apiKey
    const model = config.model ?? 'deepseek-v4-flash'

    const callLLM = async (prompt: string): Promise<string> => {
      const response = await fetch(DEEPSEEK_API_URL, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${apiKey}`,
        },
        body: JSON.stringify({
          model,
          messages: [{ role: 'user', content: prompt }],
          max_tokens: 1024,
          temperature: 0.3,
        }),
      })
      if (!response.ok) {
        throw new Error(`DeepSeek API error: ${response.status} ${response.statusText}`)
      }
      const data = (await response.json()) as {
        choices?: Array<{ message?: { content?: string } }>
      }
      return data.choices?.[0]?.message?.content ?? ''
    }

    this.slowPath = new SlowPath(this.graph, {
      callLLM,
      intervalMs: 30_000,
      batchSize: 1,
      edgeThreshold: 0.7,
      index: this.index,
      diskDir: this.diskDir,
      onPersist: () => this.save(),
      onError: (err, eventId) => {
        logForDebugging(`[Memory] SlowPath error for ${eventId}: ${err.message}`)
      },
      onTick: (stats) => {
        if (stats.processed > 0) {
          logForDebugging(
            `[Memory] SlowPath: processed=${stats.processed}, ` +
            `failed=${stats.failed}, queue=${stats.queueLength}`,
          )
        }
      },
    })
  }

  // ── Lifecycle ─────────────────────────────────────────────────────────

  start(): void {
    this.slowPath.start()
    const msg = `[Memory] 里Agent启动 (${this.graph.eventCount} 事件, ${this.graph.edgeCount} 边, ${this.index.getKeywordCount()} 关键词)`
    console.log(msg)
    logForDebugging(msg)
  }

  stop(): void {
    this.slowPath.stop()
    this.save()
    logForDebugging('[Memory] Stopped')
  }

  // ── Event Capture ─────────────────────────────────────────────────────

  /** Capture a user message as a memory event. */
  captureUserMessage(text: string): string {
    const { eventId, durationMs } = this.fastPath.process({
      speaker: '主人',
      text,
    })
    this.slowPath.enqueue(eventId)
    return eventId
  }

  /** Capture a worldview pulse. Deduplicated: only stored if content changed. */
  captureWorldview(worldview: string): string | null {
    if (worldview === this.lastWorldview) return null
    this.lastWorldview = worldview

    const { eventId } = this.fastPath.process({
      speaker: 'system',
      text: worldview,
    })
    // Don't enqueue for slow path — worldview events are too frequent
    return eventId
  }

  /** Capture Elio's own response/action. */
  captureElioResponse(text: string): string {
    const { eventId } = this.fastPath.process({
      speaker: 'Elio',
      text,
    })
    this.slowPath.enqueue(eventId)
    return eventId
  }

  // ── Persistence ───────────────────────────────────────────────────────

  save(): void {
    saveAll(this.graph, this.index, this.diskDir)
  }

  // ── Accessors ─────────────────────────────────────────────────────────

  getStats(): {
    eventCount: number
    edgeCount: number
    edgeTypeBreakdown: Record<string, number>
    indexKeywords: number
    slowPathQueue: number
  } {
    const gs = this.graph.stats()
    return {
      ...gs,
      indexKeywords: this.index.getKeywordCount(),
      slowPathQueue: this.slowPath.queueLength,
    }
  }

  /** Direct access to the graph for debugging. */
  getGraph(): GraphStore {
    return this.graph
  }

  /** Force a slow path tick (for testing). */
  async forceTick(): Promise<{ processed: number; failed: number }> {
    return this.slowPath.forceTick()
  }
}

// ── Singleton accessors ─────────────────────────────────────────────────────

export function initMemoryAgent(config: MemoryAgentConfig): MemoryAgent {
  if (instance) {
    logForDebugging('[Memory] Already initialized, returning existing instance')
    return instance
  }
  instance = new MemoryAgent(config)
  instance.start()
  return instance
}

export function getMemoryAgent(): MemoryAgent | null {
  return instance
}
