// ── Bridge stub — remote-control feature removed ────

export const BRIDGE_FAILURE_DISMISS_MS = 10_000

const MAX_CONSECUTIVE_INIT_FAILURES = 3

export function useReplBridge(
  _messages: unknown,
  _setMessages: unknown,
  _abortControllerRef: unknown,
  _commands: unknown,
  _mainLoopModel: string,
): {
  sendBridgeResult: () => void
} {
  return { sendBridgeResult: () => {} }
}
