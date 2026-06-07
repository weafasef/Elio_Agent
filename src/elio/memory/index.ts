// ── Barrel export for src/elio/memory/ ──────────────────────────────────

export { GraphStore } from './GraphStore.js'
export { InvertedIndex, extractKeywords } from './InvertedIndex.js'
export { FastPath } from './FastPath.js'
export { SlowPath } from './SlowPath.js'
export type { SlowPathConfig } from './SlowPath.js'
export { traverse, breakdownByDimension, sortByTimestamp } from './Traversal.js'
export { synthesize } from './Synthesizer.js'
export { ContextBridge } from './ContextBridge.js'
export { MemoryAgent, initMemoryAgent, getMemoryAgent } from './MemoryAgent.js'
export type { MemoryAgentConfig } from './MemoryAgent.js'
export {
  appendEvent,
  appendEdge,
  appendEdges,
  saveAll,
  loadEvents,
  loadEdges,
  loadIndexData,
  getDiskStats,
} from './DiskIO.js'
export {
  buildNarrativePrompt,
  buildCausalityPrompt,
  buildEntityPrompt,
} from './prompts/index.js'
export type {
  EventNode,
  Edge,
  EdgeType,
  EdgeSubtype,
  TemporalSubtype,
  SemanticSubtype,
  CausalSubtype,
  EntitySubtype,
  TraversalResult,
  TraversalStep,
  MemoryContext,
  GraphSnapshot,
} from './types.js'
