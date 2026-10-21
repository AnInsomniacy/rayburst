/**
 * @fileoverview Extracted notification handlers for task lifecycle events.
 *
 * MainLayout registers these callbacks on the lifecycle service.
 *
 * **Notification architecture:**
 * - In-app toast (Naive UI message) — always fires for immediate feedback.
 * - OS-level completion/error notification is sent by Rust's task monitor so
 *   lightweight mode works after the WebView is destroyed.
 *
 * Completion toasts render inline action buttons so the user can open
 * the downloaded file or reveal it in the system file manager directly
 * from the notification — without navigating through the task list.
 */
import type { VNodeChild } from 'vue'
import type { Aria2Task } from '@shared/types'
import type { SoundEvent } from '@shared/utils/sound'
import { getTaskName } from '@shared/utils'
import type { TaskSharingKind } from '@shared/utils/task'
import { logger } from '@shared/logger'
import { summarizeExternalInput } from '@shared/utils/externalInputDiagnostics'
import { isMetadataTask } from '@/composables/useTaskLifecycle'
import { renderCompletionToast } from '@/composables/useNotificationToast'

export interface NotifyDeps {
  messageSuccess: (content: string | (() => VNodeChild)) => void
  messageError: (content: string) => void
  t: (key: string, params?: Record<string, unknown>) => string
  onOpenFile: (task: Aria2Task) => void
  onShowInFolder: (task: Aria2Task) => void
  /** Optional sound hook — fired alongside the toast when the user enabled sound alerts. */
  playSound?: (event: SoundEvent) => void
}

/**
 * Handle a completed stream download.
 * Always sends in-app toast. Native OS notification is sent by Rust monitor.
 *
 * The toast includes inline buttons
 * for "Open File" and "Show in Folder".
 */
export function handleTaskComplete(task: Aria2Task, deps: NotifyDeps): void {
  if (isMetadataTask(task)) return

  const taskName = getTaskName(task)
  const body = deps.t('task.download-complete-message', { taskName })

  const toastContent = renderCompletionToast({
    body,
    t: deps.t,
    onOpenFile: () => deps.onOpenFile(task),
    onShowInFolder: () => deps.onShowInFolder(task),
  })
  deps.messageSuccess(toastContent)
  deps.playSound?.('complete')
  logger.debug('TaskNotify.complete', 'completion_toast_shown', { gid: task.gid, task_name: taskName })
}

/**
 * Handle a P2P download entering shared-upload state.
 * Always sends in-app toast. Native OS notification is sent by Rust monitor.
 *
 * The toast includes inline buttons
 * for "Open File" and "Show in Folder".
 */
export function handleP2pDownloadComplete(task: Aria2Task, kind: TaskSharingKind, deps: NotifyDeps): void {
  const taskName = getTaskName(task)
  const bodyKey = kind === 'bt' ? 'task.bt-download-complete-message' : 'task.ed2k-download-complete-message'
  const body = deps.t(bodyKey, { taskName })

  const toastContent = renderCompletionToast({
    body,
    t: deps.t,
    onOpenFile: () => deps.onOpenFile(task),
    onShowInFolder: () => deps.onShowInFolder(task),
  })
  deps.messageSuccess(toastContent)
  deps.playSound?.('complete')
  logger.debug('TaskNotify.p2pDownloadComplete', 'p2p_completion_toast_shown', {
    gid: task.gid,
    kind,
    task_name: taskName,
  })
}

/**
 * Handle a download error.
 * Always sends in-app toast. Native OS notification is sent by Rust monitor.
 */
export function handleTaskError(
  task: Aria2Task,
  reason: string,
  deps: Pick<NotifyDeps, 'messageError' | 't' | 'playSound'>,
): void {
  const taskName = getTaskName(task, { defaultName: 'Unknown' })
  const body = deps.t('task.download-fail-message', { taskName, reason })
  deps.messageError(body)
  deps.playSound?.('error')
  logger.warn('TaskNotify.error', 'download_error_toast_shown', { gid: task.gid, reason })
}

// ── Download-start notification ─────────────────────────────────────

/** Dependency interface for start notification — minimal subset. */
export interface StartNotifyDeps {
  messageInfo: (content: string) => void
  t: (key: string, params?: Record<string, unknown>) => string
  /** Optional sound hook — fired alongside the toast when the user enabled sound alerts. */
  playSound?: (event: SoundEvent) => void
}

/**
 * Handle download submission success — send start notification.
 *
 * For single tasks:  "Downloading: movie.mp4"
 * For batch tasks:   "Downloading: movie.mp4 and 2 other task(s)"
 *
 * Native start notifications are owned by the background lifecycle monitor.
 */
export function handleTaskStart(taskNames: string[], deps: StartNotifyDeps): void {
  if (taskNames.length === 0) return

  const firstName = taskNames[0]
  const body =
    taskNames.length === 1
      ? deps.t('task.download-start-message', { taskName: firstName })
      : deps.t('task.download-batch-start-message', {
          taskName: firstName,
          count: taskNames.length - 1,
        })

  deps.messageInfo(body)
  deps.playSound?.('start')
  logger.info('TaskNotify.start', 'download_notification_started', {
    count: taskNames.length,
    first: /^(?:https?|sftp|magnet|ed2k|thunder):/i.test(firstName) ? summarizeExternalInput(firstName) : firstName,
  })
}
