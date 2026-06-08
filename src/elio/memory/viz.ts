/**
 * 四维记忆图可视化脚本
 * Run: bun run src/elio/memory/viz.ts
 *
 * 输入一段模拟对话，逐步展示四维图的变化过程。
 */

import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { mkdtempSync, rmSync } from 'node:fs'
import { MemoryAgent } from './MemoryAgent.js'
import { ContextBridge } from './ContextBridge.js'

// ── Terminal colors ─────────────────────────────────────────────────────────

const C = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  dim: '\x1b[2m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  magenta: '\x1b[35m',
  cyan: '\x1b[36m',
  white: '\x1b[37m',
  bgRed: '\x1b[41m',
  bgGreen: '\x1b[42m',
  bgYellow: '\x1b[43m',
  bgBlue: '\x1b[44m',
  bgMagenta: '\x1b[45m',
  bgCyan: '\x1b[46m',
}

// ── Helper ──────────────────────────────────────────────────────────────────

const DIM_COLORS = {
  TEMPORAL: C.cyan,
  SEMANTIC: C.blue,
  CAUSAL: C.magenta,
  ENTITY: C.green,
}
const DIM_ICONS = { TEMPORAL: '⏳', SEMANTIC: '🔗', CAUSAL: '⚡', ENTITY: '🏷️' }
const DIM_LABELS = { TEMPORAL: '时间', SEMANTIC: '语义', CAUSAL: '因果', ENTITY: '实体' }

function sleep(ms: number): Promise<void> {
  return new Promise(r => setTimeout(r, ms))
}

function hr(title?: string): void {
  console.log(C.dim + '─'.repeat(60) + C.reset)
  if (title) console.log(C.bold + title + C.reset)
}

function dimBadge(label: string, color: string): string {
  return `${color}${label}${C.reset}`
}

// ── Scenario ────────────────────────────────────────────────────────────────

interface ScenarioStep {
  speaker: 'master' | 'Elio' | 'system'
  text: string
  pause?: number // extra pause after this step (ms)
}

const SCENARIO: ScenarioStep[] = [
  {
    speaker: 'master',
    text: 'Elio，我今天开始学 Python 了，从最基础的变量和循环开始。',
  },
  {
    speaker: 'Elio',
    text: '太好了 master！Python 是很好的入门语言。变量就像贴了标签的盒子，循环就像重复做一件事。需要我帮你做什么练习吗？',
  },
  {
    speaker: 'master',
    text: '帮我整理一下今天学的知识点：变量赋值、if 判断、for 循环、while 循环。',
    pause: 300,
  },
  {
    speaker: 'system',
    text: '当前时间: 2026-06-07 15:00:00（下午）\n本次持续运行: 30 分钟\n你可以自主决定做点什么。',
  },
  {
    speaker: 'Elio',
    text: '好的，我帮你整理成一张表格：\n1. 变量赋值 x = 10\n2. if 判断: if x > 5: print("大")\n3. for 循环: for i in range(10)\n4. while 循环: while True',
    pause: 500,
  },
  {
    speaker: 'master',
    text: 'for 循环和 while 循环有什么区别？我搞混了。',
  },
  {
    speaker: 'Elio',
    text: 'for 循环是"遍历已知集合"——你知道要循环几次。while 循环是"满足条件就继续"——你不知道要循环几次，直到条件不满足为止。比如：for i in range(5) 循环 5 次，while x > 0 一直循环到 x 不大于 0。',
    pause: 500,
  },
]

// ── Visualizers ─────────────────────────────────────────────────────────────

function showEvent(event: { id: string; speaker: string; rawText: string; timestamp: number }): void {
  const speakerColor = event.speaker === 'master' ? C.yellow :
    event.speaker === 'Elio' ? C.cyan :
    C.dim
  const line = event.rawText.length > 80
    ? event.rawText.slice(0, 80) + '...'
    : event.rawText
  console.log(`  ${C.bold}${event.id}${C.reset} ${speakerColor}[${event.speaker}]${C.reset} ${line}`)
}

