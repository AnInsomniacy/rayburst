/** @fileoverview Mouse gestures and native dragging for the floating orb. */
import { onBeforeUnmount } from 'vue'
import { useEventListener } from '@vueuse/core'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { getErrorMessage } from '@shared/utils/errorMessage'
import { logger } from '@shared/logger'

/** Mouse travel that turns a press into a drag, in CSS pixels. */
const DRAG_THRESHOLD = 4

export function useFloatDrag(onClick: () => void) {
  const view = getCurrentWebviewWindow()
  let press: {
    x: number
    y: number
    focused: Promise<void>
    dragging: boolean
  } | null = null

  function release() {
    press = null
  }

  function onMouseDown(event: MouseEvent) {
    if (event.button !== 0) return
    // Native dragging can consume mouseup on WebKitGTK. The next physical
    // press still sends mousedown, but may not send pointerdown. Start a fresh
    // gesture for every mousedown without relying on pointer state or capture.
    const focused = view.setFocus().catch((error: unknown) => logger.debug('FloatWindow.focus', getErrorMessage(error)))
    press = { x: event.screenX, y: event.screenY, focused, dragging: false }
  }

  async function onMouseMove(event: MouseEvent) {
    const current = press
    if (!current) return
    if ((event.buttons & 1) === 0) {
      release()
      return
    }
    if (current.dragging || Math.hypot(event.screenX - current.x, event.screenY - current.y) < DRAG_THRESHOLD) return
    current.dragging = true
    await current.focused
    // Focus crosses IPC: the press may already have ended or been cancelled.
    if (press !== current) return
    release()
    void invoke('float_panel_dismiss').catch((error: unknown) =>
      logger.debug('FloatWindow.dismiss', getErrorMessage(error)),
    )
    await view.startDragging().catch((error: unknown) => logger.debug('FloatWindow.drag', getErrorMessage(error)))
  }

  function onMouseUp(event: MouseEvent) {
    if (!press || event.button !== 0) return
    const clicked = !press.dragging
    release()
    if (clicked) onClick()
  }

  // Follow the gesture outside the orb, and stop when the webview loses focus.
  useEventListener(window, 'mousemove', onMouseMove)
  useEventListener(window, 'mouseup', onMouseUp)
  useEventListener(window, 'blur', release)
  onBeforeUnmount(release)
  return { onMouseDown }
}
