/** @fileoverview Speed limit values as the engine accepts and reports them. */

export type SpeedLimitUnit = 'K' | 'M'

/**
 * Reads `512K`, `2M` or a plain byte count (what the engine reports) as an
 * amount in KiB/s or MiB/s. Empty and zero mean unlimited.
 */
export function parseSpeedLimit(value: string | number | undefined): { amount: number; unit: SpeedLimitUnit } {
  const text = String(value ?? '')
    .trim()
    .toUpperCase()
  const match = /^(\d+(?:\.\d+)?)\s*([KMG]?)B?$/.exec(text)
  if (!match) return { amount: 0, unit: 'K' }
  const number = Number(match[1])
  const bytes = number * { '': 1, K: 1024, M: 1024 ** 2, G: 1024 ** 3 }[match[2] as '' | 'K' | 'M' | 'G']
  if (!bytes) return { amount: 0, unit: match[2] === 'M' ? 'M' : 'K' }
  if (bytes >= 1024 ** 2 && bytes % 1024 ** 2 === 0) return { amount: bytes / 1024 ** 2, unit: 'M' }
  return { amount: Math.max(1, Math.round(bytes / 1024)), unit: 'K' }
}

/** Formats an amount for the engine; `0` is unlimited. */
export function formatSpeedLimit(amount: number, unit: SpeedLimitUnit): string {
  const whole = Math.max(0, Math.floor(amount))
  return whole > 0 ? `${whole}${unit}` : '0'
}
