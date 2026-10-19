/**
 * @fileoverview Snapshots the desktop sends the floating orb and its panel.
 */
import { onBeforeUnmount, onMounted, shallowRef, watch } from 'vue'
import { isEqual } from 'lodash-es'
import { invoke } from '@tauri-apps/api/core'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { logger } from '@shared/logger'
import type { FloatUpdate } from '@shared/utils/floatOrb'

export type { FloatUpdate }

interface FloatUpdateOptions {
  onUpdate?: (next: FloatUpdate) => void
  /** Disabled consumers retain only the latest snapshot until enabled again. */
  enabled?: () => boolean
}

/** Follows the snapshots for the lifetime of the calling component. */
export function useFloatUpdates({ onUpdate, enabled = () => true }: FloatUpdateOptions = {}) {
  const update = shallowRef<FloatUpdate | null>(null)
  let latest: FloatUpdate | null = null
  let unlisten: UnlistenFn | null = null
  let disposed = false

  function publish() {
    if (disposed || !enabled() || !latest || isEqual(update.value, latest)) return
    onUpdate?.(latest)
    update.value = latest
  }

  watch(enabled, publish, { flush: 'sync' })

  onMounted(async () => {
    try {
      const stop = await getCurrentWebviewWindow().listen<FloatUpdate>('float:update', ({ payload }) => {
        latest = payload
        publish()
      })
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
