import { join } from 'node:path'
import { existsSync, mkdirSync, readFileSync, writeFileSync, appendFileSync } from 'node:fs'
import type { Edge, EventNode } from './types.js'
import type { GraphStore } from './GraphStore.js'
import type { InvertedIndex } from './InvertedIndex.js'

const DEFAULT_MEMORY_DIR = join(
  process.env.HOME ?? process.env.USERPROFILE ?? '.',
  '.elio',
  'memory',
)

const EVENTS_FILE = 'events.jsonl'
const EDGES_FILE = 'edges.jsonl'
const INDEX_FILE = 'inverted_index.json'

// ── Helpers ──────────────────────────────────────────────────────────────

function ensureDir(dir: string): void {
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true })
  }
}

// ── Append (incremental, fast) ───────────────────────────────────────────

export function appendEvent(
  event: EventNode,
  dir: string = DEFAULT_MEMORY_DIR,
): void {
  ensureDir(dir)
  const line = JSON.stringify(event) + '\n'
  appendFileSync(join(dir, EVENTS_FILE), line, 'utf-8')
}

export function appendEdge(
  edge: Edge,
  dir: string = DEFAULT_MEMORY_DIR,
): void {
  ensureDir(dir)
  const line = JSON.stringify(edge) + '\n'
  appendFileSync(join(dir, EDGES_FILE), line, 'utf-8')
}

export function appendEdges(
  edges: Edge[],
  dir: string = DEFAULT_MEMORY_DIR,
): void {
  ensureDir(dir)
  const lines = edges.map(e => JSON.stringify(e)).join('\n') + '\n'
  appendFileSync(join(dir, EDGES_FILE), lines, 'utf-8')
}

// ── Full save (used on shutdown or periodic flush) ───────────────────────

export function saveAll(
  graph: GraphStore,
  index: InvertedIndex,
  dir: string = DEFAULT_MEMORY_DIR,
): void {
  ensureDir(dir)

  // Events: one JSON per line
  const eventLines = graph.getAllEvents().map(e => JSON.stringify(e)).join('\n') + '\n'
  writeFileSync(join(dir, EVENTS_FILE), eventLines, 'utf-8')

  // Edges: one JSON per line
  const edgeLines = graph.getAllEdgesGlobal().map(e => JSON.stringify(e)).join('\n') + '\n'
  writeFileSync(join(dir, EDGES_FILE), edgeLines, 'utf-8')

  // Index: still full JSON (compact enough)
  writeFileSync(join(dir, INDEX_FILE), JSON.stringify(index.toJSON()), 'utf-8')
}

// ── Load ─────────────────────────────────────────────────────────────────

export function loadEvents(dir: string = DEFAULT_MEMORY_DIR): EventNode[] {
  const path = join(dir, EVENTS_FILE)
  try {
    const raw = readFileSync(path, 'utf-8')
    const events: EventNode[] = []
    for (const line of raw.split('\n')) {
      const trimmed = line.trim()
      if (!trimmed) continue
      try {
        events.push(JSON.parse(trimmed))
      } catch {
        // Skip malformed lines
      }
    }
    return events
  } catch {
    return []
  }
}

export function loadEdges(dir: string = DEFAULT_MEMORY_DIR): Edge[] {
  const path = join(dir, EDGES_FILE)
  try {
    const raw = readFileSync(path, 'utf-8')
    const edges: Edge[] = []
    for (const line of raw.split('\n')) {
      const trimmed = line.trim()
      if (!trimmed) continue
      try {
        edges.push(JSON.parse(trimmed))
      } catch {
        // Skip malformed lines
      }
    }
    return edges
  } catch {
    return []
  }
}

export function loadIndexData(
  dir: string = DEFAULT_MEMORY_DIR,
): Record<string, string[]> | null {
  const path = join(dir, INDEX_FILE)
  try {
    const raw = readFileSync(path, 'utf-8')
    return JSON.parse(raw)
  } catch {
    return null
  }
}

// ── Stats ────────────────────────────────────────────────────────────────

export function getDiskStats(
  dir: string = DEFAULT_MEMORY_DIR,
): { eventsFile: string; edgesFile: string; indexFile: string; exists: boolean } {
  return {
    eventsFile: join(dir, EVENTS_FILE),
    edgesFile: join(dir, EDGES_FILE),
    indexFile: join(dir, INDEX_FILE),
    exists: existsSync(dir),
  }
}
