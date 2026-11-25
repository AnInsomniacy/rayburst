import { describe, expect, it } from 'vitest'
import { opensEditMenu } from '../contextMenu'

describe('opensEditMenu', () => {
  it('keeps the native menu in text fields only', () => {
    document.body.innerHTML = `
      <input id="text" />
      <input id="url" type="url" />
      <input id="password" type="password" />
      <input id="check" type="checkbox" />
      <textarea id="area"></textarea>
      <div id="editor" contenteditable="true"><span id="inside">x</span></div>
      <div id="plain"><button id="button">b</button></div>`
    const at = (id: string) => document.getElementById(id)
    expect(opensEditMenu(at('text'))).toBe(true)
    expect(opensEditMenu(at('url'))).toBe(true)
    expect(opensEditMenu(at('area'))).toBe(true)
    expect(opensEditMenu(at('inside'))).toBe(true)
    expect(opensEditMenu(at('password'))).toBe(true)
    expect(opensEditMenu(at('check'))).toBe(false)
    expect(opensEditMenu(at('button'))).toBe(false)
    expect(opensEditMenu(null)).toBe(false)
  })
})
