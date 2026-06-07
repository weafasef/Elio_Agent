// ── Memory search stub — replaced by dual-agent graph memory system ────

export type RelevantMemory = {
  relativePath: string
  name: string
  description: string
  content: string
}

export async function findRelevantMemories(): Promise<RelevantMemory[]> {
  return []
}
