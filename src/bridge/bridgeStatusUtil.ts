// ── Bridge stub — remote-control feature removed ────

export const SHIMMER_INTERVAL_MS = 0

export function getBridgeStatus(): string {
  return 'off'
}

export function buildBridgeConnectUrl(): null {
  return null
}

export function buildActiveFooterText(): string {
  return ''
}

export function buildIdleFooterText(): string {
  return ''
}

export const FAILED_FOOTER_TEXT = ''

export function computeGlimmerIndex(): number {
  return 0
}

export function computeShimmerSegments(): Array<{ start: number; end: number }> {
  return []
}
