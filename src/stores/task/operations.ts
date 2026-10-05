/**
 * @fileoverview Extracted task CRUD operations from the Pinia task store.
 *
 * Contains task mutation and native batch-operation orchestration.
 *
 * Uses dependency injection — accepts API + store refs instead of importing
 * them directly, enabling testability and keeping the task store thin.
 */
import { TASK_STATUS } from '@shared/constants'
import { checkTaskIsBT, checkTaskIsSharing } from '@shared/utils'
import { logger } from '@shared/logger'
import { isAwaitingBtFileSelection } from '@/composables/useBtLifecycle'
import type { Aria2Task, TaskApi, TaskDeletionOptions } from '@shared/types'
import type { Ref } from 'vue'

interface TaskOperationsDeps {
  api: TaskApi
  taskList: Ref<Aria2Task[]>
  currentTaskGid: Ref<string>
  hideTaskDetail: () => void
  fetchList: () => Promise<void>
  setTaskRemoving?: (gid: string, removing: boolean) => void
  requestMediaSelection?: (task: Aria2Task) => void
  requestMagnetSelection?: (gid: string) => void
  clearSelections?: (gids: string[]) => void | Promise<void>
}

export function createTaskOperations(deps: TaskOperationsDeps) {
  const { api, currentTaskGid, hideTaskDetail, fetchList } = deps
  const setTaskRemoving = deps.setTaskRemoving ?? (() => undefined)

  async function removeTask(task: Aria2Task, options: TaskDeletionOptions = {}) {
    if (task.gid === currentTaskGid.value) hideTaskDetail()
    setTaskRemoving(task.gid, true)
    try {
      await api.deleteTask({ gid: task.gid, ...options })
      await deps.clearSelections?.([task.gid])
      logger.info('TaskOps.removeTask', `gid=${task.gid}`)
      setTaskRemoving(task.gid, false)
      await fetchList()
    } catch (error) {
      setTaskRemoving(task.gid, false)
      await fetchList()
      throw error
    }
  }

  async function pauseTask(task: Aria2Task) {
    const isBT = checkTaskIsBT(task)
    const promise = isBT ? api.forcePauseTask({ gid: task.gid }) : api.pauseTask({ gid: task.gid })
    try {
      await promise
      logger.info('TaskOps.pauseTask', `gid=${task.gid} bt=${isBT}`)
    } finally {
      await fetchList()
    }
  }

  async function finishSharing(task: Aria2Task): Promise<void> {
    try {
      await api.finishSharing({ gid: task.gid })
      logger.info('TaskOps.finishSharing', `gid=${task.gid}`)
    } finally {
      await fetchList()
    }
  }

  async function finishSharingTasks(gids: string[]) {
    try {
      const result = await api.batchFinishSharing({ gids })
      logger.info(
        'TaskOps.finishSharingTasks',
        `finished=${result.succeeded.length} failed=${result.failed.length} gids=[${gids.join(',')}]`,
      )
      return result
    } finally {
      await fetchList()
    }
  }

  async function resumeTask(task: Aria2Task): Promise<boolean> {
    if (task.media?.state === 'awaiting-selection') {
      deps.requestMediaSelection?.(task)
      return false
    }
    if (isAwaitingBtFileSelection(task)) {
      logger.info('TaskOps.resumeTask', `gid=${task.gid} blocked=file-selection-required`)
      deps.requestMagnetSelection?.(task.gid)
      return false
    }

    try {
      await api.resumeTask({ gid: task.gid })
      logger.info('TaskOps.resumeTask', `gid=${task.gid}`)
      return true
    } finally {
      await fetchList()
    }
  }

  async function pauseAllTask() {
    try {
      await api.forcePauseAll()
      logger.info('TaskOps.pauseAllTask', 'native forcePauseAll completed')
    } finally {
      await fetchList()
    }
  }

  async function resumeAllTask(): Promise<{ resumed: number; blocked: number }> {
    try {
      const result = await api.resumeEligible()
      logger.info('TaskOps.resumeAllTask', `resumed=${result.resumed} blocked=${result.blocked}`)
      return result
    } finally {
      await fetchList()
    }
  }

  function toggleTask(task: Aria2Task) {
    const { status } = task
    if (status === TASK_STATUS.ACTIVE) return pauseTask(task)
    if (status === TASK_STATUS.WAITING) return pauseTask(task)
    if (status === TASK_STATUS.PAUSED) return resumeTask(task)
    logger.debug('TaskOps.toggleTask', `no-op gid=${task.gid} status=${status} sharing=${checkTaskIsSharing(task)}`)
  }

  async function removeTaskRecord(task: Aria2Task) {
    await removeTask(task)
  }

  async function purgeTaskRecord() {
    await api.purgeTaskRecords()
    await fetchList()
  }

  async function batchRemoveTask(gids: string[], options: TaskDeletionOptions = {}) {
    gids.forEach((gid) => setTaskRemoving(gid, true))
    try {
      const result = await api.batchDeleteTasks({
        tasks: gids.map((gid) => ({ gid, ...options })),
      })
      await deps.clearSelections?.(result.succeeded)
      logger.info(
        'TaskOps.batchRemoveTask',
        `removed=${result.succeeded.length} failed=${result.failed.length} gids=[${gids.join(',')}]`,
      )
      return result
    } finally {
      gids.forEach((gid) => setTaskRemoving(gid, false))
      await fetchList()
    }
  }

  async function hasActiveTasks(): Promise<boolean> {
    try {
      const tasks = await api.fetchTaskList({ type: TASK_STATUS.ACTIVE })
      return tasks.some((t) => t.status === TASK_STATUS.ACTIVE || t.status === TASK_STATUS.WAITING)
    } catch (e) {
      logger.debug('TaskOps.hasActiveTasks', `fetchTaskList failed: ${e}`)
      return false
    }
  }

  async function hasPausedTasks(): Promise<boolean> {
    try {
      const tasks = await api.fetchTaskList({ type: TASK_STATUS.ACTIVE })
      return tasks.some((t) => t.status === TASK_STATUS.PAUSED)
    } catch (e) {
      logger.debug('TaskOps.hasPausedTasks', `fetchTaskList failed: ${e}`)
      return false
    }
  }

  async function saveSession() {
    await api.saveSession()
  }

  return {
    removeTask,
    pauseTask,
    finishSharing,
    finishSharingTasks,
    resumeTask,
    pauseAllTask,
    resumeAllTask,
    toggleTask,
    removeTaskRecord,
    purgeTaskRecord,
    batchRemoveTask,
    hasActiveTasks,
    hasPausedTasks,
    saveSession,
  }
}
