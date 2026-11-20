/**
 * @fileoverview Bridges task lifecycle events to sound alerts, honouring the
 * user's per-event sound preferences. Kept separate from the pure notify
 * handlers so they stay trivially testable via an injected `playSound` dep.
 */
import { usePreferenceStore } from '@/stores/preference'
import { playSoundEvent, type SoundEvent } from '@shared/utils/sound'

const ENABLED_BY_EVENT: Record<SoundEvent, 'soundOnStart' | 'soundOnComplete' | 'soundOnError'> = {
  start: 'soundOnStart',
  complete: 'soundOnComplete',
  error: 'soundOnError',
}

/** Play the event's alert tone when the matching preference is enabled. */
export function useNotificationSound(): (event: SoundEvent) => void {
  const preferenceStore = usePreferenceStore()
  return (event: SoundEvent) => {
    if (!preferenceStore.config[ENABLED_BY_EVENT[event]]) return
    playSoundEvent(event)
  }
}
