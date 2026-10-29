import { beforeEach, describe, expect, it, vi } from 'vitest'

const { gainEnvelope, resumeMock, oscillatorConnect, contextState, createOscillatorSpy } = vi.hoisted(() => ({
  gainEnvelope: {
    setValueAtTime: vi.fn(),
    linearRampToValueAtTime: vi.fn(),
    exponentialRampToValueAtTime: vi.fn(),
  },
  resumeMock: vi.fn().mockResolvedValue(undefined),
  oscillatorConnect: vi.fn(),
  createOscillatorSpy: vi.fn(),
  contextState: { value: 'running' },
}))

class MockAudioContext {
  get state() {
    return contextState.value
  }
  currentTime = 1
  destination = {}
  createOscillator = createOscillatorSpy.mockImplementation(() => {
    const oscillator = { type: '', frequency: { value: 0 }, start: vi.fn(), stop: vi.fn(), connect: oscillatorConnect }
    oscillatorConnect.mockReturnValue({ connect: vi.fn() })
    return oscillator
  })
  createGain = vi.fn(() => ({ gain: gainEnvelope, connect: vi.fn((node: unknown) => node) }))
  resume = resumeMock
}

vi.stubGlobal('AudioContext', MockAudioContext)

import { playSoundEvent } from '../sound'

describe('playSoundEvent', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    contextState.value = 'running'
  })

  it('schedules two tone steps with envelopes for complete', () => {
    playSoundEvent('complete')
    expect(createOscillatorSpy).toHaveBeenCalledTimes(2)
    expect(gainEnvelope.setValueAtTime).toHaveBeenCalledTimes(2)
    expect(gainEnvelope.linearRampToValueAtTime).toHaveBeenCalledTimes(2)
    expect(gainEnvelope.exponentialRampToValueAtTime).toHaveBeenCalledTimes(2)
  })

  it('plays a single soft blip for start', () => {
    playSoundEvent('start')
    expect(createOscillatorSpy).toHaveBeenCalledTimes(1)
  })

  it('plays a descending pair for error', () => {
    playSoundEvent('error')
    expect(createOscillatorSpy).toHaveBeenCalledTimes(2)
  })

  it('attempts to resume a suspended context without throwing', () => {
    contextState.value = 'suspended'
    expect(() => playSoundEvent('complete')).not.toThrow()
    expect(resumeMock).toHaveBeenCalled()
  })
})
