/**
 * @fileoverview Pause all CSS infinite animations when the page is hidden.
 *
 * WebView2 on Windows continues rendering even when the window is minimized
 * or hidden — DWM compositing doesn't fully respect the Page Visibility API.
 * This composable toggles a CSS class on `<html>` that applies
 * `animation-play-state: paused` globally, eliminating needless GPU
 * re-composition each frame.
 *
 * The corresponding CSS rule lives in `variables.css`:
 *
 * ```css
 * .animations-paused *,
 * .animations-paused *::before,
 * .animations-paused *::after {
 *   animation-play-state: paused !important;
 * }
 * ```
 *
 * This is a cross-platform benefit: macOS and Linux also skip unnecessary
 * CSS animation work when the window is not visible.
 */
import { onMounted, onUnmounted } from 'vue'

const PAUSED_CLASS = 'animations-paused'

/**
 * Toggle the `animations-paused` class on `<html>` based on page visibility.
 *
 * Must be called inside a Vue component's `setup()` function so the
 * lifecycle hooks (`onMounted` / `onUnmounted`) attach correctly.
 */
export function useVisibilityPause(): void {
  let windowHidden = false
  let mounted = false
  let unlisten: Array<() => void> = []

  function apply(): void {
    document.documentElement.classList.toggle(PAUSED_CLASS, document.hidden || windowHidden)
  }

  function onVisibilityChange(): void {
    apply()
  }

  // WebView2 can keep reporting a minimized or tray-hidden window's page as
  // visible, so the window's own state counts too (#799).
  async function watchWindow(): Promise<void> {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      const appWindow = getCurrentWindow()
      const check = async () => {
        windowHidden = (await appWindow.isMinimized()) || !(await appWindow.isVisible())
        if (mounted) apply()
      }
      const listeners = await Promise.all([appWindow.onResized(check), appWindow.onFocusChanged(check)])
      if (mounted) unlisten = listeners
      else listeners.forEach((stop) => stop())
    } catch {
      // Outside a Tauri window only the page's visibility is known.
    }
  }

  onMounted(() => {
    mounted = true
    document.addEventListener('visibilitychange', onVisibilityChange)
    void watchWindow()
  })

  onUnmounted(() => {
    mounted = false
    document.removeEventListener('visibilitychange', onVisibilityChange)
    unlisten.forEach((stop) => stop())
    unlisten = []
  })
}
