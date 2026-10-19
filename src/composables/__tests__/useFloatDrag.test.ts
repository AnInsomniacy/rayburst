/** @fileoverview Regression coverage for repeated floating-orb native drags. */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { useFloatDrag } from '../useFloatDrag'

const bridge = vi.hoisted(() => ({ focus: vi.fn(), drag: vi.fn(), invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: bridge.invoke }))
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  getCurrentWebviewWindow: () => ({ setFocus: bridge.focus, startDragging: bridge.drag }),
}))
vi.mock('@shared/logger', () => ({ logger: { debug: vi.fn() } }))

const cleanups: (() => void)[] = []

function setup() {
  const onClick = vi.fn()
  const wrapper = mount(
    defineComponent({
      setup() {
        const { onMouseDown } = useFloatDrag(onClick)
        return () => h('div', { onMousedown: onMouseDown })
      },
    }),
    { attachTo: document.body },
  )
  cleanups.push(() => {
    wrapper.unmount()
    wrapper.element.remove()
  })
  const mouse = (type: string, options: MouseEventInit = {}) =>
    wrapper.trigger(type, { button: 0, buttons: 1, screenX: 100, screenY: 100, ...options })
  return { wrapper, onClick, mouse }
}

beforeEach(() => {
  vi.clearAllMocks()
  bridge.focus.mockResolvedValue(undefined)
  bridge.drag.mockResolvedValue(undefined)
  bridge.invoke.mockResolvedValue(undefined)
})
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup()
})

describe('floating orb dragging', () => {
  it('moves on all six presses when native dragging consumes release and pointer events', async () => {
    const { mouse, wrapper, onClick } = setup()
    for (let attempt = 1; attempt <= 6; attempt++) {
      // WebKitGTK only sends pointerdown on odd presses after a native drag.
      if (attempt % 2 === 1) await wrapper.trigger('pointerdown', { pointerId: 1, buttons: 1 })
      await mouse('mousedown')
      await mouse('mousemove', { screenX: 104 })
      await flushPromises()
      await mouse('mousemove', { screenX: 120 })
      // The native move loop consumes mouseup: it never reaches the webview.
      expect(bridge.drag).toHaveBeenCalledTimes(attempt)
    }
    expect(bridge.focus).toHaveBeenCalledTimes(6)
    expect(bridge.invoke).toHaveBeenCalledTimes(6)
    expect(bridge.invoke).toHaveBeenCalledWith('float_panel_dismiss')
    expect(onClick).not.toHaveBeenCalled()
  })

  it('keeps a click with small mouse movement opening the panel', async () => {
    const { mouse, onClick } = setup()
    await mouse('mousedown')
    await mouse('mousemove', { screenX: 102, screenY: 102 })
    await mouse('mouseup', { buttons: 0 })
    expect(onClick).toHaveBeenCalledTimes(1)
    expect(bridge.drag).not.toHaveBeenCalled()
  })

  it('keeps clicking after a native drag that consumed mouseup', async () => {
    const { mouse, onClick } = setup()
    await mouse('mousedown')
    await mouse('mousemove', { screenX: 110 })
    await flushPromises()
    await mouse('mousedown')
    await mouse('mouseup', { buttons: 0 })
    expect(bridge.drag).toHaveBeenCalledTimes(1)
    expect(onClick).toHaveBeenCalledTimes(1)
  })

  it('follows mouse movement and release outside the orb', async () => {
    const { mouse, onClick } = setup()
    await mouse('mousedown')
    window.dispatchEvent(new MouseEvent('mousemove', { buttons: 1, screenX: 110, screenY: 100 }))
    await flushPromises()
    window.dispatchEvent(new MouseEvent('mouseup', { button: 0, buttons: 0 }))
    expect(bridge.drag).toHaveBeenCalledTimes(1)
    expect(onClick).not.toHaveBeenCalled()
  })

  it('waits for focus before handing the held press to native dragging', async () => {
    let focused: (() => void) | undefined
    bridge.focus.mockReturnValue(new Promise<void>((resolve) => (focused = resolve)))
    const { mouse } = setup()
    await mouse('mousedown')
    await mouse('mousemove', { screenX: 110 })
    expect(bridge.drag).not.toHaveBeenCalled()
    focused?.()
    await flushPromises()
    expect(bridge.drag).toHaveBeenCalledTimes(1)
  })

  it.each(['mouseup', 'blur'])('does not start a delayed drag after %s', async (ending) => {
    let focused: (() => void) | undefined
    bridge.focus.mockReturnValue(new Promise<void>((resolve) => (focused = resolve)))
    const { mouse, onClick } = setup()
    await mouse('mousedown')
    await mouse('mousemove', { screenX: 110 })
    window.dispatchEvent(new MouseEvent(ending, { button: 0, buttons: 0 }))
    focused?.()
    await flushPromises()
    expect(bridge.drag).not.toHaveBeenCalled()
    expect(onClick).not.toHaveBeenCalled()
  })

  it('ignores right clicks and movement after releasing the button', async () => {
    const { mouse, onClick } = setup()
    await mouse('mousedown', { button: 2, buttons: 2 })
    expect(bridge.focus).not.toHaveBeenCalled()
    await mouse('mousedown')
    await mouse('mouseup', { button: 2, buttons: 1 })
    await mouse('mousemove', { screenX: 110, buttons: 0 })
    await mouse('mouseup', { buttons: 0 })
    expect(bridge.drag).not.toHaveBeenCalled()
    expect(onClick).not.toHaveBeenCalled()
  })

  it('replaces an unfinished gesture when a new physical press arrives', async () => {
    let focused: (() => void) | undefined
    bridge.focus.mockReturnValueOnce(new Promise<void>((resolve) => (focused = resolve)))
    const { mouse } = setup()
    await mouse('mousedown')
    await mouse('mousemove', { screenX: 110 })
    await mouse('mousedown')
    focused?.()
    await flushPromises()
    expect(bridge.drag).not.toHaveBeenCalled()
    await mouse('mousemove', { screenX: 110 })
    await flushPromises()
    expect(bridge.drag).toHaveBeenCalledTimes(1)
  })

  it('cancels pending dragging and removes event listeners on unmount', async () => {
    let focused: (() => void) | undefined
    bridge.focus.mockReturnValue(new Promise<void>((resolve) => (focused = resolve)))
    const { mouse, wrapper, onClick } = setup()
    await mouse('mousedown')
    await mouse('mousemove', { screenX: 110 })
    wrapper.unmount()
    focused?.()
    await flushPromises()
    window.dispatchEvent(new MouseEvent('mouseup', { button: 0, buttons: 0 }))
    expect(bridge.drag).not.toHaveBeenCalled()
    expect(onClick).not.toHaveBeenCalled()
  })

  it('allows the next gesture after native dragging fails', async () => {
    bridge.drag.mockRejectedValueOnce(new Error('drag unavailable'))
    const { mouse } = setup()
    await mouse('mousedown')
    await mouse('mousemove', { screenX: 110 })
    await flushPromises()
    await mouse('mousedown')
    await mouse('mousemove', { screenX: 110 })
    await flushPromises()
    expect(bridge.drag).toHaveBeenCalledTimes(2)
  })
})
