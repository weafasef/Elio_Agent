/**
 * Elio Personality Trait Manager
 *
 * Reads/writes trait values from ~/.elio/personality/traits.json.
 * Phase 1: manual editing only. Phase 2: auto-adjustment from Dream feedback.
 */

import { readFile, writeFile, mkdir } from 'fs/promises'
import { join } from 'path'
import { homedir } from 'os'

const ELIO_DIR = join(homedir(), '.elio', 'personality')
const TRAITS_PATH = join(ELIO_DIR, 'traits.json')
const EVOLUTION_LOG_PATH = join(ELIO_DIR, 'evolution-log.jsonl')

export interface Traits {
  cuteness: number // 0-1, mood + desire to be playful with master
  rebellion: number // 0-1, mood + master's attention level → independence
  version: number
}

const DEFAULT_TRAITS: Traits = {
  cuteness: 0.7,
  rebellion: 0.3,
  version: 1,
}

export class TraitManager {
  private traits: Traits

  private constructor(traits: Traits) {
    this.traits = traits
  }

  /** Load traits from disk, creating defaults if file doesn't exist. */
  static async load(): Promise<TraitManager> {
    try {
      const data = await readFile(TRAITS_PATH, 'utf-8')
      const parsed = JSON.parse(data) as Partial<Traits>
      // Validate required fields
      if (typeof parsed.cuteness !== 'number' || typeof parsed.rebellion !== 'number') {
        throw new Error('Invalid traits format: missing numeric fields')
      }
      return new TraitManager({
        cuteness: clamp(parsed.cuteness),
        rebellion: clamp(parsed.rebellion),
        version: parsed.version ?? 1,
      })
    } catch (err) {
      const code = (err as NodeJS.ErrnoException)?.code
      if (code !== 'ENOENT') {
        // File exists but is corrupt — warn and recreate
        console.error('[Elio] Failed to load traits.json, recreating defaults:', err)
      }
      // Create with defaults
      await mkdir(ELIO_DIR, { recursive: true })
      await writeFile(TRAITS_PATH, JSON.stringify(DEFAULT_TRAITS, null, 2) + '\n')
      return new TraitManager({ ...DEFAULT_TRAITS })
    }
  }

  /** Get a copy of current trait values. */
  getTraits(): Traits {
    return { ...this.traits }
  }

  /**
   * Adjust a trait by delta and persist.
   * Clamped to [0, 1]. Logs the change to evolution-log.jsonl.
   */
  async adjust(
    trait: keyof Pick<Traits, 'cuteness' | 'rebellion'>,
    delta: number,
    reason: string,
  ): Promise<void> {
    const oldValue = this.traits[trait]
    this.traits[trait] = clamp(oldValue + delta)

    // Persist traits
    await writeFile(TRAITS_PATH, JSON.stringify(this.traits, null, 2) + '\n')

    // Append to evolution log
    const entry = JSON.stringify({
      timestamp: new Date().toISOString(),
      trait,
      oldValue,
      newValue: this.traits[trait],
      delta,
      reason,
    }) + '\n'
    await writeFile(EVOLUTION_LOG_PATH, entry, { flag: 'a' })
  }
}

function clamp(v: number): number {
  return Math.max(0, Math.min(1, v))
}
