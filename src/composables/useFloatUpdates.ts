/**
 * @fileoverview Snapshots the desktop sends the floating orb and its panel.
 */
import { onBeforeUnmount, onMounted, shallowRef } from 'vue'
import { isEqual } from 'lodash-es'
import { invoke } from '@tauri-apps/api/core'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { logger } from '@shared/logger'
import type { FloatUpdate } from '@shared/utils/floatOrb'

export type { FloatUpdate }

/**
 * Follows the snapshots for the lifetime of the calling component. A snapshot
 * equal to the last one is dropped, so a still orb and panel do not re-render.
 */
export function useFloatUpdates(onUpdate?: (next: FloatUpdate) => void) {
  const update = shallowRef<FloatUpdate | null>(null)
  let unlisten: UnlistenFn | null = null
  let disposed = false

  function publish(next: FloatUpdate) {
    if (disposed || isEqual(update.value, next)) return
    onUpdate?.(next)
    update.value = next
  }

  onMounted(async () => {
    try {
      const stop = await getCurrentWebviewWindow().listen<FloatUpdate>('float:update', ({ payload }) =>
        publish(payload),
      )
      if (disposed) {
        stop()
        return
      }
      unlisten = stop
    } catch (error: unknown) {
      logger.warn('FloatWindow.listen', error)
      return
    }
    // Listening now; the desktop sends a snapshot without waiting for its next tick.
    await invoke('float_window_ready').catch((error: unknown) => logger.debug('FloatWindow.ready', error))
  })

  onBeforeUnmount(() => {
    disposed = true
    unlisten?.()
  })

  return { update }
}
