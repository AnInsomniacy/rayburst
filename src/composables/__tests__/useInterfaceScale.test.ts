/** @fileoverview Tests for interface-size stepping and shortcuts. */
import { describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ setZoom: vi.fn() }) }))

import { shortcutScale, stepUiScale } from '../useInterfaceScale'

function key(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent('keydown', init)
}

describe('interface scale', () => {
  it('steps through the allowed sizes and stops at both ends', () => {
    expect(stepUiScale(1, 1)).toBe(1.1)
    expect(stepUiScale(1.1, 1)).toBe(1.25)
    expect(stepUiScale(1.5, 1)).toBe(1.5)
    expect(stepUiScale(0.8, -1)).toBe(0.8)
    expect(stepUiScale(1.07, -1)).toBe(0.9)
  })

  it('maps Ctrl or Cmd with +, - and 0 only', () => {
    expect(shortcutScale(key({ key: '=', ctrlKey: true }), 1)).toBe(1.1)
    expect(shortcutScale(key({ key: '-', metaKey: true }), 1)).toBe(0.9)
    expect(shortcutScale(key({ key: '0', ctrlKey: true }), 1.25)).toBe(1)
    expect(shortcutScale(key({ key: '=' }), 1)).toBeNull()
    expect(shortcutScale(key({ key: '=', ctrlKey: true, altKey: true }), 1)).toBeNull()
    expect(shortcutScale(key({ key: 'a', ctrlKey: true }), 1)).toBeNull()
  })
})
