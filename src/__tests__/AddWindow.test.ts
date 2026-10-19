/** @fileoverview Regression tests for overlapping add-window presentations and closes. */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, shallowMount } from '@vue/test-utils'
import { reactive } from 'vue'
import type { EventCallback } from '@tauri-apps/api/event'
import AddWindow from '../AddWindow.vue'

interface Inputs {
  inputs: []
  links: string[]
  items: []
  manual: boolean
}

const empty = (): Inputs => ({ inputs: [], links: [], items: [], manual: false })
const manual = (): Inputs => ({ ...empty(), manual: true })
const bridge = vi.hoisted(() => ({
  invoke: vi.fn(),
  emit: vi.fn(),
  listen: vi.fn(),
  setTitle: vi.fn(),
  title: vi.fn(),
  setSize: vi.fn(),
  onScaleChanged: vi.fn(),
  setDecorations: vi.fn(),
  onCloseRequested: vi.fn(),
  destroy: vi.fn(),
  afterReveal: vi.fn(),
  reducedMotion: vi.fn(),
  resize: () => {},
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: bridge.invoke }))
vi.mock('@tauri-apps/api/event', () => ({ emit: bridge.emit }))
vi.mock('@tauri-apps/api/webviewWindow', () => ({ getCurrentWebviewWindow: () => bridge }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/stores/app', () => ({ useAppStore: () => store }))
vi.mock('@/stores/preference', () => ({
  usePreferenceStore: () => ({ config: { systemTitleBar: false }, reloadPreferenceFromDisk: vi.fn() }),
}))
vi.mock('@/composables/useAppMessage', () => ({ useAppMessage: () => ({ warning: vi.fn(), error: vi.fn() }) }))
vi.mock('@/composables/useEscapeKey', () => ({ useEscapeKey: vi.fn() }))
vi.mock('@/composables/useWindowScale', async () => {
  const { ref } = await import('vue')
  return { useWindowScale: () => ref(1) }
})
vi.mock('@/composables/usePlatform', async () => {
  const { ref } = await import('vue')
  return { usePlatform: () => ({ platform: ref('linux'), isLinux: ref(true), isMac: ref(false) }) }
})
vi.mock('@/composables/useTorrentDrop', () => ({ listenForTorrentDrops: async () => () => {} }))
vi.mock('@/composables/useWindowReveal', () => ({
  afterReveal: bridge.afterReveal,
  nextFrame: async () => {},
  reducedMotion: bridge.reducedMotion,
}))
vi.mock('@shared/logger', () => ({ logger: { warn: vi.fn(), debug: vi.fn() } }))
vi.mock('@/components/task/AddTask.vue', () => ({
  default: { name: 'AddTask', props: ['show'], emits: ['close', 'submitted'], template: '<div />' },
}))
vi.mock('@/components/layout/WindowControls.vue', () => ({ default: { template: '<div />' } }))
vi.mock('@/components/task/TorrentDropOverlay.vue', () => ({ default: { template: '<div />' } }))

const store = reactive({
  addTaskVisible: false,
  externalInputSubmitting: false,
  showAddTaskDialog: () => {
    store.addTaskVisible = true
  },
  hideAddTaskDialog: () => {
    store.addTaskVisible = false
  },
  setExternalInputErrorHandler: vi.fn(),
  enqueueForwardedItems: vi.fn(),
  handleDeepLinkUrls: vi.fn(),
  handleExternalInputs: vi.fn(),
})

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

let host: ReturnType<typeof shallowMount<typeof AddWindow>> | undefined
let receive: EventCallback<Inputs>
let contentHeight = 560

async function mountWindow() {
  host = shallowMount(AddWindow)
  await flushPromises()
  const listener = bridge.listen.mock.calls.find(([event]) => event === 'add-window:inputs')
  if (!listener) throw new Error('Input listener was not registered')
  receive = listener[1]
  return host
}

async function open() {
  receive({ event: 'add-window:inputs', id: 1, payload: manual() })
  await flushPromises()
}

async function close() {
  host?.findComponent({ name: 'AddTask' }).vm.$emit('close')
  await flushPromises()
}

beforeEach(() => {
  vi.resetAllMocks()
  store.addTaskVisible = false
  contentHeight = 560
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(
    () => new DOMRect(0, 0, 632, contentHeight),
  )
  bridge.invoke.mockImplementation(async (command: string) => (command === 'add_window_screen_height' ? 1000 : empty()))
  bridge.listen.mockResolvedValue(() => {})
  bridge.onCloseRequested.mockResolvedValue(() => {})
  bridge.emit.mockResolvedValue(undefined)
  bridge.setTitle.mockResolvedValue(undefined)
  bridge.setSize.mockResolvedValue(undefined)
  bridge.title.mockResolvedValue('')
  bridge.onScaleChanged.mockResolvedValue(() => {})
  bridge.setDecorations.mockResolvedValue(undefined)
  bridge.afterReveal.mockResolvedValue(undefined)
  bridge.reducedMotion.mockReturnValue(true)
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(callback: () => void) {
        bridge.resize = callback
      }
      observe() {}
      disconnect() {}
    },
  )
})

