/**
 * Shimmer animation utilities — moved from bridge/bridgeStatusUtil.ts when bridge was deleted.
 * Used by Spinner.tsx for brief-mode text glow animation.
 */

export const SHIMMER_INTERVAL_MS = 150

export function computeGlimmerIndex(tick: number, messageWidth: number): number {
  const cycleLength = messageWidth + 20
  return messageWidth + 10 - (tick % cycleLength)
}

export function computeShimmerSegments(
  text: string,
  glimmerIndex: number,
): { before: string; shimmer: string; after: string } {
  const idx = Math.max(0, Math.min(glimmerIndex, text.length - 1))
  return {
    before: text.slice(0, idx),
    shimmer: text.charAt(idx) || '',
    after: text.slice(idx + 1),
  }
}
