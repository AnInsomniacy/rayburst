/**
 * @fileoverview Integration tests for the useTaskDetailOptions composable.
 *
 * Covers:
 * - Loading per-task options via getTaskOption
 * - ProxyMode detection and aria2-compatible proxy option output
 * - Multi-field dirty tracking
 * - Bulk apply via changeTaskOption with correct aria2 option keys
 * - Toast differentiation (active vs paused)
 * - Edge cases: stopped tasks, engine down, RPC failure, concurrent calls
 *
 * Pure header utility tests live in taskDetailOptionsParse.test.ts.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'
import { ref, nextTick, type Ref } from 'vue'
import { useTaskDetailOptions } from '@/composables/useTaskDetailOptions'
import type { Aria2Task, ProxyConfig } from '@shared/types'
import { TASK_STATUS } from '@shared/constants'

// ── Mock modules ────────────────────────────────────────────────────

vi.mock('@/api/aria2', () => ({
  isEngineReady: vi.fn(() => true),
}))

const { isEngineReady } = await import('@/api/aria2')

// ── Factory helpers ─────────────────────────────────────────────────

function makeTask(overrides: Partial<Aria2Task> = {}): Aria2Task {
  return {
    gid: 'abc123',
    status: TASK_STATUS.ACTIVE as Aria2Task['status'],
    totalLength: '1000000',
    completedLength: '500000',
    uploadLength: '0',
    downloadSpeed: '10000',
    uploadSpeed: '0',
    connections: '5',
    dir: '/downloads',
    files: [],
    ...overrides,
  }
}

function makeProxy(overrides: Partial<ProxyConfig> = {}): ProxyConfig {
  return {
    mode: 'manual',
    server: 'http://127.0.0.1:7890',
    scope: ['download', 'update-app'],
    ...overrides,
  }
}

interface MockDeps {
  task: Ref<Aria2Task | null>
  getTaskOption: Mock
  changeTaskOption: Mock
  replaceTaskUri: Mock
  retryTaskWithUri: Mock
  proxyConfig: ProxyConfig
  successFn: Mock
  errorFn: Mock
  t: (key: string) => string
}

function createMocks(overrides: Partial<MockDeps> = {}): MockDeps {
  return {
    task: ref<Aria2Task | null>(makeTask()),
    getTaskOption: vi.fn().mockResolvedValue({ userAgent: '', referer: '', proxyMode: 'direct' }),
    changeTaskOption: vi.fn().mockResolvedValue(undefined),
    replaceTaskUri: vi.fn().mockResolvedValue(undefined),
    retryTaskWithUri: vi.fn().mockResolvedValue(undefined),
    proxyConfig: makeProxy(),
    successFn: vi.fn(),
    errorFn: vi.fn(),
    t: (key: string) => key,
    ...overrides,
  }
}

function setup(mocks: MockDeps) {
  return useTaskDetailOptions({
    task: mocks.task,
    getTaskOption: mocks.getTaskOption,
    changeTaskOption: mocks.changeTaskOption,
    replaceTaskUri: mocks.replaceTaskUri,
    retryTaskWithUri: mocks.retryTaskWithUri,
    proxyConfig: () => mocks.proxyConfig,
    message: { success: mocks.successFn, error: mocks.errorFn },
    t: mocks.t,
  })
}

// ── Tests ───────────────────────────────────────────────────────────

describe('useTaskDetailOptions', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ;(isEngineReady as Mock).mockReturnValue(true)
  })

  describe('canModify', () => {
    it.each([
      ['active', TASK_STATUS.ACTIVE, true],
      ['waiting', TASK_STATUS.WAITING, true],
      ['paused', TASK_STATUS.PAUSED, true],
      ['complete', TASK_STATUS.COMPLETE, false],
      ['error', TASK_STATUS.ERROR, false],
      ['removed', TASK_STATUS.REMOVED, false],
    ])('returns %s for %s tasks', (_, status, expected) => {
      const m = createMocks({ task: ref(makeTask({ status: status as Aria2Task['status'] })) })
      expect(setup(m).canModify.value).toBe(expected)
    })

    it('returns false when engine is not ready', () => {
      ;(isEngineReady as Mock).mockReturnValue(false)
      expect(setup(createMocks()).canModify.value).toBe(false)
    })

    it('returns false when task is null', () => {
      expect(setup(createMocks({ task: ref(null) })).canModify.value).toBe(false)
    })
  })

  describe('loadOptions', () => {
    it('calls getTaskOption on mount with task gid', async () => {
      const mocks = createMocks()
      setup(mocks)
      await nextTick()
      expect(mocks.getTaskOption).toHaveBeenCalledWith('abc123')
    })

    it('populates form fields from response', async () => {
      const mocks = createMocks({
        getTaskOption: vi.fn().mockResolvedValue({
          userAgent: 'Custom UA',
          referer: 'https://example.com',
          allProxy: 'http://127.0.0.1:7890',
          header: 'Cookie: session=abc',
        }),
      })
      const { form } = setup(mocks)
      await nextTick()
      expect(form.userAgent).toBe('Custom UA')
      expect(form.referer).toBe('https://example.com')
      expect(form.cookie).toBe('session=abc')
    })

    it('does not call getTaskOption for null task', async () => {
      const mocks = createMocks({ task: ref(null) })
      setup(mocks)
      await nextTick()
      expect(mocks.getTaskOption).not.toHaveBeenCalled()
    })

    it('does not call getTaskOption for completed tasks', async () => {
      const mocks = createMocks({
        task: ref(makeTask({ status: TASK_STATUS.COMPLETE as Aria2Task['status'] })),
      })
      setup(mocks)
      await nextTick()
      expect(mocks.getTaskOption).not.toHaveBeenCalled()
    })

    it('reloads when task gid changes', async () => {
      const mocks = createMocks()
      setup(mocks)
      await nextTick()
      expect(mocks.getTaskOption).toHaveBeenCalledTimes(1)

      mocks.task.value = makeTask({ gid: 'xyz789' })
      await nextTick()
      expect(mocks.getTaskOption).toHaveBeenCalledTimes(2)
      expect(mocks.getTaskOption).toHaveBeenLastCalledWith('xyz789')
    })

    it('handles getTaskOption failure gracefully', async () => {
      const mocks = createMocks({
        getTaskOption: vi.fn().mockRejectedValue(new Error('RPC error')),
      })
      const { form } = setup(mocks)
      await nextTick()
      expect(form.userAgent).toBe('')
      expect(form.proxyMode).toBe('direct')
    })

    it('parses header array with Cookie and Authorization', async () => {
      const mocks = createMocks({
        getTaskOption: vi.fn().mockResolvedValue({
          header: ['Cookie: sid=abc', 'Authorization: Bearer tok'],
        }),
      })
      const { form } = setup(mocks)
      await nextTick()
      expect(form.cookie).toBe('sid=abc')
      expect(form.authorization).toBe('Bearer tok')
    })
  })

  describe('proxyMode detection', () => {
    it('sets proxyMode to direct when no allProxy is present', async () => {
      const mocks = createMocks({
        getTaskOption: vi.fn().mockResolvedValue({}),
      })
      const { form } = setup(mocks)
      await nextTick()
      expect(form.proxyMode).toBe('direct')
    })

    it('sets proxyMode to manual when allProxy is present', async () => {
      const mocks = createMocks({
        getTaskOption: vi.fn().mockResolvedValue({
          allProxy: 'http://127.0.0.1:7890',
          allProxyUser: 'proxy-user',
          allProxyPasswd: 'proxy-pass',
        }),
      })
      const { form } = setup(mocks)
      await nextTick()
      expect(form.proxyMode).toBe('manual')
      expect(form.customProxy).toBe('http://127.0.0.1:7890')
      expect(form.customProxyUsername).toBe('proxy-user')
      expect(form.customProxyPassword).toBe('proxy-pass')
    })
  })

  describe('dirty tracking', () => {
    it('dirty is false after initial load', async () => {
      const { dirty } = setup(createMocks())
      await nextTick()
      expect(dirty.value).toBe(false)
    })

    it.each([
      ['userAgent', 'new UA'],
      ['referer', 'https://new.com'],
      ['cookie', 'new=1'],
      ['authorization', 'Bearer new'],
      ['httpAuthUsername', 'demo'],
      ['httpAuthPassword', 'secret'],
      ['customProxyUsername', 'proxy-user'],
      ['customProxyPassword', 'proxy-pass'],
    ] as const)('dirty becomes true when %s changes', async (field, value) => {
      const { form, dirty } = setup(createMocks())
      await nextTick()
      ;(form as Record<string, string>)[field] = value
      expect(dirty.value).toBe(true)
    })

    it('dirty becomes true when proxyMode changes', async () => {
      const { form, dirty } = setup(createMocks())
      await nextTick()
      form.proxyMode = 'manual'
      expect(dirty.value).toBe(true)
    })

    it('dirty becomes true when customProxy changes', async () => {
      const mocks = createMocks({
        getTaskOption: vi.fn().mockResolvedValue({ proxyMode: 'manual', allProxy: 'http://old:1080' }),
      })
      const { form, dirty } = setup(mocks)
      await nextTick()
      form.customProxy = 'http://new:1080'
      expect(dirty.value).toBe(true)
    })

    it('dirty becomes false when reverted', async () => {
      const mocks = createMocks({
        getTaskOption: vi.fn().mockResolvedValue({ userAgent: 'orig' }),
      })
      const { form, dirty } = setup(mocks)
      await nextTick()
      form.userAgent = 'changed'
      expect(dirty.value).toBe(true)
      form.userAgent = 'orig'
      expect(dirty.value).toBe(false)
    })
  })

  describe('applyOptions', () => {
    it('sends changed user-agent', async () => {
      const mocks = createMocks()
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.userAgent = 'New UA'
      await applyOptions()
      expect(mocks.changeTaskOption).toHaveBeenCalledWith({
        gid: 'abc123',
        options: expect.objectContaining({ 'user-agent': 'New UA' }),
      })
    })

    it('sends changed referer', async () => {
      const mocks = createMocks()
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.referer = 'https://example.com'
      await applyOptions()
      expect(mocks.changeTaskOption).toHaveBeenCalledWith({
        gid: 'abc123',
        options: expect.objectContaining({ referer: 'https://example.com' }),
      })
    })

    it('sends manual proxy when proxyMode is manual', async () => {
      const mocks = createMocks()
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.proxyMode = 'manual'
      form.customProxy = 'http://10.0.0.1:8080'
      form.customProxyUsername = 'proxy-user'
      form.customProxyPassword = 'proxy-pass'
      await applyOptions()
      expect(mocks.changeTaskOption).toHaveBeenCalledWith({
        gid: 'abc123',
        options: expect.objectContaining({
          'all-proxy': 'http://10.0.0.1:8080',
          'all-proxy-user': 'proxy-user',
          'all-proxy-passwd': 'proxy-pass',
        }),
      })
      expect(mocks.changeTaskOption.mock.calls[0][0].options['proxy-mode']).toBeUndefined()
    })

    it('blocks an unsupported custom proxy with error toast', async () => {
      const mocks = createMocks()
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.proxyMode = 'manual'
      form.customProxy = 'ftp://10.0.0.1:21'
      await applyOptions()
      expect(mocks.changeTaskOption).not.toHaveBeenCalled()
      expect(mocks.errorFn).toHaveBeenCalledWith('preferences.invalid-proxy-url')
    })

    it('keeps dirty after a proxy rejection', async () => {
      const mocks = createMocks()
      const { form, applyOptions, dirty } = setup(mocks)
      await nextTick()
      form.proxyMode = 'manual'
      form.customProxy = 'ftp://10.0.0.1:21'
      await applyOptions()
      expect(dirty.value).toBe(true)
    })

    it('applies a SOCKS5 custom proxy', async () => {
      const mocks = createMocks()
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.proxyMode = 'manual'
      form.customProxy = 'socks5://10.0.0.1:1080'
      await applyOptions()
      expect(mocks.changeTaskOption).toHaveBeenCalled()
    })

    it('sends direct proxy mode when proxyMode is direct', async () => {
      const mocks = createMocks({
        getTaskOption: vi.fn().mockResolvedValue({ proxyMode: 'manual', allProxy: 'http://127.0.0.1:7890' }),
      })
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.proxyMode = 'direct'
      await applyOptions()
      expect(mocks.changeTaskOption).toHaveBeenCalledWith({
        gid: 'abc123',
        options: expect.objectContaining({
          'all-proxy': '',
          'all-proxy-user': '',
          'all-proxy-passwd': '',
        }),
      })
      expect(mocks.changeTaskOption.mock.calls[0][0].options['proxy-mode']).toBeUndefined()
    })

    it('sends header array when cookie/auth change', async () => {
      const mocks = createMocks()
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.cookie = 'session=abc'
      form.authorization = 'Bearer xyz'
      await applyOptions()
      expect(mocks.changeTaskOption).toHaveBeenCalledWith({
        gid: 'abc123',
        options: expect.objectContaining({
          header: ['Cookie: session=abc', 'Authorization: Bearer xyz'],
        }),
      })
    })

    it('sends aria2 HTTP auth options when Basic Auth changes', async () => {
      const mocks = createMocks()
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.httpAuthUsername = ' demo '
      form.httpAuthPassword = ' secret '
      await applyOptions()
      expect(mocks.changeTaskOption).toHaveBeenCalledWith({
        gid: 'abc123',
        options: expect.objectContaining({
          'http-user': 'demo',
          'http-passwd': 'secret',
        }),
      })
    })

    it('shows restart toast for active tasks', async () => {
      const mocks = createMocks()
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.userAgent = 'new'
      await applyOptions()
      expect(mocks.successFn).toHaveBeenCalledWith('task.options-applied-restart')
    })

    it('shows simple toast for paused tasks', async () => {
      const mocks = createMocks({
        task: ref(makeTask({ status: TASK_STATUS.PAUSED as Aria2Task['status'] })),
      })
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.userAgent = 'new'
      await applyOptions()
      expect(mocks.successFn).toHaveBeenCalledWith('task.options-applied')
    })

    it('shows error toast on RPC failure', async () => {
      const mocks = createMocks({
        changeTaskOption: vi.fn().mockRejectedValue(new Error('fail')),
      })
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.userAgent = 'new'
      await applyOptions()
      expect(mocks.errorFn).toHaveBeenCalledWith('task.options-apply-failed')
    })

    it('resets dirty after successful apply', async () => {
      const mocks = createMocks()
      const { form, applyOptions, dirty } = setup(mocks)
      await nextTick()
      form.userAgent = 'new'
      expect(dirty.value).toBe(true)
      await applyOptions()
      expect(dirty.value).toBe(false)
    })

    it('keeps dirty after failed apply', async () => {
      const mocks = createMocks({
        changeTaskOption: vi.fn().mockRejectedValue(new Error('fail')),
      })
      const { form, applyOptions, dirty } = setup(mocks)
      await nextTick()
      form.userAgent = 'new'
      await applyOptions()
      expect(dirty.value).toBe(true)
    })

    it('sets applying during RPC call', async () => {
      let resolve!: () => void
      const mocks = createMocks({
        changeTaskOption: vi.fn().mockReturnValue(new Promise<void>((r) => (resolve = r))),
      })
      const { form, applyOptions, applying } = setup(mocks)
      await nextTick()
      form.userAgent = 'new'
      const p = applyOptions()
      expect(applying.value).toBe(true)
      resolve()
      await p
      expect(applying.value).toBe(false)
    })

    it('prevents concurrent apply calls', async () => {
      let resolve!: () => void
      const mocks = createMocks({
        changeTaskOption: vi.fn().mockReturnValue(new Promise<void>((r) => (resolve = r))),
      })
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.userAgent = 'new'
      const p1 = applyOptions()
      const p2 = applyOptions()
      resolve()
      await p1
      await p2
      expect(mocks.changeTaskOption).toHaveBeenCalledTimes(1)
    })

    it('does not call changeTaskOption when not dirty', async () => {
      const mocks = createMocks()
      const { applyOptions } = setup(mocks)
      await nextTick()
      await applyOptions()
      expect(mocks.changeTaskOption).not.toHaveBeenCalled()
    })

    it('does not call changeTaskOption when task is null', async () => {
      const mocks = createMocks({ task: ref(null) })
      const { applyOptions } = setup(mocks)
      await applyOptions()
      expect(mocks.changeTaskOption).not.toHaveBeenCalled()
    })
  })

  describe('speed limits and sources', () => {
    const sourced = (status: string) =>
      makeTask({
        status: status as Aria2Task['status'],
        files: [
          {
            index: '1',
            path: '/downloads/file.zip',
            length: '1000',
            completedLength: '500',
            selected: 'true',
            uris: [{ uri: 'https://old.example/file.zip', status: 'used' }],
          },
        ],
      })

    it('applies a speed limit without announcing a restart', async () => {
      const mocks = createMocks({
        getTaskOption: vi.fn().mockResolvedValue({ maxDownloadLimit: '1048576' }),
      })
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      await nextTick()
      expect(form.maxDownloadLimit).toBe('1M')
      form.maxDownloadLimit = '512K'
      await applyOptions()
      expect(mocks.changeTaskOption).toHaveBeenCalledWith({ gid: 'abc123', options: { 'max-download-limit': '512K' } })
      expect(mocks.successFn).toHaveBeenCalledWith('task.options-applied')
    })

    it('replaces the source of an unfinished download', async () => {
      const mocks = createMocks({ task: ref(sourced(TASK_STATUS.PAUSED)) })
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      expect(form.uri).toBe('https://old.example/file.zip')
      form.uri = 'https://new.example/file.zip'
      await applyOptions()
      expect(mocks.replaceTaskUri).toHaveBeenCalledWith({ gid: 'abc123', uri: 'https://new.example/file.zip' })
      expect(mocks.changeTaskOption).not.toHaveBeenCalled()
    })

    it('retries a failed download from the new source', async () => {
      const task = sourced(TASK_STATUS.ERROR)
      const mocks = createMocks({ task: ref(task) })
      const { form, retriesWithUri, applyOptions } = setup(mocks)
      form.uri = 'https://new.example/file.zip'
      expect(retriesWithUri.value).toBe(true)
      await applyOptions()
      expect(mocks.retryTaskWithUri).toHaveBeenCalledWith(task, 'https://new.example/file.zip')
      expect(mocks.replaceTaskUri).not.toHaveBeenCalled()
    })

    it('rejects a source the engine cannot replace', async () => {
      const mocks = createMocks({ task: ref(sourced(TASK_STATUS.PAUSED)) })
      const { form, applyOptions } = setup(mocks)
      await nextTick()
      form.uri = 'magnet:?xt=urn:btih:abc'
      await applyOptions()
      expect(mocks.errorFn).toHaveBeenCalledWith('task.source-url-invalid')
      expect(mocks.replaceTaskUri).not.toHaveBeenCalled()
    })
  })
})