afterEach(() => {
  host?.unmount()
  host = undefined
  vi.useRealTimers()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('add-window lifecycle', () => {
  it('reuses its frontend and listeners across repeated open and close cycles', async () => {
    await mountWindow()
    for (let cycle = 0; cycle < 20; cycle += 1) {
      await open()
      expect(host?.classes()).toContain('add-window--shown')
      await close()
      expect(host?.classes()).toContain('add-window--hidden')
    }
    const commands = bridge.invoke.mock.calls.map(([command]) => command)
    expect(commands.filter((command) => command === 'add_window_attach')).toHaveLength(1)
    expect(commands.filter((command) => command === 'add_window_present')).toHaveLength(20)
    expect(commands.filter((command) => command === 'add_window_park')).toHaveLength(20)
    expect(bridge.listen).toHaveBeenCalledTimes(1)
    expect(bridge.onCloseRequested).toHaveBeenCalledTimes(1)
    expect(bridge.destroy).not.toHaveBeenCalled()
  })

  it('does not present again for repeated open requests while already shown', async () => {
    await mountWindow()
    for (let request = 0; request < 20; request += 1) await open()
    expect(bridge.invoke.mock.calls.filter(([command]) => command === 'add_window_present')).toHaveLength(1)
    expect(host?.classes()).toContain('add-window--shown')
    await close()
    expect(bridge.invoke.mock.calls.filter(([command]) => command === 'add_window_park')).toHaveLength(1)
  })

  it('cancels a presentation still measuring the screen when closed', async () => {
    const screen = deferred<number>()
    await mountWindow()
    bridge.invoke.mockImplementation(async (command: string) =>
      command === 'add_window_screen_height' ? screen.promise : empty(),
    )
    await open()
    await close()
    screen.resolve(1000)
    await flushPromises()
    expect(bridge.invoke).not.toHaveBeenCalledWith('add_window_present')
    expect(host?.classes()).toContain('add-window--hidden')
  })

  it('does not revive a closed panel when an earlier reveal finishes', async () => {
    const reveal = deferred<void>()
    await mountWindow()
    bridge.afterReveal.mockReturnValueOnce(reveal.promise)
    await open()
    await close()
    reveal.resolve()
    await flushPromises()
    expect(host?.classes()).toContain('add-window--hidden')
  })

  it('ignores a stale reveal after a newer open has started', async () => {
    const oldReveal = deferred<void>()
    const newReveal = deferred<void>()
    await mountWindow()
    bridge.afterReveal.mockReturnValueOnce(oldReveal.promise).mockReturnValueOnce(newReveal.promise)
    await open()
    await close()
    await open()
    oldReveal.resolve()
    await flushPromises()
    expect(host?.classes()).toContain('add-window--hidden')
    newReveal.resolve()
    await flushPromises()
    expect(host?.classes()).toContain('add-window--shown')
  })

  it('cancels pending presentation work when unmounted', async () => {
    const screen = deferred<number>()
    await mountWindow()
    bridge.invoke.mockImplementation(async (command: string) =>
      command === 'add_window_screen_height' ? screen.promise : empty(),
    )
    await open()
    host?.unmount()
    host = undefined
    screen.resolve(1000)
    await flushPromises()
    expect(bridge.invoke).not.toHaveBeenCalledWith('add_window_present')
  })

  it('parks only after an in-flight native show has completed', async () => {
    const present = deferred<void>()
    await mountWindow()
    bridge.invoke.mockImplementation(async (command: string) =>
      command === 'add_window_present' ? present.promise : command === 'add_window_screen_height' ? 1000 : empty(),
    )
    await open()
    await close()
    expect(bridge.invoke).not.toHaveBeenCalledWith('add_window_park')
    present.resolve()
    await flushPromises()
    expect(bridge.invoke).toHaveBeenCalledWith('add_window_park')
    expect(host?.classes()).toContain('add-window--hidden')
  })

  it('cancels closing and reattaches when an already delivered request still needs the window', async () => {
    const detach = deferred<Inputs>()
    await mountWindow()
    await open()
    bridge.invoke.mockImplementation(async (command: string) =>
      command === 'add_window_detach' ? detach.promise : command === 'add_window_screen_height' ? 1000 : empty(),
    )
    await close()
    await open()
    expect(bridge.invoke.mock.calls.filter(([command]) => command === 'add_window_present')).toHaveLength(1)
    detach.resolve(empty())
    await flushPromises()
    const commands = bridge.invoke.mock.calls.map(([command]) => command)
    expect(commands).not.toContain('add_window_park')
    expect(commands.filter((command) => command === 'add_window_attach')).toHaveLength(2)
    expect(commands.filter((command) => command === 'add_window_present')).toHaveLength(2)
    expect(host?.classes()).toContain('add-window--shown')
  })

  it('restores delivery for requests queued natively during detach', async () => {
    await mountWindow()
    await open()
    bridge.invoke.mockImplementation(async (command: string) =>
      command === 'add_window_detach' ? manual() : command === 'add_window_screen_height' ? 1000 : empty(),
    )
    await close()
    const commands = bridge.invoke.mock.calls.map(([command]) => command)
    expect(commands).not.toContain('add_window_park')
    expect(commands.filter((command) => command === 'add_window_attach')).toHaveLength(2)
    expect(host?.classes()).toContain('add-window--shown')
  })

  it('retains requests arriving during the leave animation without destroying their frontend', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    await mountWindow()
    await open()
    bridge.reducedMotion.mockReturnValue(false)
    await close()
    await open()
    expect(host?.classes()).toContain('add-window--leaving')
    expect(bridge.invoke.mock.calls.filter(([command]) => command === 'add_window_present')).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(200)
    await flushPromises()
    expect(bridge.invoke).not.toHaveBeenCalledWith('add_window_park')
    expect(host?.classes()).toContain('add-window--shown')
  })

  it('coalesces repeated close requests and skips layout resizing during the leave animation', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    await mountWindow()
    await open()
    bridge.reducedMotion.mockReturnValue(false)
    await close()
    await close()
    bridge.setSize.mockClear()
    contentHeight = 420
    bridge.resize()
    await vi.advanceTimersByTimeAsync(500)
    await flushPromises()
    expect(bridge.invoke.mock.calls.filter(([command]) => command === 'add_window_detach')).toHaveLength(1)
    expect(bridge.invoke.mock.calls.filter(([command]) => command === 'add_window_park')).toHaveLength(1)
    expect(bridge.setSize).not.toHaveBeenCalled()
  })
})
