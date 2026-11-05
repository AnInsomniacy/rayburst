import { describe, expect, it } from 'vitest'
import { PROXY_SCOPES } from '@shared/constants'
import {
  buildDownloadProxyOptions,
  buildTaskProxyOptions,
  getDefaultTaskProxyMode,
  getDownloadProxy,
  normalizeProxyMode,
  proxySwitchValueToMode,
  resolveAppProxyUrl,
} from '@shared/utils/proxy'

describe('proxyPolicy', () => {
  it('rejects unsupported proxy modes', () => {
    expect(normalizeProxyMode('auto')).toBe('direct')
  })

  it('follows the system proxy when switched on without a server', () => {
    expect(proxySwitchValueToMode(true)).toBe('system')
    expect(proxySwitchValueToMode(true, 'http://127.0.0.1:7890')).toBe('manual')
    expect(proxySwitchValueToMode(false, 'http://127.0.0.1:7890')).toBe('direct')
  })

  it('leaves the system proxy to the backend', () => {
    const proxy = { mode: 'system' as const, server: 'http://stale:1', scope: ['download', 'update-app'] }
    expect(normalizeProxyMode('system')).toBe('system')
    expect(buildDownloadProxyOptions(proxy)['all-proxy']).toBe('')
    expect(buildTaskProxyOptions('system', '', proxy)).toEqual({})
    expect(buildTaskProxyOptions('manual', '', proxy)['all-proxy']).toBe('')
    expect(getDefaultTaskProxyMode(proxy)).toBe('system')
    expect(getDownloadProxy(proxy)).toBe('system')
    expect(resolveAppProxyUrl(proxy, PROXY_SCOPES.UPDATE_APP)).toBe('system')
    expect(resolveAppProxyUrl(proxy, PROXY_SCOPES.UPDATE_TRACKERS)).toBeNull()
  })

  it('emits scoped HTTP download options', () => {
    expect(
      buildDownloadProxyOptions({
        mode: 'manual',
        server: 'http://127.0.0.1:7890',
        bypass: 'localhost',
        scope: ['download'],
      }),
    ).toEqual({
      'all-proxy': 'http://127.0.0.1:7890',
      'bt-proxy': '',
      'no-proxy': 'localhost',
    })
  })

  it('embeds credentials in the BitTorrent proxy URI', () => {
    expect(
      buildDownloadProxyOptions({
        mode: 'manual',
        server: 'socks5://127.0.0.1:1080',
        username: 'user',
        password: 'pass',
        bypass: '',
        scope: [PROXY_SCOPES.BITTORRENT],
      })['bt-proxy'],
    ).toBe('socks5://user:pass@127.0.0.1:1080')
  })

  it('clears standard aria2 proxy keys for direct task mode', () => {
    expect(buildTaskProxyOptions('direct', '')).toEqual({
      'all-proxy': '',
      'all-proxy-user': '',
      'all-proxy-passwd': '',
      'http-proxy': '',
      'http-proxy-user': '',
      'http-proxy-passwd': '',
      'https-proxy': '',
      'https-proxy-user': '',
      'https-proxy-passwd': '',
      'no-proxy': '',
    })
  })

  it('defaults task proxy mode to direct when download scope is excluded', () => {
    expect(
      getDefaultTaskProxyMode({
        mode: 'manual',
        server: 'http://127.0.0.1:7890',
        scope: ['update-app'],
      }),
    ).toBe('direct')
  })

  it('defaults task proxy mode to manual when download scope is included', () => {
    expect(
      getDefaultTaskProxyMode({
        mode: 'manual',
        server: '',
        scope: ['download'],
      }),
    ).toBe('manual')
  })
})

describe('resolveAppProxyUrl', () => {
  it('ignores stale proxy fields when mode is direct', () => {
    expect(
      resolveAppProxyUrl(
        {
          mode: 'direct',
          server: 'Test123',
          username: 'Test123',
          password: 'Test1234',
          scope: [PROXY_SCOPES.UPDATE_TRACKERS],
        },
        PROXY_SCOPES.UPDATE_TRACKERS,
      ),
    ).toBeNull()
  })
})
