/** @fileoverview Runs an action on a bare Escape press that nothing on screen would consume. */
import { onMounted, onUnmounted } from 'vue'

/** Open Naive UI layers that handle Escape themselves. */
const ESCAPE_CONSUMERS = [
  '.n-modal',
  '.n-drawer',
  '.n-dialog',
  '.n-popover:not(.n-tooltip)',
  '.n-dropdown-menu',
  '.n-base-select-menu',
  '.n-color-picker-panel',
  '.n-date-panel',
  '.n-time-picker-panel',
  '.n-image-preview-container',
].join(', ')

const EDITABLE = 'input, textarea, select, [contenteditable="true"]'

export interface EscapeKeyOptions {
  /** Also fire while focus is in a text field. */
  fromEditable?: boolean
  /** Extra veto, e.g. while a selection is active. */
  when?: () => boolean
}

export function isFreeEscape(event: KeyboardEvent, options: EscapeKeyOptions = {}): boolean {
  if (event.key !== 'Escape' || event.repeat || event.isComposing) return false
  if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return false
  const target = event.target instanceof Element ? event.target : null
  if (!options.fromEditable && target?.closest(EDITABLE)) return false
  if (document.querySelector(ESCAPE_CONSUMERS)) return false
  return options.when?.() ?? true
}

/**
 * Listens in the capture phase so overlays are still mounted when the press
 * is checked; an overlay that closes on this press therefore keeps it.
 */
export function useEscapeKey(action: () => void, options: EscapeKeyOptions = {}): void {
  function onKeydown(event: KeyboardEvent) {
    if (isFreeEscape(event, options)) action()
  }
  onMounted(() => window.addEventListener('keydown', onKeydown, true))
  onUnmounted(() => window.removeEventListener('keydown', onKeydown, true))
}
