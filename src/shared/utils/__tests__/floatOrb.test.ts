/** @fileoverview Tests for what the floating orb shows for each snapshot. */
import { describe, it, expect } from 'vitest'
import { floatOrbState, type FloatUpdate } from '../floatOrb'
import type { Aria2Task } from '@shared/types'

function task(overrides: Partial<Aria2Task> = {}): Aria2Task {
  return {
    gid: '1',
    status: 'active',
    totalLength: '1000',
    completedLength: '500',
    uploadLength: '0',
    downloadSpeed: '0',
    uploadSpeed: '0',
    connections: '1',
    dir: '/tmp',
    files: [],
    ...overrides,
  }
}

function snapshot(tasks: Aria2Task[], speeds: { down?: number; up?: number } = {}, progress: number | null = null) {
  const update: FloatUpdate = {
    stat: {
      downloadSpeed: speeds.down ?? 0,
      uploadSpeed: speeds.up ?? 0,
      numActive: tasks.filter((t) => t.status === 'active').length,
    },
    progress,
    tasks,
  }
  return update
}

describe('floatOrbState', () => {
  it('rests without a snapshot or tasks', () => {
    expect(floatOrbState(null, false).kind).toBe('idle')
    expect(floatOrbState(snapshot([]), false).kind).toBe('idle')
  })

  it('shows the received speed and overall progress while bytes arrive', () => {
    const state = floatOrbState(snapshot([task(), task({ gid: '2' })], { down: 2048 }, 42), false)
    expect(state).toEqual({ kind: 'downloading', speed: 2048, progress: 42, count: 2 })
  })

  it('keeps an unmeasurable download apart from one that waits on its servers', () => {
    expect(floatOrbState(snapshot([task()], { down: 10 }), false)).toMatchObject({
      kind: 'downloading',
      progress: null,
    })
    expect(floatOrbState(snapshot([task()], { down: 0 }, 30), false)).toMatchObject({
      kind: 'connecting',
      progress: 30,
    })
  })

  it('shows a torrent that only shares as seeding, not as a download of unknown size', () => {
    const seeder = task({ seeder: 'true', completedLength: '1000' })
    expect(floatOrbState(snapshot([seeder], { up: 512 }), false)).toEqual({
      kind: 'seeding',
      speed: 512,
      progress: 100,
      count: 1,
    })
    // Downloads come first while both run.
    expect(floatOrbState(snapshot([task({ gid: '2' }), seeder], { down: 1, up: 512 }), false).kind).toBe('downloading')
  })

  it('holds the check after the last download until a new one starts', () => {
    expect(floatOrbState(snapshot([]), true).kind).toBe('done')
    expect(floatOrbState(snapshot([task({ seeder: 'true' })], { up: 1 }), true).kind).toBe('done')
    expect(floatOrbState(snapshot([task()], { down: 1 }), true).kind).toBe('downloading')
  })

  it('shows a queue that waits before one that is paused', () => {
    const paused = task({ status: 'paused', totalLength: '400', completedLength: '100' })
    const waiting = task({ gid: '2', status: 'waiting', completedLength: '0' })
    expect(floatOrbState(snapshot([waiting, paused]), false)).toMatchObject({ kind: 'waiting', progress: 0, count: 1 })
    expect(floatOrbState(snapshot([paused, task({ gid: '3', status: 'paused' })]), false)).toMatchObject({
      kind: 'paused',
      progress: 42,
      count: 2,
    })
  })

  it('leaves the line of a paused queue empty while a size is unknown', () => {
    const state = floatOrbState(snapshot([task({ status: 'paused', totalLength: '0' })]), false)
    expect(state).toMatchObject({ kind: 'paused', progress: null })
  })

  it('ignores paused seeding: nothing is left to download', () => {
    expect(floatOrbState(snapshot([task({ status: 'paused', seeder: 'true' })]), false).kind).toBe('idle')
  })
})
