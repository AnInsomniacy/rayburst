/**
 * @fileoverview The ratio of a window's logical pixels to its page's CSS pixels.
 *
 * Windows sized to their content convert the content's CSS size with it. The
 * interface size zooms the page, and the platform may scale its CSS pixels
 * beyond the screen's own scale: WebView2 folds the Windows text size in,
 * WebKitGTK the GNOME text scaling factor. Before the page is zoomed, its
 * device pixel ratio over the window's scale factor gives that system part,
 * which holds even while a hidden WebKitGTK window has no viewport to measure.
 * Once the page has a viewport, the window's logical width over the viewport's
 * CSS width measures the whole ratio directly; it is refreshed when the
 * window, the pixel ratio or the screen's scale changes.
 */
import { computed, inject, onBeforeUnmount, onMounted, ref, watch, type InjectionKey } from 'vue'
import { useDevicePixelRatio, useEventListener } from '@vueuse/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { usePreferenceStore } from '@/stores/preference'
import { logger } from '@shared/logger'

/** The system part of the scale, provided by the window's bootstrap before the page is zoomed. */
export const SYSTEM_WINDOW_SCALE: InjectionKey<number> = Symbol('systemWindowScale')

/** The system part of the scale for a page not yet zoomed. */
export async function systemWindowScale(): Promise<number> {
  const factor = await getCurrentWindow()
    .scaleFactor()
    .catch(() => window.devicePixelRatio || 1)
  const scale = (window.devicePixelRatio || 1) / factor
  return Number.isFinite(scale) && scale > 0 ? scale : 1
}

function viewportWidth(): number {
  return window.visualViewport?.width || window.innerWidth
}

/** Follows the window's logical pixels per CSS pixel for the calling component. */
export function useWindowScale() {
  const preferenceStore = usePreferenceStore()
  const view = getCurrentWindow()
  const system = ref(inject(SYSTEM_WINDOW_SCALE, 1))
  const zoom = computed(() => preferenceStore.config.uiScale || 1)
  const scale = computed(() => system.value * zoom.value)
  const { pixelRatio } = useDevicePixelRatio()
  let frame = 0
  let measuring = false
  let again = false
  let unlistenScale: UnlistenFn | undefined
  let active = true

  async function measure() {
    if (measuring) {
      again = true
      return
    }
    const width = viewportWidth()
    if (width <= 0) return
    measuring = true
    const zoomed = zoom.value
    try {
      const [size, factor] = await Promise.all([view.innerSize(), view.scaleFactor()])
      // A zoom applied meanwhile changes the viewport; the next resize measures it.
      if (!active || width !== viewportWidth() || zoomed !== zoom.value) return
      const logical = size.width / factor
      // The viewport is rounded to a CSS pixel: a smaller difference is that
      // rounding, and following it would resize the window back and forth.
      if (logical > 0 && Math.abs(logical - width * scale.value) > scale.value) {
        system.value = logical / width / zoomed
      }
    } catch (error) {
      logger.debug('WindowScale.measure', error)
    } finally {
      measuring = false
      if (again) {
        again = false
        schedule()
      }
    }
  }

  function schedule() {
    if (!active || frame) return
    frame = requestAnimationFrame(() => {
      frame = 0
      void measure()
    })
  }

  useEventListener(window, 'resize', schedule)
  watch(pixelRatio, schedule)
  onMounted(async () => {
    schedule()
    const unlisten = await view.onScaleChanged(schedule).catch(() => undefined)
    if (active) unlistenScale = unlisten
    else unlisten?.()
  })
  onBeforeUnmount(() => {
    active = false
    cancelAnimationFrame(frame)
    unlistenScale?.()
  })

  return scale
}
