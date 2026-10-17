/**
 * @fileoverview What the floating orb shows for a snapshot (#669, #718).
 *
 * Downloads and seeding are told apart: a torrent that only shares is not a
 * download whose size is unknown. Motion belongs to bytes arriving; a download
 * that waits on its servers, a paused queue or a seeding torrent holds still.
 */
import type { Aria2Task } from '@shared/types'
import { calcProgress, getTaskCompletedLength } from './task'

/** A snapshot the desktop sends the floating orb and its panel. */
export interface FloatUpdate {
  stat: { downloadSpeed: number; uploadSpeed: number; numActive: number }
  /** Overall progress of the active downloads in percent, when measurable. */
  progress: number | null
  /** Active downloads first, then waiting and paused ones. */
  tasks: Aria2Task[]
}

export type FloatOrbKind = 'idle' | 'downloading' | 'connecting' | 'seeding' | 'waiting' | 'paused' | 'done'

export interface FloatOrbState {
  kind: FloatOrbKind
  /** Bytes per second: received while downloading, sent while seeding. */
  speed: number
  /** Fill of the progress line in percent; null while it cannot be measured. */
  progress: number | null
  /** Tasks in this state. */
  count: number
}

const isSeeder = (task: Aria2Task) => task.seeder === 'true'

/** Overall progress of `tasks` by size, when every size is known. */
function overall(tasks: Aria2Task[]): number | null {
  let total = 0
  let completed = 0
  for (const task of tasks) {
    const length = Number(task.totalLength) || 0
    if (length === 0) return null
    total += length
    completed += getTaskCompletedLength(task)
  }
  return total > 0 ? Math.floor(calcProgress(total, completed)) : null
}

/** `done` holds for a moment after the last download completed. */
export function floatOrbState(update: FloatUpdate | null, done: boolean): FloatOrbState {
  const stat = update?.stat
  const tasks = update?.tasks ?? []
  const active = tasks.filter((task) => task.status === 'active')
  const downloading = active.filter((task) => !isSeeder(task))
  if (stat && downloading.length > 0) {
    return {
      kind: stat.downloadSpeed > 0 ? 'downloading' : 'connecting',
      speed: stat.downloadSpeed,
      progress: update.progress,
      count: downloading.length,
    }
  }
  if (done) return { kind: 'done', speed: 0, progress: 100, count: 0 }
  const seeding = active.filter(isSeeder)
  if (stat && seeding.length > 0) {
    return { kind: 'seeding', speed: stat.uploadSpeed, progress: 100, count: seeding.length }
  }
  const waiting = tasks.filter((task) => task.status === 'waiting')
  if (waiting.length > 0) return { kind: 'waiting', speed: 0, progress: overall(waiting), count: waiting.length }
  const paused = tasks.filter((task) => task.status === 'paused' && !isSeeder(task))
  if (paused.length > 0) return { kind: 'paused', speed: 0, progress: overall(paused), count: paused.length }
  return { kind: 'idle', speed: 0, progress: null, count: 0 }
}
