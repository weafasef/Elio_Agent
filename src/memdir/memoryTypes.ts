// ── Memory types stub — replaced by dual-agent graph memory system ────

export const MEMORY_TYPES = [
  'user',
  'feedback',
  'project',
  'reference',
  'relationship',
  'emotional',
] as const

export type MemoryType = (typeof MEMORY_TYPES)[number]

export function parseMemoryType(_raw: unknown): MemoryType | undefined {
  return undefined
}

export const TYPES_SECTION_COMBINED: readonly string[] = []
export const TYPES_SECTION_INDIVIDUAL: readonly string[] = []
export const WHAT_NOT_TO_SAVE_SECTION: readonly string[] = []
export const MEMORY_DRIFT_CAVEAT = ''
export const WHEN_TO_ACCESS_SECTION: readonly string[] = []
export const TRUSTING_RECALL_SECTION: readonly string[] = []
export const MEMORY_FRONTMATTER_EXAMPLE: readonly string[] = []
