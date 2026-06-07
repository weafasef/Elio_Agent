import type { Edge, EdgeType, EventNode } from './types.js'

// ── Adjacency Bucket ─────────────────────────────────────────────────────

interface AdjacencyBucket {
  temporal: Edge[]
  semantic: Edge[]
  causal: Edge[]
  entity: Edge[]
}

function createEmptyBucket(): AdjacencyBucket {
  return { temporal: [], semantic: [], causal: [], entity: [] }
}

function hasEdge(existing: Edge[], edge: Edge): boolean {
  return existing.some(
    e =>
      e.sourceId === edge.sourceId &&
      e.targetId === edge.targetId &&
      e.type === edge.type &&
      e.subtype === edge.subtype,
  )
}

function bucketKey(type: EdgeType): keyof AdjacencyBucket {
  switch (type) {
    case 'TEMPORAL':
      return 'temporal'
    case 'SEMANTIC':
      return 'semantic'
    case 'CAUSAL':
      return 'causal'
    case 'ENTITY':
      return 'entity'
  }
}

// ── GraphStore ───────────────────────────────────────────────────────────

export class GraphStore {
  private events: Map<string, EventNode> = new Map()
  private forwardAdj: Map<string, AdjacencyBucket> = new Map()
  private reverseAdj: Map<string, AdjacencyBucket> = new Map()

  // ── Event CRUD ─────────────────────────────────────────────────────

  addEvent(event: EventNode): void {
    this.events.set(event.id, event)
    if (!this.forwardAdj.has(event.id)) {
      this.forwardAdj.set(event.id, createEmptyBucket())
    }
    if (!this.reverseAdj.has(event.id)) {
      this.reverseAdj.set(event.id, createEmptyBucket())
    }
  }

  getEvent(id: string): EventNode | undefined {
    return this.events.get(id)
  }

  hasEvent(id: string): boolean {
    return this.events.has(id)
  }

  get eventCount(): number {
    return this.events.size
  }

  getAllEvents(): EventNode[] {
    return Array.from(this.events.values())
  }

  /** Get the event with the highest timestamp. Equal timestamps → last-inserted wins. */
  getLatestEvent(): EventNode | undefined {
    let latest: EventNode | undefined
    for (const e of this.events.values()) {
      if (!latest || e.timestamp >= latest.timestamp) {
        latest = e
      }
    }
    return latest
  }

  // ── Edge CRUD ───────────────────────────────────────────────────────

  addEdge(edge: Edge): void {
    const fwdKey = bucketKey(edge.type)
    const revKey = bucketKey(edge.type)

    // Forward: source → target
    if (!this.forwardAdj.has(edge.sourceId)) {
      this.forwardAdj.set(edge.sourceId, createEmptyBucket())
    }
    const fwdBucket = this.forwardAdj.get(edge.sourceId)!
    if (!hasEdge(fwdBucket[fwdKey], edge)) {
      fwdBucket[fwdKey].push(edge)
    }

    // Reverse: target → source
    if (!this.reverseAdj.has(edge.targetId)) {
      this.reverseAdj.set(edge.targetId, createEmptyBucket())
    }
    const revBucket = this.reverseAdj.get(edge.targetId)!
    if (!hasEdge(revBucket[revKey], edge)) {
      revBucket[revKey].push(edge)
    }
  }

  addEdges(edges: Edge[]): void {
    for (const e of edges) this.addEdge(e)
  }

  /** Get all outgoing edges of a given type from a node. */
  getOutgoingEdges(nodeId: string, type?: EdgeType): Edge[] {
    const bucket = this.forwardAdj.get(nodeId)
    if (!bucket) return []
    if (!type) {
      return [
        ...bucket.temporal,
        ...bucket.semantic,
        ...bucket.causal,
        ...bucket.entity,
      ]
    }
    return [...bucket[bucketKey(type)]]
  }

  /** Get all incoming edges of a given type to a node. */
  getIncomingEdges(nodeId: string, type?: EdgeType): Edge[] {
    const bucket = this.reverseAdj.get(nodeId)
    if (!bucket) return []
    if (!type) {
      return [
        ...bucket.temporal,
        ...bucket.semantic,
        ...bucket.causal,
        ...bucket.entity,
      ]
    }
    return [...bucket[bucketKey(type)]]
  }

  /** Get all edges (in + out) for a node. */
  getAllEdges(nodeId: string, type?: EdgeType): Edge[] {
    return [
      ...this.getOutgoingEdges(nodeId, type),
      ...this.getIncomingEdges(nodeId, type),
    ]
  }

  /** Get all edges in the graph. */
  getAllEdgesGlobal(): Edge[] {
    const edges: Edge[] = []
    for (const bucket of this.forwardAdj.values()) {
      edges.push(
        ...bucket.temporal,
        ...bucket.semantic,
        ...bucket.causal,
        ...bucket.entity,
      )
    }
    return edges
  }

  get edgeCount(): number {
    let count = 0
    for (const bucket of this.forwardAdj.values()) {
      count +=
        bucket.temporal.length +
        bucket.semantic.length +
        bucket.causal.length +
        bucket.entity.length
    }
    return count
  }

  // ── Neighbors ────────────────────────────────────────────────────────

  /** Get neighbor IDs along outgoing edges of given type. */
  getNeighbors(nodeId: string, type?: EdgeType): string[] {
    const edges = this.getOutgoingEdges(nodeId, type)
    return [...new Set(edges.map(e => e.targetId))]
  }

  /**
   * Multi-hop traversal along a specific edge type.
   * Returns all node IDs reachable within `maxHops` steps.
   */
  traverseHops(
    startId: string,
    type: EdgeType,
    maxHops: number,
  ): Map<string, number> {
    const visited = new Map<string, number>()
    visited.set(startId, 0)
    const queue: [string, number][] = [[startId, 0]]

    while (queue.length > 0) {
      const [currentId, depth] = queue.shift()!
      if (depth >= maxHops) continue

      for (const neighborId of this.getNeighbors(currentId, type)) {
        if (!visited.has(neighborId)) {
          visited.set(neighborId, depth + 1)
          queue.push([neighborId, depth + 1])
        }
      }
    }

    return visited
  }

  // ── Snapshots ────────────────────────────────────────────────────────

  toJSON(): { events: EventNode[]; edges: Edge[] } {
    return {
      events: Array.from(this.events.values()),
      edges: this.getAllEdgesGlobal(),
    }
  }

  static fromJSON(
    events: EventNode[],
    edges: Edge[],
  ): GraphStore {
    const store = new GraphStore()
    for (const e of events) store.addEvent(e)
    for (const e of edges) store.addEdge(e)
    return store
  }

  // ── Stats ────────────────────────────────────────────────────────────

  stats(): {
    eventCount: number
    edgeCount: number
    edgeTypeBreakdown: Record<EdgeType, number>
  } {
    const breakdown: Record<EdgeType, number> = {
      TEMPORAL: 0,
      SEMANTIC: 0,
      CAUSAL: 0,
      ENTITY: 0,
    }
    for (const bucket of this.forwardAdj.values()) {
      breakdown.TEMPORAL += bucket.temporal.length
      breakdown.SEMANTIC += bucket.semantic.length
      breakdown.CAUSAL += bucket.causal.length
      breakdown.ENTITY += bucket.entity.length
    }
    return {
      eventCount: this.events.size,
      edgeCount: this.edgeCount,
      edgeTypeBreakdown: breakdown,
    }
  }
}
