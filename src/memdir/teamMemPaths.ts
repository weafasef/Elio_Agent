// ── Team memory stub — replaced by dual-agent graph memory system ────

export class PathTraversalError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'PathTraversalError'
  }
}

export function isTeamMemoryEnabled(): boolean {
  return false
}

export function getTeamMemPath(): string {
  return ''
}

export function getTeamMemEntrypoint(): string {
  return ''
}

export function isTeamMemPath(_filePath: string): boolean {
  return false
}

export async function validateTeamMemWritePath(): Promise<string> {
  return ''
}

export async function validateTeamMemKey(_relativeKey: string): Promise<string> {
  return ''
}

export function isTeamMemFile(_filePath: string): boolean {
  return false
}
