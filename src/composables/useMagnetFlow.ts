/** @fileoverview Pure utility functions for the magnet URI file selection flow.
 *
 * Extracted as pure functions for testability:
 * - Detect magnet URIs
 * - Parse aria2 file list into UI-friendly selection items
 */
import type { Aria2File, Aria2Task, BtFileSelectionItem } from '@shared/types'

/** Check if a URI is a magnet link. */
export function isMagnetUri(uri: string): boolean {
  return uri.toLowerCase().startsWith('magnet:')
}

/** Convert raw Aria2File array into UI-friendly selection items. */
export function parseFilesForSelection(files: Aria2File[]): BtFileSelectionItem[] {
  return files
    .filter((file) => Number(file.length) > 0)
    .map((f) => {
      const parts = f.path.split(/[/\\]/)
      return {
        index: Number(f.index),
        name: parts[parts.length - 1],
        path: f.path,
        length: Number(f.length),
      }
    })
}

export function isPendingMagnetSelectionTask(task: Aria2Task): boolean {
  return (
    ['paused', 'waiting'].includes(task.status) &&
    ['awaiting', 'ready'].includes(task.bittorrent?.fileSelectionState ?? '')
  )
}
