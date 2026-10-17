/**
 * @fileoverview Snapshots the desktop sends the floating orb and its panel.
 */
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { logger } from '@shared/logger'
import type { FloatUpdate } from '@shared/utils/floatOrb'

export type { FloatUpdate }

/** Follows the snapshots for the lifetime of the calling component. */
export function useFloatUpdates(onUpdate?: (next: FloatUpdate) => void) {
  const update = ref<FloatUpdate | null>(null)
  let unlisten: UnlistenFn | null = null

  onMounted(async () => {
    unlisten = await getCurrentWebviewWindow().listen<FloatUpdate>('float:update', ({ payload }) => {
      onUpdate?.(payload)
      update.value = payload
    })
    // Listening now; the desktop sends a snapshot without waiting for its next tick.
    await invoke('float_window_ready').catch((error: unknown) => logger.debug('FloatWindow.ready', error))
  })

  onBeforeUnmount(() => unlisten?.())

  return { update }
}
