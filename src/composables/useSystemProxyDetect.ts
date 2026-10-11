/**
 * @fileoverview Composable for detecting the OS-level system proxy.
 *
 * Calls the Rust `get_system_proxy` Tauri command and routes the result
 * through one of three callbacks: onSuccess, onNotFound, onError.
 *
 * Exposes a `detecting` ref for loading-state UI binding and guards
 * against concurrent invocations.
 */
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { logger } from '@shared/logger'
import type { SystemProxyInfo } from '@shared/types'

import { DETECT_MIN_DURATION } from '@shared/timing'

export interface SystemProxyDetectCallbacks {
  /** Called with the detected proxy info when a valid HTTP/HTTPS proxy is found. */
  onSuccess: (info: SystemProxyInfo) => void
  /** Called when no proxy is configured or the server field is empty. */
  onNotFound: () => void
  /** Called when the Tauri IPC call itself fails. */
  onError: (err: unknown) => void
}

/** Returns a promise that resolves after `ms` milliseconds. */
const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms))

/**
 * Provides a `detect()` function that reads the OS proxy via Tauri IPC,
 * and a reactive `detecting` flag for loading spinners.
 */
export function useSystemProxyDetect(callbacks: SystemProxyDetectCallbacks) {
  const detecting = ref(false)

  async function detect(): Promise<void> {
    if (detecting.value) return

    detecting.value = true
    try {
      // Run IPC and minimum-duration timer in parallel so the spinner
      // always displays for at least DETECT_MIN_DURATION ms.
      const [info] = await Promise.all([invoke<SystemProxyInfo | null>('get_system_proxy'), delay(DETECT_MIN_DURATION)])

      if (!info || !info.server?.trim()) {
        callbacks.onNotFound()
        return
      }

      callbacks.onSuccess(info)
    } catch (err: unknown) {
      logger.warn('SystemProxy', `detection failed: ${err}`)
      callbacks.onError(err)
    } finally {
      detecting.value = false
    }
  }

  return { detecting, detect }
}
