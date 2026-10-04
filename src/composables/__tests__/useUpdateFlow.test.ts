/**
 * @fileoverview Tests for useUpdateFlow pure functions.
 *
 * Tests the update dialog state machine, progress calculations,
 * version direction detection, proxy resolution, and error formatting.
 * Zero mocks — all pure functions.
 */
import { describe, it, expect } from 'vitest'
import {
  isActionDisabled,
  getActionLabel,
  getActionType,
  getActionTarget,
  shouldAllowUpdateDialogClose,
  calcProgressPercent,
  bytesToMB,
  getUpdateProxy,
  formatUpdateError,
  type UpdatePhase,
} from '../useUpdateFlow'

// ── isActionDisabled ────────────────────────────────────────────────

describe('isActionDisabled', () => {
  it.each<[UpdatePhase, boolean]>([
    ['checking', true],
    ['up-to-date', true],
    ['available', false],
    ['downloading', false],
    ['ready', false],
    ['installing', true],
    ['error', false],
  ])('phase "%s" → disabled=%s', (phase, expected) => {
    expect(isActionDisabled(phase)).toBe(expected)
  })
})

// ── getActionLabel ──────────────────────────────────────────────────

describe('getActionLabel', () => {
  it('returns retry for error phase', () => {
    expect(getActionLabel('error')).toBe('app.retry')
  })

  it('returns cancel for downloading phase', () => {
    expect(getActionLabel('downloading')).toBe('app.cancel')
  })

  it('returns restart-and-install for ready phase', () => {
    expect(getActionLabel('ready')).toBe('preferences.restart-and-install')
  })

  it('returns download-update for normal upgrade', () => {
    expect(getActionLabel('available')).toBe('preferences.download-update')
  })

  it('returns installing for installing phase', () => {
    expect(getActionLabel('installing')).toBe('preferences.installing')
  })
})

// ── getActionType ───────────────────────────────────────────────────

describe('getActionType', () => {
  it.each<[UpdatePhase, 'default' | 'info' | 'primary']>([
    ['checking', 'default'],
    ['up-to-date', 'default'],
    ['downloading', 'default'],
    ['installing', 'default'],
    ['error', 'info'],
    ['available', 'primary'],
    ['ready', 'primary'],
  ])('phase "%s" → type "%s"', (phase, expected) => {
    expect(getActionType(phase)).toBe(expected)
  })
})

// ── getActionTarget ─────────────────────────────────────────────────

describe('getActionTarget', () => {
  it.each<[UpdatePhase, string | null]>([
    ['available', 'download'],
    ['downloading', 'cancel'],
    ['ready', 'install'],
    ['installing', null],
    ['error', 'retry'],
    ['checking', null],
    ['up-to-date', null],
  ])('phase "%s" → action "%s"', (phase, expected) => {
    expect(getActionTarget(phase)).toBe(expected)
  })
})

// ── shouldAllowUpdateDialogClose ───────────────────────────────────

describe('shouldAllowUpdateDialogClose', () => {
  it('disallows closing while downloading', () => {
    expect(shouldAllowUpdateDialogClose('downloading')).toBe(false)
  })

  it('disallows closing while installing', () => {
    expect(shouldAllowUpdateDialogClose('installing')).toBe(false)
  })

  it('allows closing in idle or terminal phases', () => {
    expect(shouldAllowUpdateDialogClose('checking')).toBe(true)
    expect(shouldAllowUpdateDialogClose('available')).toBe(true)
    expect(shouldAllowUpdateDialogClose('ready')).toBe(true)
    expect(shouldAllowUpdateDialogClose('error')).toBe(true)
    expect(shouldAllowUpdateDialogClose('up-to-date')).toBe(true)
  })
})

// ── calcProgressPercent ─────────────────────────────────────────────

describe('calcProgressPercent', () => {
  it('returns 0 when total is 0', () => {
    expect(calcProgressPercent(500, 0)).toBe(0)
  })

  it('returns 0 when total is negative', () => {
    expect(calcProgressPercent(500, -1)).toBe(0)
  })

  it('calculates percentage correctly', () => {
    expect(calcProgressPercent(50, 100)).toBe(50)
    expect(calcProgressPercent(100, 100)).toBe(100)
    expect(calcProgressPercent(33, 100)).toBe(33)
  })

  it('rounds to nearest integer', () => {
    expect(calcProgressPercent(1, 3)).toBe(33)
    expect(calcProgressPercent(2, 3)).toBe(67)
  })
})

// ── bytesToMB ───────────────────────────────────────────────────────

describe('bytesToMB', () => {
  it('converts bytes to MB with 1 decimal', () => {
    expect(bytesToMB(1048576)).toBe('1.0')
    expect(bytesToMB(5242880)).toBe('5.0')
    expect(bytesToMB(1572864)).toBe('1.5')
  })

  it('handles 0 bytes', () => {
    expect(bytesToMB(0)).toBe('0.0')
  })
})

// ── getUpdateProxy ──────────────────────────────────────────────────

describe('getUpdateProxy', () => {
  it('returns null when config is undefined', () => {
    expect(getUpdateProxy(undefined)).toBeNull()
  })

  it('returns null when proxy is disabled', () => {
    expect(getUpdateProxy({ mode: 'direct', server: 'http://p:8080', scope: ['update-app'] })).toBeNull()
  })

  it('returns null when no server', () => {
    expect(getUpdateProxy({ mode: 'manual', server: '', scope: ['update-app'] })).toBeNull()
  })

  it('returns null when scope does not include update-app', () => {
    expect(getUpdateProxy({ mode: 'manual', server: 'http://p:8080', scope: ['download'] })).toBeNull()
  })

  it('returns server when fully configured', () => {
    expect(getUpdateProxy({ mode: 'manual', server: 'http://p:8080', scope: ['update-app'] })).toBe('http://p:8080')
  })

  it('adds encoded proxy credentials when configured', () => {
    expect(
      getUpdateProxy({
        mode: 'manual',
        server: 'http://p:8080',
        username: 'user@example.com',
        password: 'pa:ss word',
        scope: ['update-app'],
      }),
    ).toBe('http://user%40example.com:pa%3Ass%20word@p:8080/')
  })

  it('returns null when scope is missing', () => {
    expect(getUpdateProxy({ mode: 'manual', server: 'http://p:8080' })).toBeNull()
  })
})

// ── formatUpdateError ───────────────────────────────────────────────

describe('formatUpdateError', () => {
  it('extracts message from Error objects', () => {
    expect(formatUpdateError(new Error('fail'))).toBe('fail')
  })

  it('returns strings directly', () => {
    expect(formatUpdateError('network timeout')).toBe('network timeout')
  })

  it('JSON.stringifies unknown types', () => {
    expect(formatUpdateError({ code: 404 })).toBe('{"code":404}')
  })
})
