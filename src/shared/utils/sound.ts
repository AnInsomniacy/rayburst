/**
 * @fileoverview Short, dependency-free notification sounds synthesized with the
 * Web Audio API. No audio assets are bundled: each event tone is generated at
 * runtime, so the feature adds zero binary size and no licensing concerns.
 *
 * All playback failures (no AudioContext, autoplay policy rejection, device
 * errors) are swallowed intentionally — a sound alert must never break the
 * notification flow it accompanies.
 */
import { logger } from '@shared/logger'

/** Lifecycle events that can play a sound alert. */
export type SoundEvent = 'start' | 'complete' | 'error'

interface ToneStep {
  /** Oscillator frequency in Hz. */
  frequency: number
  /** Offset from the event start, in seconds. */
  at: number
  /** Tone duration in seconds. */
  duration: number
  type: OscillatorType
  /** Peak gain (0-1). Kept low: these are alerts, not media playback. */
  gain: number
}

/**
 * Tone sequences per event. Complete is an ascending two-note chime, error a
 * descending low pair, start a single soft blip so frequent adds stay subtle.
 */
const SOUND_STEPS: Record<SoundEvent, ToneStep[]> = {
  start: [{ frequency: 880, at: 0, duration: 0.12, type: 'sine', gain: 0.08 }],
  complete: [
    { frequency: 659.25, at: 0, duration: 0.16, type: 'sine', gain: 0.1 },
    { frequency: 987.77, at: 0.14, duration: 0.24, type: 'sine', gain: 0.1 },
  ],
  error: [
    { frequency: 392, at: 0, duration: 0.18, type: 'triangle', gain: 0.12 },
    { frequency: 261.63, at: 0.18, duration: 0.28, type: 'triangle', gain: 0.12 },
  ],
}

let sharedContext: AudioContext | null = null

function getAudioContext(): AudioContext | null {
  if (typeof AudioContext === 'undefined') return null
  if (!sharedContext) sharedContext = new AudioContext()
  return sharedContext
}

function scheduleStep(context: AudioContext, step: ToneStep, baseTime: number): void {
  const oscillator = context.createOscillator()
  oscillator.type = step.type
  oscillator.frequency.value = step.frequency

  const envelope = context.createGain()
  const startAt = baseTime + step.at
  const endAt = startAt + step.duration
  // Fast attack, smooth release: avoids audible clicks at tone boundaries.
  envelope.gain.setValueAtTime(0, startAt)
  envelope.gain.linearRampToValueAtTime(step.gain, startAt + 0.01)
  envelope.gain.exponentialRampToValueAtTime(0.0001, endAt)

  oscillator.connect(envelope).connect(context.destination)
  oscillator.start(startAt)
  oscillator.stop(endAt)
}

/**
 * Play the alert tone for a lifecycle event. No-op when the Web Audio API is
 * unavailable (tests, SSR) or the context cannot start.
 */
export function playSoundEvent(event: SoundEvent): void {
  try {
    const context = getAudioContext()
    if (!context) return
    if (context.state === 'suspended') {
      // Autoplay policy: resume() may reject without a user gesture — ignore.
      void context.resume().catch(() => undefined)
    }
    const baseTime = context.currentTime
    for (const step of SOUND_STEPS[event]) scheduleStep(context, step, baseTime)
  } catch (e) {
    logger.debug('Sound', 'playback_failed', { event, error: String(e) })
  }
}
