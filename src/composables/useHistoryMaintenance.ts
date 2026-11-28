/** @fileoverview Single-flight trigger for history maintenance (stale record + retention cleanup). */
import aria2Api from '@/api/aria2'
import { useHistoryStore } from '@/stores/history'
import { usePreferenceStore } from '@/stores/preference'
import { useTaskStore } from '@/stores/task'
import { logger } from '@shared/logger'
import { runHistoryMaintenance } from './useStaleCleanup'
import { extractHistoryFilePaths } from './useTaskLifecycle'

let inFlight: Promise<void> | null = null

/** Runs history maintenance with single-flight deduplication and refreshes the task list if records were removed. */
export function triggerHistoryMaintenance(): Promise<void> {
  if (inFlight) return inFlight

  inFlight = (async () => {
    try {
      const preferenceStore = usePreferenceStore()
      const historyStore = useHistoryStore()
      const taskStore = useTaskStore()
      const removed = await runHistoryMaintenance({
        autoDeleteStaleRecords: !!preferenceStore.config?.autoDeleteStaleRecords,
        completedRecordRetentionDays: Number(preferenceStore.config?.completedRecordRetentionDays ?? 0),
        getRecords: historyStore.getRecords,
        removeStaleRecords: historyStore.removeStaleRecords,
        removeHistoryRecords: historyStore.removeStaleRecords,
        removeTaskRecord: aria2Api.removeTaskRecord,
        extractFilePaths: extractHistoryFilePaths,
      })
      if (removed > 0) {
        await taskStore.fetchList()
      }
    } catch (e) {
      logger.debug('HistoryMaintenance', e)
    } finally {
      inFlight = null
    }
  })()

  return inFlight
}
