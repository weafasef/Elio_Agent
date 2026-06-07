// ── Memory scan stub — replaced by dual-agent graph memory system ────

export type MemoryHeader = {
  relativePath: string
  name: string
  description: string
}

export async function scanMemoryFiles(): Promise<MemoryHeader[]> {
  return []
}

export function formatMemoryManifest(_memories: MemoryHeader[]): string {
  return ''
}
