/** @fileoverview Orchestration for stale download record cleanup.
 *
 * Connects the history store records to file existence checks,
 * removing records whose local files no longer exist.
 */
import { findStaleRecords, type StaleCheckItem } from './useDownloadCleanup'
import { logger } from '@shared/logger'
import type { HistoryRecord } from '@shared/types'

interface CleanupResult {
  scanned: number
  removed: number
}

interface CompletedRecordRetentionCleanupOptions {
  retentionDays: number
  now?: Date
  records: HistoryRecord[]
  removeHistoryRecords: (gids: string[]) => Promise<void>
  removeTaskRecord: (params: { gid: string }) => Promise<unknown>
}

export interface HistoryMaintenanceOptions {
  autoDeleteStaleRecords: boolean
  completedRecordRetentionDays: number
  getRecords: (status?: string) => Promise<HistoryRecord[]>
  removeStaleRecords: (gids: string[]) => Promise<void>
  removeHistoryRecords: (gids: string[]) => Promise<void>
  removeTaskRecord: (params: { gid: string }) => Promise<unknown>
  extractFilePaths: (record: HistoryRecord) => string[]
}

/** Scan records for stale files and remove them via the provided callback.
 *  Designed for dependency injection: the caller passes the remove function
 *  from the history store, keeping this function pure and testable. */
export async function runStaleRecordCleanup(
  records: StaleCheckItem[],
  removeStaleRecords: (gids: string[]) => Promise<void>,
): Promise<CleanupResult> {
  if (records.length === 0) {
    return { scanned: 0, removed: 0 }
  }

  const staleGids = await findStaleRecords(records)

  if (staleGids.length > 0) {
    await removeStaleRecords(staleGids)
  }

  logger.info('StaleCleanup', `scanned=${records.length} removed=${staleGids.length}`)
  return { scanned: records.length, removed: staleGids.length }
}

export function getCompletedRecordRetentionCutoff(retentionDays: number, now = new Date()): string | null {
  if (!Number.isFinite(retentionDays) || retentionDays <= 0) return null
  return new Date(now.getTime() - retentionDays * 86_400_000).toISOString()
}

export async function runCompletedRecordRetentionCleanup(
  options: CompletedRecordRetentionCleanupOptions,
): Promise<CleanupResult> {
  const cutoff = getCompletedRecordRetentionCutoff(options.retentionDays, options.now)
  if (!cutoff || options.records.length === 0) {
    return { scanned: options.records.length, removed: 0 }
  }

  const expiredGids = options.records
    .filter((record) => record.status === 'complete' && !!record.completed_at && record.completed_at < cutoff)
    .map((record) => record.gid)

  if (expiredGids.length === 0) {
    return { scanned: options.records.length, removed: 0 }
  }

  await options.removeHistoryRecords(expiredGids)
  await Promise.allSettled(expiredGids.map((gid) => options.removeTaskRecord({ gid })))

  logger.info('CompletedRetentionCleanup', `scanned=${options.records.length} removed=${expiredGids.length}`)
  return { scanned: options.records.length, removed: expiredGids.length }
}

export async function runHistoryMaintenance(options: HistoryMaintenanceOptions): Promise<number> {
  const needsStaleCleanup = options.autoDeleteStaleRecords
  const needsRetentionCleanup = options.completedRecordRetentionDays > 0
  if (!needsStaleCleanup && !needsRetentionCleanup) return 0

  const records = await options.getRecords('complete')
  let remainingRecords = records
  let totalRemoved = 0

  if (needsStaleCleanup) {
    const removedGids = new Set<string>()
    const staleResult = await runStaleRecordCleanup(
      records.map((record) => ({
        gid: record.gid,
        name: record.name,
        dir: record.dir ?? '',
        filePaths: options.extractFilePaths(record),
      })),
      async (gids) => {
        await options.removeStaleRecords(gids)
        // Finished downloads the engine still lists would show again in the next snapshot.
        await Promise.allSettled(gids.map((gid) => options.removeTaskRecord({ gid })))
        for (const gid of gids) removedGids.add(gid)
      },
    )
    totalRemoved += staleResult.removed
    if (removedGids.size > 0) {
      remainingRecords = records.filter((record) => !removedGids.has(record.gid))
    }
  }

  if (needsRetentionCleanup) {
    const retentionResult = await runCompletedRecordRetentionCleanup({
      retentionDays: options.completedRecordRetentionDays,
      records: remainingRecords,
      removeHistoryRecords: options.removeHistoryRecords,
      removeTaskRecord: options.removeTaskRecord,
    })
    totalRemoved += retentionResult.removed
  }

  return totalRemoved
}
