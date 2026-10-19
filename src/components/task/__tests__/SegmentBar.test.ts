/** @fileoverview Regression coverage for coalesced piece-bar drawing. */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { ref } from 'vue'
import SegmentBar from '../SegmentBar.vue'

const theme = vi.hoisted(() => ({ getTokens: vi.fn() }))
vi.mock('@/composables/useColorScheme', () => ({ useAppColorTokens: theme.getTokens }))

let resize: () => void
let nextFrame = 0
const frames = new Map<number, FrameRequestCallback>()
const context = { clearRect: vi.fn(), fillRect: vi.fn(), fillStyle: '', globalAlpha: 1 }
const disconnect = vi.fn()
const cleanups: (() => void)[] = []

function paint() {
  const queued = [...frames.values()]
  frames.clear()
  for (const callback of queued) callback(0)
}

function setup() {
  const wrapper = mount(SegmentBar, { props: { bitfield: 'a', pieces: 4 } })
  cleanups.push(() => wrapper.unmount())
  const canvas = wrapper.get('canvas').element
  Object.defineProperties(canvas, { clientWidth: { value: 4, configurable: true }, clientHeight: { value: 6 } })
  return { wrapper, canvas }
}

beforeEach(() => {
  vi.clearAllMocks()
  frames.clear()
  nextFrame = 0
  theme.getTokens.mockReturnValue(ref({ statusActive: '#9e74d5' }))
  vi.stubGlobal('devicePixelRatio', 1)
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
    frames.set(++nextFrame, callback)
    return nextFrame
  })
  vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id))
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(callback: () => void) {
        resize = callback
      }
      observe() {}
      disconnect = disconnect
    },
  )
  // happy-dom has no 2D renderer; this stub supplies the operations used by the component.
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(context as unknown as CanvasRenderingContext2D)
})

afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('piece-bar rendering', () => {
  it('coalesces resize notifications and does not repaint unchanged dimensions', () => {
    setup()
    resize()
    resize()
    expect(frames.size).toBe(1)
    paint()
    expect(context.clearRect).toHaveBeenCalledTimes(1)
    expect(context.fillRect.mock.calls).toEqual([
      [0, 0, 1, 6],
      [2, 0, 1, 6],
    ])
    resize()
    paint()
    expect(context.clearRect).toHaveBeenCalledTimes(1)
  })

  it('updates pieces without resetting the backing buffer at the same size', async () => {
    const { wrapper, canvas } = setup()
    paint()
    const width = vi.spyOn(canvas, 'width', 'set')
    const height = vi.spyOn(canvas, 'height', 'set')
    await wrapper.setProps({ bitfield: 'f' })
    resize()
    paint()
    expect(context.clearRect).toHaveBeenCalledTimes(2)
    expect(context.fillRect).toHaveBeenCalledTimes(6)
    expect(width).not.toHaveBeenCalled()
    expect(height).not.toHaveBeenCalled()
  })

  it('resizes the backing buffer when density or layout changes', () => {
    const { canvas } = setup()
    paint()
    vi.stubGlobal('devicePixelRatio', 2)
    resize()
    paint()
    expect(canvas.width).toBe(8)
    expect(canvas.height).toBe(12)
    Object.defineProperty(canvas, 'clientWidth', { value: 8 })
    resize()
    paint()
    expect(canvas.width).toBe(16)
    expect(context.clearRect).toHaveBeenCalledTimes(3)
  })

  it('cancels queued painting and disconnects observation on unmount', () => {
    const { wrapper } = setup()
    wrapper.unmount()
    paint()
    expect(context.clearRect).not.toHaveBeenCalled()
    expect(disconnect).toHaveBeenCalledTimes(1)
  })
})