function showGraph(agent: MemoryAgent): void {
  const graph = agent.getGraph()
  const stats = agent.getStats()

  // ── Events ──
  console.log(C.bold + '\n📋 事件节点' + C.reset + C.dim + ` (${stats.eventCount} total)` + C.reset)
  const events = graph.getAllEvents()
  const latest = events.slice(-5)
  for (const e of latest) {
    showEvent(e)
  }
  if (events.length > 5) {
    console.log(C.dim + `  ... 还有 ${events.length - 5} 个更早的事件` + C.reset)
  }

  // ── Edges by dimension ──
  console.log(C.bold + '\n🔗 边 (Edges)' + C.reset + C.dim + ` (${stats.edgeCount} total)` + C.reset)

  type DimKey = 'TEMPORAL' | 'SEMANTIC' | 'CAUSAL' | 'ENTITY'
  for (const dim of ['TEMPORAL', 'SEMANTIC', 'CAUSAL', 'ENTITY'] as DimKey[]) {
    const allEdges = graph.getAllEdgesGlobal()
    const dimEdges = allEdges.filter(e => e.type === dim)
    if (dimEdges.length === 0) continue

    const color = DIM_COLORS[dim]
    const icon = DIM_ICONS[dim]
    const label = DIM_LABELS[dim]
    console.log(`  ${icon} ${color}${label}${C.reset} ${C.dim}(${dimEdges.length}条)${C.reset}`)

    const shown = dimEdges.slice(-4)
    for (const e of shown) {
      const src = graph.getEvent(e.sourceId)
      const tgt = graph.getEvent(e.targetId)
      const srcLabel = src ? `${src.id}(${src.speaker})` : e.sourceId
      const tgtLabel = tgt ? `${tgt.id}(${tgt.speaker})` : e.targetId
      const subtype = e.subtype || '-'
      const maker = e.createdBy === 'fast' ? C.dim + '(F)' + C.reset : C.yellow + '(S)' + C.reset
      console.log(`    ${C.dim}${srcLabel}${C.reset} ${color}→${C.reset} ${C.dim}${tgtLabel}${C.reset}  ${C.dim}[${subtype}]${C.reset} ${maker}`)
    }
    if (dimEdges.length > 4) {
      console.log(C.dim + `    ... 还有 ${dimEdges.length - 4} 条` + C.reset)
    }
  }

  // ── Index ──
  console.log(C.bold + '\n📇 倒排索引' + C.reset + C.dim + ` (${stats.indexKeywords} 个关键词)` + C.reset)
  const kwSample = agent['index'].getKeywords().slice(0, 15)
  console.log(`  ${kwSample.join('  ')}`)

  // ── Queue ──
  if (stats.slowPathQueue > 0) {
    console.log(C.yellow + `\n⏳ SlowPath 队列: ${stats.slowPathQueue} 个待处理` + C.reset)
  }
}

function showSynthesis(): void {
  const ctx = ContextBridge.get()
  if (!ctx) return

  hr('📝 ContextBridge (系统提示词注入内容)')
  console.log(C.green + ctx + C.reset)
}

// ── Step-by-step driver ─────────────────────────────────────────────────────

let stepNum = 0

async function processStep(agent: MemoryAgent, step: ScenarioStep): Promise<void> {
  stepNum++
  const speakerColor = step.speaker === 'master' ? C.yellow :
    step.speaker === 'Elio' ? C.cyan :
    C.dim

  console.log('\n' + '█'.repeat(60))
  console.log(C.bold + `Step ${stepNum}/${SCENARIO.length}` + C.reset +
    `  ${speakerColor}[${step.speaker}]${C.reset}`)
  console.log(C.white + step.text.slice(0, 80) + (step.text.length > 80 ? '...' : '') + C.reset)
  console.log('█'.repeat(60))

  // Feed to memory
  if (step.speaker === 'master') {
    agent.captureUserMessage(step.text)
  } else if (step.speaker === 'Elio') {
    agent.captureElioResponse(step.text)
  } else {
    agent.captureWorldview(step.text)
  }

  // Show graph state
  showGraph(agent)

  // Show what the system prompt would get
  showSynthesis()

  // Pause for dramatic effect
  const extra = step.pause ?? 150
  await sleep(800 + extra)
}

