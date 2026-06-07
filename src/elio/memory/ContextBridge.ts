import type { MemoryContext } from './types.js'

// ── ContextBridge ─────────────────────────────────────────────────────────

/**
 * Shared-memory bridge between inner (memory) and outer (Elio) agents.
 *
 * - 里 Agent writes `sharedContext` after Fast Path synthesis
 * - 表 Agent reads `sharedContext` when assembling the system prompt
 *
 * Both agents run in the same process, so this is a zero-cost module-scoped
 * variable swap. Thread safety is not a concern in JS's single-threaded model.
 */
let sharedContext = ''
let lastContext: MemoryContext | null = null

export const ContextBridge = {
  /** Inner agent: write the synthesized memory context. */
  set(context: string, metadata?: { anchorIds: string[] }): void {
    sharedContext = context
    lastContext = {
      narrative: context,
      anchorIds: metadata?.anchorIds ?? [],
      generatedAt: Date.now(),
    }
  },

  /** Outer agent: read the current memory context for system prompt injection. */
  get(): string {
    return sharedContext
  },

  /** Get the full context object with metadata. */
  getLastContext(): MemoryContext | null {
    return lastContext
  },

  /** Check if any context has been set. */
  hasContext(): boolean {
    return sharedContext.length > 0
  },

  /** Clear context (e.g. on session reset). */
  clear(): void {
    sharedContext = ''
    lastContext = null
  },

  /** Get a summary of when the context was last updated. */
  getAge(): { generatedAt: number; ageMs: number } | null {
    if (!lastContext) return null
    return {
      generatedAt: lastContext.generatedAt,
      ageMs: Date.now() - lastContext.generatedAt,
    }
  },
}
