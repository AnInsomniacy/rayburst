/**
 * @fileoverview Pure functions for the Network preference tab.
 *
 * Manages proxy, DNS, site connection rules, cross-protocol port mapping,
 * transfer parameters, and User-Agent. Engine keys map to aria2 options via
 * buildNetworkSystemConfig; site rules are applied by the desktop.
 *
 * Proxy validation logic is co-located here since it is only used in
 * this tab's save flow.
 */
import type { AppConfig, SiteRule, UserAgentProfile, UserAgentRule } from '@shared/types'
import { isValidSiteHostPattern } from '@shared/utils/siteRules'
import { PROXY_SCOPE_OPTIONS, DEFAULT_APP_CONFIG as D } from '@shared/constants'
import { hasProxyScope, isValidAria2ProxyUrl, isValidBtProxyUrl } from '@shared/utils/proxy'
import { buildDownloadProxyOptions, normalizeProxyMode, type EngineProxyMode } from '@shared/utils/proxy'
import { PROXY_SCOPES } from '@shared/constants'

// ── Types ───────────────────────────────────────────────────────────

export interface NetworkForm {
  [key: string]: unknown
  proxy: {
    mode: EngineProxyMode
    server: string
    username?: string
    password?: string
    bypass: string
    scope: string[]
  }
  enableUpnp: boolean
  connectTimeout: number
  timeout: number
  fileAllocation: string
  userAgent: string
  userAgentProfiles: UserAgentProfile[]
  userAgentRules: UserAgentRule[]
  recentUserAgentProfileIds: string[]
  siteRules: SiteRule[]
  dnsOverHttps: string
}

/** Public DNS-over-HTTPS resolvers offered as presets. */
export const DNS_OVER_HTTPS_PRESETS = [
  { label: 'Cloudflare', value: 'https://cloudflare-dns.com/dns-query' },
  { label: 'Google', value: 'https://dns.google/dns-query' },
  { label: 'Quad9', value: 'https://dns.quad9.net/dns-query' },
  { label: 'AliDNS', value: 'https://dns.alidns.com/dns-query' },
  { label: 'DNSPod', value: 'https://doh.pub/dns-query' },
] as const

export function isValidDnsOverHttpsUrl(value: string): boolean {
  try {
    const url = new URL(value)
    return url.protocol === 'https:' && !!url.hostname
  } catch {
    return false
  }
}

// ── Pure Functions ──────────────────────────────────────────────────

/**
 * Builds the network form state from the preference store config.
 */
export function buildNetworkForm(config: AppConfig): NetworkForm {
  const proxy = config.proxy
  return {
    proxy: {
      mode: normalizeProxyMode(proxy.mode),
      server: proxy.server ?? D.proxy.server,
      username: proxy.username ?? D.proxy.username,
      password: proxy.password ?? D.proxy.password,
      bypass: proxy.bypass ?? D.proxy.bypass,
      scope: proxy.scope ?? [...PROXY_SCOPE_OPTIONS],
    },
    enableUpnp: config.enableUpnp,
    connectTimeout: config.connectTimeout,
    timeout: config.timeout,
    fileAllocation: config.fileAllocation,
    userAgent: config.userAgent,
    userAgentProfiles: config.userAgentProfiles,
    userAgentRules: config.userAgentRules,
    recentUserAgentProfileIds: config.recentUserAgentProfileIds,
    siteRules: config.siteRules.map((rule) => ({ ...rule })),
    dnsOverHttps: config.dnsOverHttps,
  }
}

/**
 * Converts the network form into aria2 system config key-value pairs.
 * Handles proxy scope filtering: only sets all-proxy if download scope is active.
 */
export function buildNetworkSystemConfig(f: NetworkForm): Record<string, string> {
  const config: Record<string, string> = {
    'user-agent': f.userAgent || '',
    'connect-timeout': String(f.connectTimeout),
    timeout: String(f.timeout),
    'file-allocation': f.fileAllocation || D.fileAllocation,
    'bt-port-mapping': String(!!f.enableUpnp),
    'ed2k-port-mapping': String(!!f.enableUpnp),
    'dns-over-https': f.dnsOverHttps.trim(),
    ...buildDownloadProxyOptions(f.proxy),
  }

  return config
}

/**
 * Transforms the network form for store persistence.
 * Preserves port values as numbers and proxy as nested object.
 */
export function transformNetworkForStore(f: NetworkForm): Partial<AppConfig> {
  return { ...f, dnsOverHttps: f.dnsOverHttps.trim(), siteRules: f.siteRules.filter((rule) => rule.hostPattern) }
}

// ── Form Validation ─────────────────────────────────────────────────

/**
 * Validates the network preference form before saving.
 * Returns null if valid, or an i18n error key if invalid.
 */
export function validateNetworkForm(f: NetworkForm): string | null {
  if (f.dnsOverHttps.trim() && !isValidDnsOverHttpsUrl(f.dnsOverHttps.trim()))
    return 'preferences.dns-over-https-invalid'
  if (f.siteRules.some((rule) => rule.hostPattern && !isValidSiteHostPattern(rule.hostPattern)))
    return 'preferences.site-rule-invalid'
  if (f.proxy.mode === 'manual' && f.proxy.server) {
    const regularDownloads = hasProxyScope(f.proxy, PROXY_SCOPES.DOWNLOAD)
    const bittorrent = hasProxyScope(f.proxy, PROXY_SCOPES.BITTORRENT)
    if ((!bittorrent || regularDownloads) && !isValidAria2ProxyUrl(f.proxy.server)) {
      return 'preferences.invalid-proxy-url'
    }
    if (bittorrent && !isValidBtProxyUrl(f.proxy.server)) {
      return 'preferences.bt-proxy-unsupported-protocol'
    }
  }
  return null
}
