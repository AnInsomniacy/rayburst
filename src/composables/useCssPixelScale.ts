/**
 * @fileoverview Converts a page's CSS pixels to its window's logical pixels.
 *
 * Windows sized to their content multiply the content's CSS size by this
 * scale. The interface size alone is not enough: WebView2 also folds the
 * system text size into the page's scale (#731), so a window sized by the
 * interface size cuts off its content once that setting is above 100%.
 */
import type { WebviewWindow } from '@tauri-apps/api/webviewWindow'

/**
 * Logical pixels per CSS pixel, measured as the window's logical width over
 * the page's viewport width. `fallback`, the interface size, stands in while
 * the page has no width yet, as in a hidden window on WebKitGTK, where the
 * system text size does not scale the page.
 */
export async function cssPixelScale(view: WebviewWindow, fallback: number): Promise<number> {
  const viewport = window.visualViewport?.width ?? window.innerWidth
  if (viewport <= 0) return fallback
  try {
    const [size, factor] = await Promise.all([view.innerSize(), view.scaleFactor()])
    const scale = size.width / factor / viewport
    return Number.isFinite(scale) && scale > 0 ? scale : fallback
  } catch {
    return fallback
  }
}
