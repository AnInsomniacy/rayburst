/**
 * @fileoverview Tests for the useSystemProxyDetect composable.
 *
 * Key behaviors under test:
 * - `detect()` invokes `get_system_proxy` Tauri command
 * - `detecting` ref is true while detection is in-progress
 * - `detecting` ref resets to false after completion (success, not-found, socks, error)
 * - Calls `onSuccess` with SystemProxyInfo when a valid HTTP proxy is detected
 * - Calls `onNotFound` when invoke returns null or empty server
 * - Calls `onError` when invoke rejects with an exception
 * - Only one callback fires per detection (mutual exclusivity)
 * - Concurrent detect() calls are serialized (second call is no-op while detecting)
 * - Minimum loading duration is enforced via DETECT_MIN_DURATION
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { nextTick } from 'vue'
import type { SystemProxyInfo } from '@shared/types'

// ── Mock Tauri Core ──────────────────────────────────────────────────
const mockInvoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

import { useSystemProxyDetect } from '../useSystemProxyDetect'
import { DETECT_MIN_DURATION } from '@shared/timing'

// ── Helpers ──────────────────────────────────────────────────────────

function makeProxyInfo(overrides: Partial<SystemProxyInfo> = {}): SystemProxyInfo {
  return {
    server: 'http://127.0.0.1:7890',
    bypass: '*.local',
    unsupportedBypass: [],
    isSocks: false,
    ...overrides,
  }
}

function makeCallbacks() {
  return {
    onSuccess: vi.fn(),
    onNotFound: vi.fn(),
    onError: vi.fn(),
  }
}

/** Start detect() and advance fake timers so the minimum-duration delay resolves. */
async function detectAndFlush(detect: () => Promise<void>): Promise<void> {
  const promise = detect()
  await vi.advanceTimersByTimeAsync(DETECT_MIN_DURATION)
  await promise
}

