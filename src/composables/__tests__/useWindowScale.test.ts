/** @fileoverview Regression coverage for accessible text scaling in content-sized windows. */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, h, provide, reactive, type ComputedRef } from 'vue'
import { SYSTEM_WINDOW_SCALE, useWindowScale } from '../useWindowScale'

const bridge = vi.hoisted(() => ({ innerSize: vi.fn(), scaleFactor: vi.fn(), onScaleChanged: vi.fn() }))
const stopScaleEvents = vi.fn()
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => bridge }))
vi.mock('@shared/logger', () => ({ logger: { debug: vi.fn() } }))
vi.mock('@/stores/preference', () => ({ usePreferenceStore: () => ({ config }) }))

const config = reactive({ uiScale: 1 })
const cleanups: (() => void)[] = []

function setup(systemScale = 1): ComputedRef<number> {
  let scale: ComputedRef<number> | undefined
  const Content = defineComponent({
    setup() {
      scale = useWindowScale()
      return () => null
    },
  })
  const host = mount(
    defineComponent({
      setup() {
        provide(SYSTEM_WINDOW_SCALE, systemScale)
        return () => h(Content)
      },
    }),
  )
  cleanups.push(() => host.unmount())
  if (!scale) throw new Error('Window scale was not initialized')
  return scale
}

async function resize() {
  window.dispatchEvent(new Event('resize'))
  await vi.advanceTimersByTimeAsync(20)
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame'] })
  vi.clearAllMocks()
  config.uiScale = 1
  vi.stubGlobal('innerWidth', 0)
  bridge.scaleFactor.mockResolvedValue(1)
  bridge.innerSize.mockResolvedValue({ width: 632, height: 560 })
  bridge.onScaleChanged.mockResolvedValue(stopScaleEvents)
})

afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('content window scale', () => {
  it('includes GNOME large text before the hidden window has a measurable viewport', async () => {
    const scale = setup(1.25)
    await vi.advanceTimersByTimeAsync(20)
    expect(scale.value).toBe(1.25)
    expect(Math.ceil(632 * scale.value)).toBe(790)
    expect(Math.ceil(68 * scale.value)).toBe(85)
    expect(bridge.innerSize).not.toHaveBeenCalled()
  })

  it('combines system text scaling with interface zoom exactly once', async () => {
    config.uiScale = 1.1
    const scale = setup(1.25)
    expect(scale.value).toBeCloseTo(1.375)
    config.uiScale = 1.5
    await flushPromises()
    expect(scale.value).toBe(1.875)
  })

  it('follows system scaling changes through the native and CSS viewport sizes', async () => {
    const scale = setup()
    vi.stubGlobal('innerWidth', 506)
    await resize()
    expect(scale.value).toBeCloseTo(1.25, 2)
    vi.stubGlobal('innerWidth', 632)
    await resize()
    expect(scale.value).toBe(1)
  })

  it('uses the fractional viewport width to avoid truncating scaled content', async () => {
    const scale = setup()
    vi.stubGlobal('innerWidth', 506)
    vi.stubGlobal('visualViewport', { width: 505.6 })
    await resize()
    expect(scale.value).toBe(1.25)
  })

  it('removes monitor DPI from physical pixels without doubling the interface zoom', async () => {
    config.uiScale = 1.25
    const scale = setup(1.25)
    bridge.scaleFactor.mockResolvedValue(2)
    bridge.innerSize.mockResolvedValue({ width: 790, height: 700 })
    vi.stubGlobal('innerWidth', 316)
    await resize()
    expect(scale.value).toBe(1.25)
    config.uiScale = 1.5
    await flushPromises()
    expect(scale.value).toBe(1.5)
  })

  it('ignores viewport rounding when fitting a small floating orb', async () => {
    config.uiScale = 1.1
    const scale = setup(1.25)
    bridge.innerSize.mockResolvedValue({ width: 94, height: 94 })
    vi.stubGlobal('innerWidth', 68)
    await resize()
    expect(scale.value).toBe(1.375)
  })

  it('keeps the known scale when the native measurement fails', async () => {
    const scale = setup(1.25)
    bridge.innerSize.mockRejectedValue(new Error('window unavailable'))
    vi.stubGlobal('innerWidth', 506)
    await resize()
    expect(scale.value).toBe(1.25)
  })

  it('discards a measurement whose viewport changed during IPC', async () => {
    const scale = setup(1.25)
    let resolveSize: ((size: { width: number; height: number }) => void) | undefined
    bridge.innerSize.mockReturnValue(
      new Promise<{ width: number; height: number }>((resolve) => (resolveSize = resolve)),
    )
    vi.stubGlobal('innerWidth', 506)
    await resize()
    vi.stubGlobal('innerWidth', 632)
    resolveSize?.({ width: 632, height: 560 })
    await flushPromises()
    expect(scale.value).toBe(1.25)
  })

  it('ignores measurements completed after unmount and releases resize listeners', async () => {
    const scale = setup(1.25)
    let resolveSize: ((size: { width: number; height: number }) => void) | undefined
    bridge.innerSize.mockReturnValue(
      new Promise<{ width: number; height: number }>((resolve) => (resolveSize = resolve)),
    )
    vi.stubGlobal('innerWidth', 316)
    await resize()
    for (const cleanup of cleanups.splice(0)) cleanup()
    resolveSize?.({ width: 632, height: 560 })
    await flushPromises()
    expect(scale.value).toBe(1.25)
    await resize()
    expect(bridge.innerSize).toHaveBeenCalledTimes(1)
    expect(stopScaleEvents).toHaveBeenCalledTimes(1)
  })
})
