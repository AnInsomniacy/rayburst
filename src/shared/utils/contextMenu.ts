/**
 * @fileoverview Which right-clicks keep the WebView's own menu.
 *
 * The app draws its own menus and blocks the browser one, except in text
 * fields, where the native menu offers cut, copy, paste and select all (#837).
 */

// Password fields included: secrets are pasted, and the WebView offers no copy there.
const TEXT_INPUTS = new Set(['', 'text', 'search', 'url', 'email', 'tel', 'number', 'password'])

/** Whether a right-click on `target` should open the native edit menu. */
export function opensEditMenu(target: EventTarget | null): boolean {
  if (typeof Element === 'undefined' || !(target instanceof Element)) return false
  const field = target.closest('input, textarea, [contenteditable]')
  if (!field) return false
  if (field instanceof HTMLInputElement) return TEXT_INPUTS.has(field.type.toLowerCase())
  if (field instanceof HTMLTextAreaElement) return true
  // The nearest contenteditable decides; "false" ends editing below it.
  return field.getAttribute('contenteditable')?.toLowerCase() !== 'false'
}
