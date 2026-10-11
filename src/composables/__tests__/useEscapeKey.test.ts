/** @fileoverview Tests for deciding whether an Escape press is free to act on. */
import { afterEach, describe, expect, it } from 'vitest'
import { isFreeEscape } from '../useEscapeKey'

function press(target: Element, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, ...init })
  Object.defineProperty(event, 'target', { value: target })
  return event
}

describe('isFreeEscape', () => {
  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('accepts a bare Escape on the page', () => {
    expect(isFreeEscape(press(document.body))).toBe(true)
    expect(isFreeEscape(press(document.body, { key: 'Enter' }))).toBe(false)
    expect(isFreeEscape(press(document.body, { shiftKey: true }))).toBe(false)
  })

  it('leaves Escape to text fields unless asked', () => {
    const input = document.body.appendChild(document.createElement('input'))
    expect(isFreeEscape(press(input))).toBe(false)
    expect(isFreeEscape(press(input), { fromEditable: true })).toBe(true)
  })

  it('leaves Escape to open overlays and honours the veto', () => {
    expect(isFreeEscape(press(document.body), { when: () => false })).toBe(false)
    const modal = document.body.appendChild(document.createElement('div'))
    modal.className = 'n-modal'
    expect(isFreeEscape(press(document.body))).toBe(false)
    modal.className = 'n-popover n-tooltip'
    expect(isFreeEscape(press(document.body))).toBe(true)
  })
})
