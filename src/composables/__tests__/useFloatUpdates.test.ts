/** @fileoverview Regression coverage for floating-window snapshot publication. */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, ref, shallowRef } from 'vue'
import { useFloatUpdates, type FloatUpdate } from '../useFloatUpdates'

const bridge = vi.hoisted(() => ({ listen: vi.fn(), invoke: vi.fn(), unlisten: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: bridge.invoke }))
vi.mock('@tauri-apps/api/webviewWindow', () => ({ getCurrentWebviewWindow: () => ({ listen: bridge.listen }) }))

let receive: (event: { payload: FloatUpdate }) => void
const cleanups: (() => void)[] = []

function snapshot(speed = 0): FloatUpdate {
  return { stat: { downloadSpeed: speed, uploadSpeed: 0, numActive: speed > 0 ? 1 : 0 }, progress: null, tasks: [] }
}

function setup(enabled = ref(true)) {
  const onUpdate = vi.fn()
  const state = { update: shallowRef<FloatUpdate | null>(null) }
  const wrapper = mount(
    defineComponent({
      setup() {
        state.update = useFloatUpdates({ onUpdate, enabled: () => enabled.value }).update
        return () => null
      },
    }),
  )
  cleanups.push(() => wrapper.unmount())
  return { update: state.update, wrapper, onUpdate }
}

beforeEach(() => {
  vi.clearAllMocks()
  bridge.invoke.mockResolvedValue(undefined)
  bridge.listen.mockImplementation((_name: string, handler: typeof receive) => {
    receive = handler
    return Promise.resolve(bridge.unlisten)
  })
})
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup()
})

describe('floating-window updates', () => {
  it('keeps the snapshot identity and skips callbacks when data has not changed', async () => {
    const { update, onUpdate } = setup()
    await flushPromises()
    const first = snapshot(1024)
    receive({ payload: first })
    receive({ payload: snapshot(1024) })
    expect(update.value).toBe(first)
    expect(onUpdate).toHaveBeenCalledTimes(1)
    receive({ payload: snapshot(2048) })
    expect(update.value?.stat.downloadSpeed).toBe(2048)
    expect(onUpdate).toHaveBeenCalledTimes(2)
    expect(bridge.invoke).toHaveBeenCalledWith('float_window_ready')
  })

  it('publishes only the latest hidden snapshot when the panel opens', async () => {
    const enabled = ref(false)
    const { update, onUpdate } = setup(enabled)
    await flushPromises()
    receive({ payload: snapshot(1024) })
    receive({ payload: snapshot(2048) })
    expect(update.value).toBeNull()
    expect(onUpdate).not.toHaveBeenCalled()
    enabled.value = true
    expect(update.value?.stat.downloadSpeed).toBe(2048)
    expect(onUpdate).toHaveBeenCalledTimes(1)
    enabled.value = false
    receive({ payload: snapshot(4096) })
    expect(update.value?.stat.downloadSpeed).toBe(2048)
    enabled.value = true
    expect(update.value?.stat.downloadSpeed).toBe(4096)
    expect(onUpdate).toHaveBeenCalledTimes(2)
  })

  it('does not republish unchanged data after closing and reopening', async () => {
    const enabled = ref(true)
    const { onUpdate } = setup(enabled)
    await flushPromises()
    receive({ payload: snapshot() })
    enabled.value = false
    receive({ payload: snapshot() })
    enabled.value = true
    expect(onUpdate).toHaveBeenCalledTimes(1)
  })

  it('releases an event subscription that finishes after the component unmounts', async () => {
    let resolveSubscription: ((stop: () => void) => void) | undefined
    bridge.listen.mockReturnValue(
      new Promise<() => void>((resolve) => {
        resolveSubscription = resolve
      }),
    )
    const { wrapper } = setup()
    wrapper.unmount()
    resolveSubscription?.(bridge.unlisten)
    await flushPromises()
    expect(bridge.unlisten).toHaveBeenCalledTimes(1)
    expect(bridge.invoke).not.toHaveBeenCalled()
  })
})
