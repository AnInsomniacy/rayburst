/** @fileoverview Applies the interface-size preference as WebView zoom and handles the zoom shortcuts. */
import { onMounted, onUnmounted, watch } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { usePreferenceStore } from '@/stores/preference'
import { NUMERIC_CONFIG_ENUM_VALUES } from '@shared/configConstraints'
import { logger } from '@shared/logger'

export const UI_SCALE_STEPS: readonly number[] = NUMERIC_CONFIG_ENUM_VALUES.uiScale

/** The next allowed scale from `current` in `direction`, or `current` at either end. */
export function stepUiScale(current: number, direction: 1 | -1): number {
  const index = UI_SCALE_STEPS.findIndex((step) => Math.abs(step - current) < 0.001)
  const from = index === -1 ? UI_SCALE_STEPS.indexOf(1) : index
  return UI_SCALE_STEPS[Math.min(Math.max(from + direction, 0), UI_SCALE_STEPS.length - 1)]
}

/** Maps Ctrl/Cmd with +, - or 0 to a zoom target; null for any other key. */
export function shortcutScale(event: KeyboardEvent, current: number): number | null {
  if (!(event.ctrlKey || event.metaKey) || event.altKey) return null
  if (event.key === '=' || event.key === '+') return stepUiScale(current, 1)
  if (event.key === '-' || event.key === '_') return stepUiScale(current, -1)
  if (event.key === '0') return 1
  return null
}

export function useInterfaceScale(): void {
  const preferenceStore = usePreferenceStore()

  watch(
    () => preferenceStore.config.uiScale,
    (scale) => {
      getCurrentWebview()
        .setZoom(scale || 1)
        .catch((error: unknown) => logger.debug('InterfaceScale.setZoom', error))
    },
    { immediate: true },
  )

  function onKeydown(event: KeyboardEvent) {
    const current = preferenceStore.config.uiScale
    const next = shortcutScale(event, current)
    if (next === null) return
    event.preventDefault()
    if (next !== current) void preferenceStore.updateAndSave({ uiScale: next })
  }

  onMounted(() => window.addEventListener('keydown', onKeydown))
  onUnmounted(() => window.removeEventListener('keydown', onKeydown))
}
