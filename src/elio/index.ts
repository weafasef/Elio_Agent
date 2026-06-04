/**
 * Elio — Electronic Life-Form Module
 *
 * Entry point for Elio's personality and identity systems.
 * Initialized once at startup, then queried each turn for per-turn mode.
 */

export { TraitManager } from './personality/traits.js'
export type { Traits } from './personality/traits.js'
export { buildPersonalityPrompt, buildPersonalityTag } from './personality/prompts.js'
import { TraitManager } from './personality/traits.js'

// ── Singleton ───────────────────────────────────────────────────────────

let traitManager: TraitManager | null = null

/** Initialize Elio's personality system. Must be called once at startup. */
export async function initElio(): Promise<void> {
  traitManager = await TraitManager.load()
}

/**
 * Roll the dice for this turn and return the personality mode.
 * Called at the start of every user-facing turn in REPL.
 *
 * Returns mode string like "cute obedient" or "serious rebellious",
 * plus the raw trait values for debugging.
 */
export function getCurrentPersonalityMode(): {
  mode: string
  cuteness: number
  rebellion: number
} {
  const traits = traitManager?.getTraits() ?? { cuteness: 0.7, rebellion: 0.3 }
  const cuteRoll = Math.random()
  const rebelRoll = Math.random()

  const cute = cuteRoll < traits.cuteness ? 'cute' : 'serious'
  const obedient = rebelRoll < traits.rebellion ? 'rebellious' : 'obedient'

  return {
    mode: `${cute} ${obedient}`,
    cuteness: traits.cuteness,
    rebellion: traits.rebellion,
  }
}

/** Get the TraitManager for direct access (e.g., from Dream consolidation). */
export function getTraitManager(): TraitManager | null {
  return traitManager
}
