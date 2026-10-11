/** @fileoverview Per-site connection limits and proxy choice; the desktop applies them when it creates a download. */
import type { SiteRule, SiteRuleProxy } from '@shared/types'

export const MAX_SITE_RULES = 200
export const SITE_RULE_CONNECTION_RANGE = { min: 1, max: 16 } as const
export const SITE_RULE_PROXY_MODES: readonly SiteRuleProxy[] = ['default', 'proxy', 'direct']

/** `example.com` names the domain and its subdomains; `*` matches any characters. */
export function isValidSiteHostPattern(pattern: string): boolean {
  return /^[a-z0-9*]([a-z0-9*.-]*[a-z0-9*])?$/i.test(pattern) && !pattern.includes('..')
}

export function normalizeSiteRules(value: unknown): SiteRule[] {
  if (!Array.isArray(value)) return []
  const seen = new Set<string>()
  const rules: SiteRule[] = []
  for (const item of value) {
    if (typeof item !== 'object' || item === null) continue
    const record = item as Record<string, unknown>
    const id = typeof record.id === 'string' ? record.id.trim() : ''
    const hostPattern = typeof record.hostPattern === 'string' ? record.hostPattern.trim().toLowerCase() : ''
    const connections = Number(record.maxConnectionPerServer ?? 0)
    if (!id || seen.has(id) || !isValidSiteHostPattern(hostPattern) || !Number.isInteger(connections)) continue
    seen.add(id)
    rules.push({
      id,
      enabled: typeof record.enabled === 'boolean' ? record.enabled : true,
      hostPattern,
      maxConnectionPerServer:
        connections <= 0
          ? 0
          : Math.min(SITE_RULE_CONNECTION_RANGE.max, Math.max(SITE_RULE_CONNECTION_RANGE.min, connections)),
      proxy: SITE_RULE_PROXY_MODES.includes(record.proxy as SiteRuleProxy)
        ? (record.proxy as SiteRuleProxy)
        : 'default',
    })
    if (rules.length >= MAX_SITE_RULES) break
  }
  return rules
}
