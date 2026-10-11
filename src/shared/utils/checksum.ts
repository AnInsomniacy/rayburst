/** @fileoverview Expected file digests as users paste them and as the engine verifies them. */

const ALGORITHMS = {
  md5: 32,
  'sha-1': 40,
  'sha-224': 56,
  'sha-256': 64,
  'sha-384': 96,
  'sha-512': 128,
} as const

export type ChecksumAlgorithm = keyof typeof ALGORITHMS

export interface Checksum {
  algorithm: ChecksumAlgorithm
  digest: string
}

function normalizeAlgorithm(name: string): ChecksumAlgorithm | undefined {
  const compact = name.toLowerCase().replace(/[^a-z0-9]/g, '')
  return (Object.keys(ALGORITHMS) as ChecksumAlgorithm[]).find((algorithm) => algorithm.replace('-', '') === compact)
}

/**
 * Reads a hex digest, optionally prefixed with its algorithm (`sha-256=…`,
 * `SHA256: …`). Without a prefix the length names the algorithm.
 */
export function parseChecksum(text: string): Checksum | null {
  const trimmed = text.trim()
  const prefixed = /^([A-Za-z0-9-]+)\s*[=:]\s*([0-9A-Fa-f]+)$/.exec(trimmed)
  const digest = (prefixed ? prefixed[2] : trimmed).toLowerCase()
  if (!/^[0-9a-f]+$/.test(digest)) return null
  const algorithm = prefixed
    ? normalizeAlgorithm(prefixed[1])
    : (Object.keys(ALGORITHMS) as ChecksumAlgorithm[]).find((name) => ALGORITHMS[name] === digest.length)
  if (!algorithm || ALGORITHMS[algorithm] !== digest.length) return null
  return { algorithm, digest }
}

/** The engine's `checksum` option value. */
export function checksumOption(checksum: Checksum): string {
  return `${checksum.algorithm}=${checksum.digest}`
}
