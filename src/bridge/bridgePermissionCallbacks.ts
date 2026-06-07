// ── Bridge stub — remote-control feature removed ────

export type BridgePermissionCallbacks = Record<string, unknown>
export type BridgePermissionResponse = Record<string, unknown>

export function isBridgePermissionResponse(_msg: unknown): boolean {
  return false
}
