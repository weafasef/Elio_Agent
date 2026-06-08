/**
 * Verification script for the memory system.
 * Run: bun run src/elio/memory/verify.ts
 *
 * Tests: event creation → indexing → traversal → synthesis → persistence → cache → retry
 * Slow Path tests require DEEPSEEK_API_KEY env var (or the hardcoded default).
 */

import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { mkdtempSync, rmSync, existsSync, readFileSync } from 'node:fs'
import { MemoryAgent } from './MemoryAgent.js'
import { ContextBridge } from './ContextBridge.js'
import { loadEvents, loadEdges } from './DiskIO.js'

let passed = 0
let failed = 0

function assert(condition: boolean, label: string): void {
  if (condition) {
    console.log(`  ✓ ${label}`)
    passed++
  } else {
    console.log(`  ✗ ${label}`)
    failed++
  }
}

async function main() {
  const tmpDir = mkdtempSync(join(tmpdir(), 'elio-memory-test-'))
  console.log(`Test dir: ${tmpDir}\n`)

  try {
    // ─── 1. Init ─────────────────────────────────────────────────────────
    console.log('── 1. Initialization ──')
    const agent = new MemoryAgent({ apiKey: 'test-key', diskDir: tmpDir })
    assert(agent !== null, 'MemoryAgent created')

    const stats0 = agent.getStats()
    assert(stats0.eventCount === 0, 'Fresh graph has 0 events')
    assert(stats0.edgeCount === 0, 'Fresh graph has 0 edges')
    assert(!ContextBridge.hasContext(), 'ContextBridge initially empty')

    // Don't start SlowPath yet — we'll test it separately
    agent.stop()

    // ─── 2. Event Capture ────────────────────────────────────────────────
    console.log('\n── 2. Event Capture ──')

    // Simulate user message
    const e1 = agent.captureUserMessage('我今天学了Python编程')
    assert(e1.startsWith('E'), `User event created: ${e1}`)
    assert(agent.getStats().eventCount === 1, 'Event count → 1')

    // Simulate worldview
    const e2 = agent.captureWorldview('当前时间: 2026-06-07 14:30:00（下午）\n本次持续运行: 120 分钟')
    assert(e2 !== null, 'Worldview event created')
    assert(agent.getStats().eventCount === 2, 'Event count → 2')

    // Duplicate worldview should be skipped
    const e2b = agent.captureWorldview('当前时间: 2026-06-07 14:30:00（下午）\n本次持续运行: 120 分钟')
    assert(e2b === null, 'Duplicate worldview skipped')

    // Simulate Elio response
    const e3 = agent.captureElioResponse('好的 master，我来帮你整理Python的学习笔记')
    assert(e3.startsWith('E'), `Elio response captured: ${e3}`)
    assert(agent.getStats().eventCount === 3, 'Event count → 3')

    // ─── 3. Graph Structure ──────────────────────────────────────────────
    console.log('\n── 3. Graph Structure ──')

    const graph = agent.getGraph()
    // Should have 2 temporal edges (event1→event2, event2→event3)
    assert(graph.edgeCount >= 2, `Has ≥2 temporal edges: ${graph.edgeCount}`)

    const e1Node = graph.getEvent(e1)!
    assert(e1Node.speaker === 'master', 'Speaker set correctly')
    assert(e1Node.rawText === '我今天学了Python编程', 'Raw text preserved')

    const tempEdges = graph.getOutgoingEdges(e1, 'TEMPORAL')
    assert(tempEdges.length >= 1, `Event E1 has outgoing temporal edges: ${tempEdges.length}`)

    // ─── 4. Inverted Index ──────────────────────────────────────────────
    console.log('\n── 4. Inverted Index ──')

    const stats = agent.getStats()
    assert(stats.indexKeywords > 0, `Index has keywords: ${stats.indexKeywords}`)
    console.log(`    Keywords: ${stats.indexKeywords}`)

    // Manual search test
    // (The index is internal to MemoryAgent, but FastPath uses it)

    // ─── 5. ContextBridge ───────────────────────────────────────────────
    console.log('\n── 5. ContextBridge ──')

    assert(ContextBridge.hasContext(), 'ContextBridge has content after events')
    const ctx = ContextBridge.get()
    assert(ctx.includes('相关记忆'), 'Context contains 相关记忆 header')
    assert(ctx.includes('Python'), 'Context mentions Python')
    console.log(`    Context: ${ctx.slice(0, 100)}...`)

    const age = ContextBridge.getAge()
    assert(age !== null && age.ageMs >= 0, 'ContextBridge age is valid')

    // ─── 6. Slow Path Queue ─────────────────────────────────────────────
    console.log('\n── 6. Slow Path Queue ──')

    // User + Elio messages should be enqueued (2 items)
    assert(agent.getStats().slowPathQueue === 2,
      `SlowPath queue length = 2: ${agent.getStats().slowPathQueue}`)

    // ─── 7. Slow Path LLM Cache ─────────────────────────────────────────
    console.log('\n── 7. Slow Path Cache ──')

    const slowPath = agent['slowPath'] // Access private for testing
    const testPrompt = '测试缓存：返回JSON {"test": true}'

    // Mock a quick LLM response for cache test
    const origCallLLM = slowPath['config'].callLLM
    let callCount = 0
    slowPath['config'].callLLM = async (p: string) => {
      callCount++
      return '{"test": true}'
    }

    const r1 = await slowPath['cachedCallLLM'](testPrompt)
    const r2 = await slowPath['cachedCallLLM'](testPrompt)
    const r3 = await slowPath['cachedCallLLM']('different prompt')

    assert(callCount === 2, `Cache hit: 3 calls, only 2 API invocations (was ${callCount})`)
    assert(r1 === r2, 'Cache returns same response for same prompt')
    assert(r1 === '{"test": true}', 'Cache response correct')

    slowPath['config'].callLLM = origCallLLM // Restore

    // ─── 8. Persistence Round-trip ──────────────────────────────────────
    console.log('\n── 8. Persistence ──')

    agent.save()
    const eventsFile = join(tmpDir, 'events.jsonl')
    const edgesFile = join(tmpDir, 'edges.jsonl')
    assert(existsSync(eventsFile), 'Events file written')
    assert(existsSync(edgesFile), 'Edges file written')

    // Verify content
    const loadedEvents = loadEvents(tmpDir)
    assert(loadedEvents.length === 3, `Reloaded ${loadedEvents.length} events (expected 3)`)
    const loadedEdges = loadEdges(tmpDir)
    assert(loadedEdges.length >= 2, `Reloaded ${loadedEdges.length} edges (expected ≥2)`)

    // ─── 9. Fast Path Performance ───────────────────────────────────────
    console.log('\n── 9. Fast Path Performance ──')

    const start = performance.now()
    for (let i = 0; i < 10; i++) {
      agent.captureUserMessage(`性能测试消息 ${i}`)
    }
    const elapsed = performance.now() - start
    const avgMs = elapsed / 10
    assert(avgMs < 2, `10 events avg ${avgMs.toFixed(2)}ms/event (< 2ms target)`)
    console.log(`    Average: ${avgMs.toFixed(2)}ms`)

    // ─── 10. Retry Logic ────────────────────────────────────────────────
    console.log('\n── 10. Retry Logic ──')

    const retryAgent = new MemoryAgent({ apiKey: 'retry-test-key', diskDir: join(tmpDir, 'retry-test') })
    retryAgent.stop()

    // Access SlowPath internals to test retry
    const retrySp = retryAgent['slowPath']
    let retryCalls = 0
    retrySp['config'].callLLM = async () => {
      retryCalls++
      throw new Error('Simulated failure')
    }
    retrySp['config'].maxRetries = 2

    const testId = retryAgent.captureUserMessage('retry test message')
    assert(retryCalls === 0, 'Retry not called yet')

    // Force tick → should fail once, retry count → 1, re-queue
    await retrySp.forceTick()
    assert(retryCalls === 1, 'First attempt called')
    assert(retrySp['retryCount'].get(testId) === 1, 'Retry count = 1')

    // Force tick again → should fail again, retry count → 2, NOT re-queued (max=2)
    await retrySp.forceTick()
    assert(retryCalls === 2, 'Second attempt called')
    assert(retrySp['retryCount'].has(testId) === false, 'Retry count cleared (max reached)')
    assert(retrySp['processed'].has(testId), 'Marked as processed (exhausted retries)')
    assert(retrySp.queueLength === 0, 'Queue empty after max retries')

    console.log(`    Retry calls: ${retryCalls} (expected 2)`)

    retryAgent.stop()

  } finally {
    // Cleanup
    rmSync(tmpDir, { recursive: true, force: true })
    console.log(`\nCleaned up: ${tmpDir}`)
  }

  // ─── Summary ────────────────────────────────────────────────────────
  console.log(`\n${'═'.repeat(40)}`)
  console.log(`  Passed: ${passed}  |  Failed: ${failed}`)
  console.log(`${'═'.repeat(40)}`)

  if (failed > 0) {
    console.log('\n⚠ Some tests failed. Check the output above.')
    process.exit(1)
  } else {
    console.log('\n✓ All tests passed. Memory system is working correctly.')
  }
}

main().catch(err => {
  console.error('Verification failed:', err)
  process.exit(1)
})
