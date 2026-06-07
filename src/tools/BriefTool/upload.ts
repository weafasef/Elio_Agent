/**
 * Upload BriefTool attachments — bridge removed, always returns undefined.
 * Kept as stub for API compatibility.
 */

import { logForDebugging } from '../../utils/debug.js'

function debug(msg: string): void {
  logForDebugging(`[brief:upload] ${msg}`)
}

export type BriefUploadContext = {
  replBridgeEnabled: boolean
  signal?: AbortSignal
}

export async function uploadBriefAttachment(
  _fullPath: string,
  _size: number,
  _ctx: BriefUploadContext,
): Promise<string | undefined> {
  return undefined
}
