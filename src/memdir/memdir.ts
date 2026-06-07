// ── Auto-memory stub — replaced by dual-agent graph memory system ────

export const ENTRYPOINT_NAME = 'MEMORY.md'
export const MAX_ENTRYPOINT_LINES = 200
export const MAX_ENTRYPOINT_BYTES = 25_000

export type EntrypointTruncation = {
  content: string
  truncated: boolean
  originalLength: number
}

export function truncateEntrypointContent(raw: string): EntrypointTruncation {
  return { content: '', truncated: false, originalLength: 0 }
}

export const DIR_EXISTS_GUIDANCE = ''
export const DIRS_EXIST_GUIDANCE = ''

export async function ensureMemoryDirExists(_memoryDir: string): Promise<void> {}

export function buildMemoryLines(): string[] {
  return []
}

export function buildMemoryPrompt(): string {
  return ''
}

export function buildSearchingPastContextSection(_autoMemDir: string): string[] {
  return []
}

export async function loadMemoryPrompt(): Promise<string | null> {
  return null
}
