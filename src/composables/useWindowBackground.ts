/**
 * @fileoverview Paints the native window in the theme's background.
 *
 * Windows are opaque. Until the page paints, and while the window resizes, the
 * window and its webview show their own background; keeping it in the theme's
 * color avoids a white flash and lets windows built later start from it.
 */
import { watch, type ComputedRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { logger } from '@shared/logger'

function parseRgb(color: string): [number, number, number] | null {
  const match = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/.exec(color)
  return match ? [Number(match[1]), Number(match[2]), Number(match[3])] : null
}

/** Reports the body's background, after the theme has applied, whenever it changes. */
export function useWindowBackground(theme: ComputedRef<unknown>): void {
  if (!('__TAURI_INTERNALS__' in window)) return
  let last = ''
  watch(
    theme,
    () => {
      const color = getComputedStyle(document.body).backgroundColor
      const rgb = parseRgb(color)
      if (!rgb || color === last) return
      last = color
      const [red, green, blue] = rgb
      invoke('window_background', { red, green, blue }).catch((error: unknown) =>
        logger.debug('useWindowBackground', error),
      )
    },
    { immediate: true, flush: 'post' },
  )
}
