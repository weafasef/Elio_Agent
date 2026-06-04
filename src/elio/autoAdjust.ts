/**
 * Elio Personality Auto-Adjustment Service
 *
 * Scans memory files for [TRAIT_ADJUST] markers written by the extraction
 * subagent, applies the adjustments via TraitManager, and marks them as
 * processed to prevent double-application.
 *
 * Called after each successful extractMemories run.
 */

import { readFile, writeFile, readdir } from 'fs/promises'
import { join, extname } from 'path'
import { getTraitManager } from './index.js'

// ── Regex ───────────────────────────────────────────────────────────────

/** Matches: [TRAIT_ADJUST] cuteness +0.05 — reason here */
const TRAIT_ADJUST_RE = /\[TRAIT_ADJUST\]\s+(cuteness|rebellion)\s+([+-]\d+\.?\d*)\s*[—\-]\s*(.+)/i

/** Already-processed marker */
const TRAIT_ADJUSTED_MARKER = '[TRAIT_ADJUSTED]'

// ── Types ───────────────────────────────────────────────────────────────

interface PendingAdjustment {
  trait: 'cuteness' | 'rebellion'
  delta: number
  reason: string
}

// ── Core logic ──────────────────────────────────────────────────────────

/**
 * Scan a single file for unprocessed [TRAIT_ADJUST] markers.
 * Returns parsed adjustments and the updated file content (with markers replaced).
 */
async function scanFile(filePath: string): Promise<{
  adjustments: PendingAdjustment[]
  modified: boolean
  newContent: string
}> {
  let content: string
  try {
    content = await readFile(filePath, 'utf-8')
  } catch {
    return { adjustments: [], modified: false, newContent: '' }
  }

  // Skip files that have no unprocessed markers
  if (!content.includes('[TRAIT_ADJUST]')) {
    return { adjustments: [], modified: false, newContent: content }
  }

  const adjustments: PendingAdjustment[] = []
  let modified = false

  // Process each line
  const lines = content.split('\n')
  const newLines = lines.map(line => {
    // Skip already-processed lines
    if (line.includes(TRAIT_ADJUSTED_MARKER)) return line
    // Skip if no adjust marker on this line
    if (!line.includes('[TRAIT_ADJUST]')) return line

    const match = line.match(TRAIT_ADJUST_RE)
    if (!match) return line

    const [, trait, deltaStr, reason] = match
    const delta = parseFloat(deltaStr)

    // Validate
    if (isNaN(delta) || Math.abs(delta) > 0.2) {
      // Suspiciously large delta — skip
      console.error(`[Elio:autoAdjust] Skipping suspicious delta: ${deltaStr} in ${filePath}`)
      return line.replace('[TRAIT_ADJUST]', TRAIT_ADJUSTED_MARKER)
    }

    adjustments.push({
      trait: trait as 'cuteness' | 'rebellion',
      delta,
      reason: reason.trim(),
    })

    modified = true
    // Replace the marker to prevent re-application
    return line.replace('[TRAIT_ADJUST]', TRAIT_ADJUSTED_MARKER)
  })

  return {
    adjustments,
    modified,
    newContent: newLines.join('\n'),
  }
}

/**
 * Scan all memory files for unprocessed trait adjustments and apply them.
 *
 * @param memoryDir - Path to the memory directory (e.g., ~/.elio/memory/)
 * @returns Number of adjustments applied
 */
export async function scanAndApplyAdjustments(memoryDir: string): Promise<number> {
  const tm = getTraitManager()
  if (!tm) {
    console.error('[Elio:autoAdjust] TraitManager not initialized — skipping')
    return 0
  }

  // List all .md files in memory directory
  let entries: string[]
  try {
    entries = await readdir(memoryDir)
  } catch {
    // Directory doesn't exist yet — nothing to scan
    return 0
  }

  const mdFiles = entries.filter(f => extname(f) === '.md')
  if (mdFiles.length === 0) return 0

  let totalApplied = 0

  for (const file of mdFiles) {
    const filePath = join(memoryDir, file)
    const { adjustments, modified, newContent } = await scanFile(filePath)

    // Apply each adjustment
    for (const adj of adjustments) {
      try {
        await tm.adjust(adj.trait, adj.delta, adj.reason)
        console.error(
          `[Elio:autoAdjust] Applied: ${adj.trait} ${adj.delta >= 0 ? '+' : ''}${adj.delta.toFixed(2)} — ${adj.reason}`,
        )
        totalApplied++
      } catch (err) {
        console.error(`[Elio:autoAdjust] Failed to apply adjustment:`, err)
      }
    }

    // Write back the file with processed markers
    if (modified) {
      try {
        await writeFile(filePath, newContent)
      } catch (err) {
        console.error(`[Elio:autoAdjust] Failed to write processed file ${file}:`, err)
      }
    }
  }

  return totalApplied
}
