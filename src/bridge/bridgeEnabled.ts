// ── Bridge stub — remote-control feature removed ────

export function isBridgeEnabled(): boolean {
  return false
}

export function getBridgeDisabledReason(): string {
  return 'Bridge removed — server-only mode'
}

export function checkBridgeMinVersion(): string | null {
  return 'Bridge removed — server-only mode'
}

export function isEnvLessBridgeEnabled(): boolean {
  return false
}
