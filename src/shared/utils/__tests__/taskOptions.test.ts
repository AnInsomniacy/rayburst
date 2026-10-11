import { describe, expect, it } from 'vitest'
import { checksumOption, parseChecksum } from '../checksum'
import { normalizeSiteRules, isValidSiteHostPattern, MAX_SITE_RULES } from '../siteRules'
import { formatSpeedLimit, parseSpeedLimit } from '../speedLimit'

describe('speed limits', () => {
  it('reads engine values in the largest whole unit', () => {
    expect(parseSpeedLimit('0')).toEqual({ amount: 0, unit: 'K' })
    expect(parseSpeedLimit('')).toEqual({ amount: 0, unit: 'K' })
    expect(parseSpeedLimit('512K')).toEqual({ amount: 512, unit: 'K' })
    expect(parseSpeedLimit('2M')).toEqual({ amount: 2, unit: 'M' })
    expect(parseSpeedLimit(1048576)).toEqual({ amount: 1, unit: 'M' })
    expect(parseSpeedLimit('1536K')).toEqual({ amount: 1536, unit: 'K' })
    expect(parseSpeedLimit('1G')).toEqual({ amount: 1024, unit: 'M' })
    expect(parseSpeedLimit('fast')).toEqual({ amount: 0, unit: 'K' })
  })

  it('formats whole amounts and treats zero as unlimited', () => {
    expect(formatSpeedLimit(512, 'K')).toBe('512K')
    expect(formatSpeedLimit(2.7, 'M')).toBe('2M')
    expect(formatSpeedLimit(0, 'M')).toBe('0')
    expect(formatSpeedLimit(-3, 'K')).toBe('0')
  })
})

describe('checksums', () => {
  const sha256 = 'a'.repeat(64)

  it('infers the algorithm from the digest length', () => {
    expect(parseChecksum('d41d8cd98f00b204e9800998ecf8427e')).toEqual({
      algorithm: 'md5',
      digest: 'd41d8cd98f00b204e9800998ecf8427e',
    })
    expect(parseChecksum(` ${sha256.toUpperCase()} `)?.algorithm).toBe('sha-256')
  })

  it('accepts an algorithm prefix and rejects mismatched lengths', () => {
    expect(parseChecksum(`SHA256: ${sha256}`)?.algorithm).toBe('sha-256')
    expect(parseChecksum(`sha-1=${sha256}`)).toBeNull()
    expect(parseChecksum('crc32=deadbeef')).toBeNull()
    expect(parseChecksum('not a digest')).toBeNull()
    expect(checksumOption({ algorithm: 'sha-256', digest: sha256 })).toBe(`sha-256=${sha256}`)
  })
})

describe('site rules', () => {
  it('validates host patterns', () => {
    expect(isValidSiteHostPattern('example.com')).toBe(true)
    expect(isValidSiteHostPattern('*.cdn.example.com')).toBe(true)
    expect(isValidSiteHostPattern('example..com')).toBe(false)
    expect(isValidSiteHostPattern('https://example.com')).toBe(false)
    expect(isValidSiteHostPattern('')).toBe(false)
  })

  it('repairs persisted rules', () => {
    const rules = normalizeSiteRules([
      { id: 'a', hostPattern: ' Example.COM ', maxConnectionPerServer: 40 },
      { id: 'a', hostPattern: 'duplicate.com', maxConnectionPerServer: 2 },
      { id: 'b', enabled: false, hostPattern: 'bad host', maxConnectionPerServer: 2 },
      { id: 'c', enabled: false, hostPattern: 'files.org', maxConnectionPerServer: 0, proxy: 'proxy' },
      { id: 'd', hostPattern: 'cdn.test', maxConnectionPerServer: -3, proxy: 'tunnel' },
      null,
    ])
    expect(rules).toEqual([
      { id: 'a', enabled: true, hostPattern: 'example.com', maxConnectionPerServer: 16, proxy: 'default' },
      { id: 'c', enabled: false, hostPattern: 'files.org', maxConnectionPerServer: 0, proxy: 'proxy' },
      { id: 'd', enabled: true, hostPattern: 'cdn.test', maxConnectionPerServer: 0, proxy: 'default' },
    ])
    expect(normalizeSiteRules('nope')).toEqual([])
    const many = Array.from({ length: MAX_SITE_RULES + 5 }, (_, i) => ({
      id: String(i),
      hostPattern: 'example.com',
      maxConnectionPerServer: 2,
    }))
    expect(normalizeSiteRules(many)).toHaveLength(MAX_SITE_RULES)
  })
})
