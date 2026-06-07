import type { EventNode, TraversalResult } from './types.js'
import { sortByTimestamp } from './Traversal.js'

const MAX_NARRATIVE_ITEMS = 8

// ── Time formatting ───────────────────────────────────────────────────────

function relativeTime(ts: number, now: number = Date.now()): string {
  const diff = now - ts
  const minutes = Math.floor(diff / 60_000)
  const hours = Math.floor(diff / 3_600_000)
  const days = Math.floor(diff / 86_400_000)

  if (minutes < 1) return '刚才'
  if (minutes < 60) return `${minutes}分钟前`
  if (hours < 24) return `${hours}小时前`
  if (days < 7) return `${days}天前`
  if (days < 30) return `${Math.floor(days / 7)}周前`
  return `${Math.floor(days / 30)}个月前`
}

function formatDate(ts: number): string {
  const d = new Date(ts)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

// ── Synthesis ─────────────────────────────────────────────────────────────

/**
 * Synthesize a traversal result into a natural language narrative.
 * Pure template-based (no LLM) — runs in <10ms.
 */
export function synthesize(
  result: TraversalResult,
  now: number = Date.now(),
): string {
  if (result.nodes.size === 0) return ''

  const events = sortByTimestamp(result.nodes)
  const lines: string[] = ['相关记忆：']

  // ── Narrative items ─────────────────────────────────────────────────
  const shown = events.slice(0, MAX_NARRATIVE_ITEMS)
  for (let i = 0; i < shown.length; i++) {
    const e = shown[i]
    const timeLabel = relativeTime(e.timestamp, now)
    const dateLabel = formatDate(e.timestamp)
    const speaker = e.speaker === '主人' ? '主人' : e.speaker === 'Elio' ? 'Elio' : ''

    // Use Slow Path narrative if available, else raw text (truncated)
    const text = e.narrative ?? truncate(e.rawText, 120)

    lines.push(`${i + 1}. ${timeLabel}(${dateLabel})，${speaker ? `${speaker}: ` : ''}${text}`)
  }

  if (events.length > MAX_NARRATIVE_ITEMS) {
    lines.push(`... 还有 ${events.length - MAX_NARRATIVE_ITEMS} 条相关记忆`)
  }

  // ── Core entities ───────────────────────────────────────────────────
  const entities = collectEntities(result)
  if (entities.length > 0) {
    lines.push('')
    lines.push(`核心实体：${entities.join('、')}`)
  }

  // ── Causal chains ───────────────────────────────────────────────────
  const chains = listCausalRelations(result)
  if (chains.length > 0) {
    lines.push('')
    lines.push('因果链：')
    for (const chain of chains.slice(0, 3)) {
      lines.push(`  ${chain}`)
    }
  }

  return lines.join('\n')
}

// ── Helpers ───────────────────────────────────────────────────────────────

function truncate(text: string, maxLen: number): string {
  if (text.length <= maxLen) return text
  return text.slice(0, maxLen) + '...'
}

function collectEntities(result: TraversalResult): string[] {
  const entitySet = new Set<string>()
  for (const event of result.nodes.values()) {
    for (const ent of event.entities) {
      entitySet.add(ent)
    }
  }
  return Array.from(entitySet).slice(0, 10)
}

function listCausalRelations(result: TraversalResult): string[] {
  const eventMap = result.nodes
  // Build forward adjacency: sourceId → [{targetId, subtype}]
  const forward = new Map<string, { targetId: string; subtype: string }[]>()
  const hasIncoming = new Set<string>()

  for (const edge of result.edges) {
    if (edge.type !== 'CAUSAL') continue
    if (!forward.has(edge.sourceId)) {
      forward.set(edge.sourceId, [])
    }
    forward.get(edge.sourceId)!.push({
      targetId: edge.targetId,
      subtype: edge.subtype,
    })
    hasIncoming.add(edge.targetId)
  }

  // Find roots: nodes with outgoing edges but no incoming
  const roots = [...forward.keys()].filter(id => !hasIncoming.has(id))

  // Build chains from each root
  const chains: string[] = []
  const used = new Set<string>()

  for (const root of roots) {
    const chainIds: string[] = []
    let cursor: string | undefined = root

    while (cursor && forward.has(cursor) && chainIds.length < 5) {
      chainIds.push(cursor)
      used.add(cursor)

      const next = forward.get(cursor)!.find(e => !used.has(e.targetId))
      cursor = next?.targetId
    }

    if (chainIds.length >= 2) {
      const texts = chainIds.map(id => {
        const node = eventMap.get(id)
        return node ? (node.narrative ?? truncate(node.rawText, 30)) : id
      })
      chains.push(texts.join(' → '))
    }
  }

  // Any leftover causal edges not in a chain
  for (const edge of result.edges) {
    if (edge.type !== 'CAUSAL') continue
    if (used.has(edge.sourceId) && used.has(edge.targetId)) continue

    const source = eventMap.get(edge.sourceId)
    const target = eventMap.get(edge.targetId)
    if (!source || !target) continue

    const sourceText = source.narrative ?? truncate(source.rawText, 30)
    const targetText = target.narrative ?? truncate(target.rawText, 30)
    const arrow = edge.subtype === 'LEADS_TO' ? '→' : edge.subtype === 'BECAUSE_OF' ? '←' : '—'
    chains.push(`"${sourceText}" ${arrow} "${targetText}"`)
    used.add(edge.sourceId)
    used.add(edge.targetId)
  }

  return chains
}
