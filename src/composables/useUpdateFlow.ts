/**
 * @fileoverview Pure functions extracted from UpdateDialog.vue for testability.
 *
 * Contains the update phase state machine logic: action button labels/types,
 * progress calculations, version direction detection, and proxy resolution.
 */
import { PROXY_SCOPES } from '@shared/constants'
import { resolveAppProxyUrl } from '@shared/utils/proxy'
import type { ProxyConfig } from '@shared/types'

// ── Types ───────────────────────────────────────────────────────────

export type UpdatePhase = 'checking' | 'up-to-date' | 'available' | 'downloading' | 'ready' | 'installing' | 'error'
// ── State Machine Pure Functions ────────────────────────────────────

/** Determines whether the action button should be disabled. */
export function isActionDisabled(phase: UpdatePhase): boolean {
  return phase === 'checking' || phase === 'up-to-date' || phase === 'installing'
}

/** Determines the action button label key based on phase. */
export function getActionLabel(phase: UpdatePhase): string {
  if (phase === 'error') return 'app.retry'
  if (phase === 'downloading') return 'app.cancel'
  if (phase === 'ready') return 'preferences.restart-and-install'
  if (phase === 'installing') return 'preferences.installing'
  return 'preferences.download-update'
}

/** Determines the action button Naive UI type based on phase. */
export function getActionType(phase: UpdatePhase): 'default' | 'info' | 'primary' {
  if (phase === 'downloading') return 'default'
  if (phase === 'error') return 'info'
  if (isActionDisabled(phase)) return 'default'
  return 'primary'
}

/** Determines which action to dispatch when button is clicked. */
export function getActionTarget(phase: UpdatePhase): 'download' | 'cancel' | 'install' | 'retry' | null {
  if (phase === 'available') return 'download'
  if (phase === 'downloading') return 'cancel'
  if (phase === 'ready') return 'install'
  if (phase === 'error') return 'retry'
  return null
}

/** Returns whether the dialog may be closed by generic close affordances. */
export function shouldAllowUpdateDialogClose(phase: UpdatePhase): boolean {
  return phase !== 'downloading' && phase !== 'installing'
}

// ── Progress Calculations ───────────────────────────────────────────

/** Calculates download progress percentage. */
export function calcProgressPercent(received: number, total: number): number {
  if (total <= 0) return 0
  return Math.round((received / total) * 100)
}

/** Converts bytes to megabytes string with 1 decimal. */
export function bytesToMB(bytes: number): string {
  return (bytes / 1048576).toFixed(1)
}

// ── Proxy Resolution ────────────────────────────────────────────────

/** Returns the proxy server URL if proxy is enabled for app updates. */
export function getUpdateProxy(proxyConfig: Partial<ProxyConfig> | undefined): string | null {
  return resolveAppProxyUrl(proxyConfig, PROXY_SCOPES.UPDATE_APP)
}

// ── Error Formatting ────────────────────────────────────────────────

/** Formats an unknown error into a display string. */
export function formatUpdateError(e: unknown): string {
  if (e instanceof Error) return e.message
  if (typeof e === 'string') return e
  return JSON.stringify(e)
}
