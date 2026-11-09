/**
 * @fileoverview Applies a fresh task list to the rendered one without replacing it.
 *
 * Snapshots arrive every 0.5–2 s with new objects for every task. Replacing the
 * list re-rendered the list transition, which measures every card before and
 * after each update and so forced full style, layout and compositing passes on
 * every snapshot (#799). Updating the tasks in place leaves the list untouched
 * until its order or membership changes, and only the changed cards re-render.
 */
import { toRaw } from 'vue'
import { isEqual } from 'lodash-es'
import type { Aria2Task } from '@shared/types'

/** Same tasks in the same order. */
function sameOrder(current: readonly Aria2Task[], next: readonly Aria2Task[]): boolean {
  return current.length === next.length && current.every((task, index) => task.gid === next[index]?.gid)
}

/** Writes only the fields that differ, so unchanged fields notify nothing. */
function patchTask(target: Aria2Task, source: Aria2Task): void {
  // Tasks are plain JSON objects; the typed interface has no index signature.
  const into = target as unknown as Record<string, unknown>
  const before = toRaw(target) as unknown as Record<string, unknown>
  const from = source as unknown as Record<string, unknown>
  for (const key of Object.keys(before)) {
    if (!(key in from)) delete into[key]
  }
  for (const [key, value] of Object.entries(from)) {
    if (!isEqual(before[key], value)) into[key] = value
  }
}

/**
 * Updates `current` in place to match `next` when both list the same tasks in
 * the same order. Returns false otherwise; the caller then replaces the list.
 */
export function updateTasksInPlace(current: Aria2Task[], next: readonly Aria2Task[]): boolean {
  if (!sameOrder(current, next)) return false
  next.forEach((task, index) => {
    const target = current[index]
    if (!target || toRaw(target) === task) return
    // Restored history tasks are frozen and shared between snapshots: swap
    // them rather than write into them, and keep one once it arrives.
    if (Object.isFrozen(toRaw(target)) || Object.isFrozen(task)) current[index] = task
    else patchTask(target, task)
  })
  return true
}