describe('useSystemProxyDetect', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  // ── Invocation ────────────────────────────────────────────────────

  it('invokes the get_system_proxy Tauri command', async () => {
    mockInvoke.mockResolvedValue(makeProxyInfo())
    const cbs = makeCallbacks()
    const { detect } = useSystemProxyDetect(cbs)

    await detectAndFlush(detect)

    expect(mockInvoke).toHaveBeenCalledWith('get_system_proxy')
    expect(mockInvoke).toHaveBeenCalledTimes(1)
  })

  // ── Loading state ─────────────────────────────────────────────────

  it('sets detecting to true while detection is in-progress', async () => {
    let resolveInvoke!: (v: SystemProxyInfo) => void
    mockInvoke.mockReturnValue(new Promise<SystemProxyInfo>((r) => (resolveInvoke = r)))
    const cbs = makeCallbacks()
    const { detect, detecting } = useSystemProxyDetect(cbs)

    expect(detecting.value).toBe(false)

    const promise = detect()
    await nextTick()
    expect(detecting.value).toBe(true)

    resolveInvoke(makeProxyInfo())
    await vi.advanceTimersByTimeAsync(DETECT_MIN_DURATION)
    await promise
    expect(detecting.value).toBe(false)
  })

  it('resets detecting to false after onNotFound path', async () => {
    mockInvoke.mockResolvedValue(null)
    const cbs = makeCallbacks()
    const { detect, detecting } = useSystemProxyDetect(cbs)

    await detectAndFlush(detect)

    expect(detecting.value).toBe(false)
  })

  it('resets detecting to false after onError path', async () => {
    mockInvoke.mockRejectedValue(new Error('IPC failure'))
    const cbs = makeCallbacks()
    const { detect, detecting } = useSystemProxyDetect(cbs)

    await detectAndFlush(detect)

    expect(detecting.value).toBe(false)
  })

  // ── Minimum duration ──────────────────────────────────────────────

  it('keeps detecting true for at least DETECT_MIN_DURATION even if IPC resolves instantly', async () => {
    mockInvoke.mockResolvedValue(makeProxyInfo())
    const cbs = makeCallbacks()
    const { detect, detecting } = useSystemProxyDetect(cbs)

    const promise = detect()
    await nextTick()
    // IPC resolved immediately, but delay still pending
    expect(detecting.value).toBe(true)
    expect(cbs.onSuccess).not.toHaveBeenCalled()

    // Advance past the minimum duration
    await vi.advanceTimersByTimeAsync(DETECT_MIN_DURATION)
    await promise
    expect(detecting.value).toBe(false)
    expect(cbs.onSuccess).toHaveBeenCalledTimes(1)
  })

  // ── Success path ──────────────────────────────────────────────────

  it('calls onSuccess with SystemProxyInfo for valid HTTP proxy', async () => {
    const info = makeProxyInfo({ server: 'http://10.0.0.1:8080', bypass: '192.168.*' })
    mockInvoke.mockResolvedValue(info)
    const cbs = makeCallbacks()
    const { detect } = useSystemProxyDetect(cbs)

    await detectAndFlush(detect)

    expect(cbs.onSuccess).toHaveBeenCalledTimes(1)
    expect(cbs.onSuccess).toHaveBeenCalledWith(info)
    // No other callbacks fired
    expect(cbs.onNotFound).not.toHaveBeenCalled()
    expect(cbs.onError).not.toHaveBeenCalled()
  })

  // ── SOCKS rejection path ──────────────────────────────────────────

  it('accepts a SOCKS system proxy like any other', async () => {
    mockInvoke.mockResolvedValue(makeProxyInfo({ server: 'socks5://127.0.0.1:1080', isSocks: true }))
    const cbs = makeCallbacks()
    const { detect } = useSystemProxyDetect(cbs)

    await detectAndFlush(detect)

    expect(cbs.onSuccess).toHaveBeenCalledTimes(1)
    expect(cbs.onNotFound).not.toHaveBeenCalled()
    expect(cbs.onError).not.toHaveBeenCalled()
  })

  // ── Not found path ────────────────────────────────────────────────

  it('calls onNotFound when invoke returns null', async () => {
    mockInvoke.mockResolvedValue(null)
    const cbs = makeCallbacks()
    const { detect } = useSystemProxyDetect(cbs)

    await detectAndFlush(detect)

    expect(cbs.onNotFound).toHaveBeenCalledTimes(1)
    expect(cbs.onSuccess).not.toHaveBeenCalled()
    expect(cbs.onError).not.toHaveBeenCalled()
  })

  it('calls onNotFound when invoke returns info with empty server', async () => {
    mockInvoke.mockResolvedValue(makeProxyInfo({ server: '' }))
    const cbs = makeCallbacks()
    const { detect } = useSystemProxyDetect(cbs)

    await detectAndFlush(detect)

    expect(cbs.onNotFound).toHaveBeenCalledTimes(1)
    expect(cbs.onSuccess).not.toHaveBeenCalled()
  })

  it('calls onNotFound when invoke returns info with whitespace-only server', async () => {
    mockInvoke.mockResolvedValue(makeProxyInfo({ server: '   ' }))
    const cbs = makeCallbacks()
    const { detect } = useSystemProxyDetect(cbs)

    await detectAndFlush(detect)

    expect(cbs.onNotFound).toHaveBeenCalledTimes(1)
  })

  // ── Error path ────────────────────────────────────────────────────

  it('calls onError when invoke rejects', async () => {
    const err = new Error('IPC failure')
    mockInvoke.mockRejectedValue(err)
    const cbs = makeCallbacks()
    const { detect } = useSystemProxyDetect(cbs)

    await detectAndFlush(detect)

    expect(cbs.onError).toHaveBeenCalledTimes(1)
    expect(cbs.onError).toHaveBeenCalledWith(err)
    expect(cbs.onSuccess).not.toHaveBeenCalled()
    expect(cbs.onNotFound).not.toHaveBeenCalled()
  })

  // ── Concurrency guard ─────────────────────────────────────────────

  it('ignores concurrent detect() calls while one is in-progress', async () => {
    let resolveInvoke!: (v: SystemProxyInfo | null) => void
    mockInvoke.mockReturnValue(new Promise((r) => (resolveInvoke = r)))
    const cbs = makeCallbacks()
    const { detect, detecting } = useSystemProxyDetect(cbs)

    const first = detect()
    await nextTick()
    expect(detecting.value).toBe(true)

    // Second call while first is pending — should be no-op
    const second = detect()
    await nextTick()

    // Still only one invoke call
    expect(mockInvoke).toHaveBeenCalledTimes(1)

    resolveInvoke(makeProxyInfo())
    await vi.advanceTimersByTimeAsync(DETECT_MIN_DURATION)
    await first
    await second

    expect(cbs.onSuccess).toHaveBeenCalledTimes(1)
  })
})
