// ── Core Types ───────────────────────────────────────────────────────────

export interface EventNode {
  id: string
  timestamp: number
  rawText: string
  speaker: '主人' | 'Elio' | 'system'
  narrative: string | null
  entities: string[]
  emotion: Record<string, number> | null
  embedding: number[] | null
}

export type EdgeType = 'TEMPORAL' | 'SEMANTIC' | 'CAUSAL' | 'ENTITY'

// ── Edge Subtypes ────────────────────────────────────────────────────────

export const TemporalSubtype = {
  PRECEDES: 'PRECEDES',
  SUCCEEDS: 'SUCCEEDS',
  CONCURRENT: 'CONCURRENT',
} as const
export type TemporalSubtype =
  (typeof TemporalSubtype)[keyof typeof TemporalSubtype]

export const SemanticSubtype = {
  RELATED_TO: 'RELATED_TO',
  SIMILAR_TO: 'SIMILAR_TO',
  PART_OF: 'PART_OF',
  CONTAINS: 'CONTAINS',
} as const
export type SemanticSubtype =
  (typeof SemanticSubtype)[keyof typeof SemanticSubtype]

export const CausalSubtype = {
  LEADS_TO: 'LEADS_TO',
  BECAUSE_OF: 'BECAUSE_OF',
  ENABLES: 'ENABLES',
  PREVENTS: 'PREVENTS',
  RESPONSE_TO: 'RESPONSE_TO',
} as const
export type CausalSubtype =
  (typeof CausalSubtype)[keyof typeof CausalSubtype]

export const EntitySubtype = {
  REFERS_TO: 'REFERS_TO',
  MENTIONED_IN: 'MENTIONED_IN',
} as const
export type EntitySubtype =
  (typeof EntitySubtype)[keyof typeof EntitySubtype]

export type EdgeSubtype =
  | TemporalSubtype
  | SemanticSubtype
  | CausalSubtype
  | EntitySubtype

export interface Edge {
  sourceId: string
  targetId: string
  type: EdgeType
  subtype: EdgeSubtype
  weight: number
  createdBy: 'fast' | 'slow'
}

// ── Traversal ────────────────────────────────────────────────────────────

export interface TraversalStep {
  nodeId: string
  edge: Edge
  depth: number
}

export interface TraversalResult {
  anchorIds: string[]
  nodes: Map<string, EventNode>
  edges: Edge[]
  paths: TraversalStep[][]
}

// ── Context Bridge ───────────────────────────────────────────────────────

export interface MemoryContext {
  narrative: string
  anchorIds: string[]
  generatedAt: number
}

// ── Graph Snapshot ───────────────────────────────────────────────────────

export interface GraphSnapshot {
  events: EventNode[]
  edges: Edge[]
  savedAt: number
  version: number
}