// ── Final: Full traversal report ────────────────────────────────────────────

function showTraversal(agent: MemoryAgent): void {
  hr('🔍 全图遍历报告')

  const graph = agent.getGraph()
  const allEdges = graph.getAllEdgesGlobal()

  const dimCounts: Record<string, number> = { TEMPORAL: 0, SEMANTIC: 0, CAUSAL: 0, ENTITY: 0 }
  for (const e of allEdges) {
    dimCounts[e.type] = (dimCounts[e.type] ?? 0) + 1
  }

  console.log()
  const maxCount = Math.max(...Object.values(dimCounts), 1)
  const barWidth = 30
  for (const dim of ['TEMPORAL', 'SEMANTIC', 'CAUSAL', 'ENTITY'] as const) {
    const count = dimCounts[dim]
    const filled = Math.round((count / maxCount) * barWidth)
    const empty = barWidth - filled
    const bar = '█'.repeat(filled) + '░'.repeat(empty)
    console.log(`  ${DIM_ICONS[dim]} ${DIM_COLORS[dim]}${DIM_LABELS[dim]}${C.reset}  ${bar}  ${C.bold}${count}${C.reset}`)
  }

  console.log(C.dim + `\n  事件总数: ${graph.eventCount}    边总数: ${allEdges.length}    关键词: ${agent.getStats().indexKeywords}` + C.reset)
}

// ── Main ────────────────────────────────────────────────────────────────────

async function main() {
  console.log(C.bold + '\n╔══════════════════════════════════════════════╗' + C.reset)
  console.log(C.bold + '║    🧠 Elio 四维记忆图 · 可视化推演           ║' + C.reset)
  console.log(C.bold + '╚══════════════════════════════════════════════╝\n' + C.reset)

  console.log(C.dim + '维度说明:' + C.reset)
  console.log(`  ${DIM_ICONS.TEMPORAL} ${DIM_COLORS.TEMPORAL}时间维${C.reset} — 事件先后顺序`)
  console.log(`  ${DIM_ICONS.SEMANTIC} ${DIM_COLORS.SEMANTIC}语义维${C.reset} — 关键词关联（倒排索引锚定）`)
  console.log(`  ${DIM_ICONS.CAUSAL} ${DIM_COLORS.CAUSAL}因果维${C.reset} — 因果关系链（Slow Path 推理）`)
  console.log(`  ${DIM_ICONS.ENTITY} ${DIM_COLORS.ENTITY}实体维${C.reset} — 实体共指`)
  console.log(`  ${C.dim}(F) = Fast Path 创建  |  (S) = Slow Path 创建${C.reset}`)
  console.log()

  const tmpDir = mkdtempSync(join(tmpdir(), 'elio-memory-viz-'))

  try {
    const agent = new MemoryAgent({ apiKey: 'viz-key', diskDir: tmpDir })
    agent.stop() // Don't start background Slow Path

    // Run scenario step by step
    for (const step of SCENARIO) {
      await processStep(agent, step)
    }

    // Final: full traversal
    showTraversal(agent)

    // Show disk stats
    hr('💾 磁盘持久化')
    const { loadEvents, loadEdges, getDiskStats } = await import('./DiskIO.js')
    const disk = getDiskStats(tmpDir)
    const events = loadEvents(tmpDir)
    const edges = loadEdges(tmpDir)
    console.log(`  events.jsonl:  ${events.length} 条 (${disk.eventsFile})`)
    console.log(`  edges.jsonl:   ${edges.length} 条 (${disk.edgesFile})`)

    agent.stop()
  } finally {
    rmSync(tmpDir, { recursive: true, force: true })
  }

  console.log(C.bold + '\n✅ 可视化推演完成\n' + C.reset)
}

main().catch(err => {
  console.error('Failed:', err)
  process.exit(1)
})
