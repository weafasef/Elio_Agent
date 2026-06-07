/**
 * Upload BriefTool attachments to private_api so web viewers can preview them.
 *
 * Bridge removed — upload always returns undefined.
 * Kept as stub for API compatibility.
 */

import { logForDebugging } from '../../utils/debug.js'

const debug = (msg: string) => logForDebugging(`[brief-upload] ${msg}`)

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
