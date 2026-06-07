import type { Edge, EdgeType, EventNode, TraversalResult, TraversalStep } from './types.js'
import type { GraphStore } from './GraphStore.js'

const ALL_DIMENSIONS: EdgeType[] = ['TEMPORAL', 'SEMANTIC', 'CAUSAL', 'ENTITY']

const MAX_HOPS = 2

// ── Single-dimension BFS ─────────────────────────────────────────────────

interface BfsNode {
  nodeId: string
  depth: number
  path: TraversalStep[]
}

function bfsOneDimension(
  graph: GraphStore,
  startId: string,
  type: EdgeType,
  maxHops: number,
): Map<string, { depth: number; paths: TraversalStep[][] }> {
  const visited = new Map<string, { depth: number; paths: TraversalStep[][] }>()
  visited.set(startId, { depth: 0, paths: [[]] })
  const queue: BfsNode[] = [{ nodeId: startId, depth: 0, path: [] }]

  while (queue.length > 0) {
    const { nodeId, depth, path } = queue.shift()!

    if (depth >= maxHops) continue

    // Follow outgoing edges of this type
    const outEdges = graph.getOutgoingEdges(nodeId, type)
    for (const edge of outEdges) {
      const nextId = edge.targetId
      const newStep: TraversalStep = { nodeId: nextId, edge, depth: depth + 1 }
      const newPath = [...path, newStep]

      if (!visited.has(nextId)) {
        visited.set(nextId, { depth: depth + 1, paths: [newPath] })
        queue.push({ nodeId: nextId, depth: depth + 1, path: newPath })
      } else {
        const existing = visited.get(nextId)!
        if (depth + 1 <= existing.depth) {
          existing.paths.push(newPath)
        }
      }
    }

    // Also follow incoming edges (reverse direction for entity/semantic)
    const inEdges = graph.getIncomingEdges(nodeId, type)
    for (const edge of inEdges) {
      const prevId = edge.sourceId
      const newStep: TraversalStep = { nodeId: prevId, edge, depth: depth + 1 }
      const newPath = [...path, newStep]

      if (!visited.has(prevId)) {
        visited.set(prevId, { depth: depth + 1, paths: [newPath] })
        queue.push({ nodeId: prevId, depth: depth + 1, path: newPath })
      } else {
        const existing = visited.get(prevId)!
        if (depth + 1 <= existing.depth) {
          existing.paths.push(newPath)
        }
      }
    }
  }

  return visited
}

// ── Multi-dimension Traversal ─────────────────────────────────────────────

/**
 * Traverse the graph from a set of anchor nodes across all 4 dimensions.
 * Each dimension is traversed independently up to `maxHops` hops.
 * Results are merged: node set is unioned, paths are concatenated.
 */
export function traverse(
  graph: GraphStore,
  anchorIds: string[],
  maxHops: number = MAX_HOPS,
): TraversalResult {
  const allVisited = new Map<string, EventNode>()
  const allEdges: Edge[] = []
  const allPaths: TraversalStep[][] = []

  for (const anchorId of anchorIds) {
    if (!graph.hasEvent(anchorId)) continue

    // Ensure anchor itself is included
    const anchorNode = graph.getEvent(anchorId)!
    allVisited.set(anchorId, anchorNode)

    for (const dim of ALL_DIMENSIONS) {
      const visited = bfsOneDimension(graph, anchorId, dim, maxHops)

      for (const [nodeId, info] of visited) {
        // Add node
        if (!allVisited.has(nodeId)) {
          const node = graph.getEvent(nodeId)
          if (node) allVisited.set(nodeId, node)
        }

        // Collect paths
        for (const p of info.paths) {
          if (p.length > 0) allPaths.push(p)
        }

        // Collect edges from paths
        for (const p of info.paths) {
          for (const step of p) {
            allEdges.push(step.edge)
          }
        }
      }
    }
  }

  // Deduplicate edges
  const edgeSet = new Set<string>()
  const uniqueEdges: Edge[] = []
  for (const e of allEdges) {
    const key = `${e.sourceId}→${e.targetId}:${e.type}:${e.subtype}`
    if (!edgeSet.has(key)) {
      edgeSet.add(key)
      uniqueEdges.push(e)
    }
  }

  return {
    anchorIds,
    nodes: allVisited,
    edges: uniqueEdges,
    paths: allPaths,
  }
}

// ── Dimension Breakdown ───────────────────────────────────────────────────

/** Count how many nodes were discovered through each dimension. */
export function breakdownByDimension(
  result: TraversalResult,
): Record<EdgeType, number> {
  const breakdown: Record<EdgeType, number> = { TEMPORAL: 0, SEMANTIC: 0, CAUSAL: 0, ENTITY: 0 }
  const seen = new Map<EdgeType, Set<string>>()

  for (const dim of ALL_DIMENSIONS) {
    seen.set(dim, new Set())
  }

  for (const step of result.paths.flat()) {
    seen.get(step.edge.type)?.add(step.nodeId)
  }

  for (const dim of ALL_DIMENSIONS) {
    breakdown[dim] = seen.get(dim)!.size
  }

  return breakdown
}

/**
 * Order traversed nodes by timestamp (oldest first).
 * Useful for building chronological narratives.
 */
export function sortByTimestamp(
  nodes: Map<string, EventNode>,
): EventNode[] {
  return Array.from(nodes.values()).sort((a, b) => a.timestamp - b.timestamp)
}
