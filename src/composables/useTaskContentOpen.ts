import { invoke } from '@tauri-apps/api/core'
import type { Aria2Task } from '@shared/types'
import { logger } from '@shared/logger'
import { checkTaskIsSharing, resolveOpenTarget } from '@shared/utils/task'

/** Open completed content without intercepting controls or text selection. */
export function canOpenTaskContent(task: Aria2Task, event: MouseEvent): boolean {
  if (task.status !== 'complete' && !checkTaskIsSharing(task)) return false
  if (
    event.target instanceof Element &&
    event.target.closest('button, a, input, select, textarea, [role="button"], [contenteditable="true"]')
  )
    return false
  return !window.getSelection()?.toString()
}

/**
 * Drags a finished download's file out to other applications, as from a file
 * manager (#785). The system drag starts while the button is still down; a
 * drag from a control or of selected text stays in the page.
 */
export async function dragTaskContent(task: Aria2Task, event: DragEvent): Promise<void> {
  if (!canOpenTaskContent(task, event)) return
  event.preventDefault()
  const path = await resolveOpenTarget(task)
  if (!path || path === task.dir) return
  await invoke('start_file_drag', { paths: [path] }).catch((error: unknown) => logger.debug('Task.dragOut', error))
}
