/**
 * Shimmer animation utilities — moved from bridge/bridgeStatusUtil.ts when bridge was deleted.
 * Used by Spinner.tsx for brief-mode text glow animation.
 */

export const SHIMMER_INTERVAL_MS = 80

export function computeGlimmerIndex(frame: number, width: number): number {
  return frame % width
}

export function computeShimmerSegments(
  text: string,
  glimmerIndex: number,
): { before: string; shimmer: string; after: string } {
  const idx = Math.min(glimmerIndex, text.length - 1)
  return {
    before: text.slice(0, idx),
    shimmer: text.charAt(idx) || '',
    after: text.slice(idx + 1),
  }
}
