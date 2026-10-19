/** @fileoverview Converts CSS content dimensions to native logical window dimensions. */
import { computed, inject, onBeforeUnmount, onMounted, ref, watch, type InjectionKey } from 'vue'
import { useDevicePixelRatio, useEventListener } from '@vueuse/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { usePreferenceStore } from '@/stores/preference'
import { logger } from '@shared/logger'

/** Captured before applying WebView zoom, which some engines include in devicePixelRatio. */
export const SYSTEM_WINDOW_SCALE: InjectionKey<number> = Symbol('systemWindowScale')

function viewportWidth(): number {
  return window.visualViewport?.width || window.innerWidth
}

export function useWindowScale() {
  const preferenceStore = usePreferenceStore()
  const systemScale = ref(inject(SYSTEM_WINDOW_SCALE, 1))
  const scale = computed(() => systemScale.value * (preferenceStore.config.uiScale || 1))
  const { pixelRatio } = useDevicePixelRatio()
  let frame = 0
  let active = true
  let measuring = false
  let pending = false

  // GNOME text scaling can change WebKitGTK's CSS pixel size independently of
  // GTK's native scale factor. Measure the viewport to follow later changes,
  // without assuming whether the engine includes page zoom in devicePixelRatio.
  async function refresh() {
    if (measuring) {
      pending = true
      return
    }
    const cssWidth = viewportWidth()
    if (cssWidth <= 0) return // GTK can give hidden windows a zero-sized viewport.
    measuring = true
    const uiScale = preferenceStore.config.uiScale || 1
    try {
      const view = getCurrentWindow()
      const [size, nativeScale] = await Promise.all([view.innerSize(), view.scaleFactor()])
      if (!active || cssWidth !== viewportWidth() || uiScale !== (preferenceStore.config.uiScale || 1)) return
      const logicalWidth = size.width / nativeScale
      // innerWidth is rounded to a CSS pixel. Ignore that rounding so fitting a
      // small orb does not repeatedly resize it by a pixel in either direction.
      if (logicalWidth > 0 && Math.abs(logicalWidth - cssWidth * scale.value) > scale.value) {
        systemScale.value = logicalWidth / cssWidth / uiScale
      }
    } catch (error) {
      logger.debug('WindowScale.measure', error)
    } finally {
      measuring = false
      if (pending && active) {
        pending = false
        scheduleRefresh()
      }
    }
  }

  function scheduleRefresh() {
    if (!active || frame) return
    frame = requestAnimationFrame(() => {
      frame = 0
      void refresh()
    })
  }

  useEventListener(window, 'resize', scheduleRefresh)
  watch(pixelRatio, scheduleRefresh)
  onMounted(scheduleRefresh)
  onBeforeUnmount(() => {
    active = false
    cancelAnimationFrame(frame)
  })

  return scale
}
