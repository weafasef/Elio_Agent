/**
 * JWT utility — moved from bridge/jwtUtils.ts when bridge was deleted.
 * Used by ccrClient.ts for auth token expiry checks.
 */

export function decodeJwtExpiry(token: string): Date | null {
  try {
    const parts = token.split('.')
    if (parts.length !== 3) return null
    const payload = JSON.parse(
      Buffer.from(parts[1]!, 'base64url').toString('utf8'),
    )
    return typeof payload.exp === 'number' ? new Date(payload.exp * 1000) : null
  } catch {
    return null
  }
}
