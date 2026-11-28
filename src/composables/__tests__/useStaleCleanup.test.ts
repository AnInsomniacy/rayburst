/** @fileoverview TDD tests for runStaleRecordCleanup — the orchestration function
 * that connects the history store to file existence checks.
 *
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const mockCheckPathExists = vi.fn()
vi.mock('@tauri-apps/plugin-fs', () => ({
  remove: vi.fn(),
}))

// Mock Tauri path — join uses OS-native separator, mock with /
vi.mock('@tauri-apps/api/path', () => ({
  join: (...parts: string[]) => Promise.resolve(parts.join('/')),
}))

// Mock invoke — routes check_path_exists to dedicated handler
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === 'check_path_exists') return mockCheckPathExists(args)
    return Promise.reject(new Error(`Unexpected invoke: ${cmd}`))
  },
}))

const {
  getCompletedRecordRetentionCutoff,
  runCompletedRecordRetentionCleanup,
  runHistoryMaintenance,
  runStaleRecordCleanup,
} = await import('../useStaleCleanup')

describe('runStaleRecordCleanup', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('removes records whose files no longer exist', async () => {
    // 3 records: file1 exists, file2 gone, file3 gone
    mockCheckPathExists.mockResolvedValueOnce(true).mockResolvedValueOnce(false).mockResolvedValueOnce(false)

    const records = [
      { gid: 'g1', name: 'exists.zip', dir: '/dl', status: 'complete' },
      { gid: 'g2', name: 'gone.zip', dir: '/dl', status: 'complete' },
      { gid: 'g3', name: 'deleted.zip', dir: '/dl', status: 'complete' },
    ]

    const mockRemoveStale = vi.fn().mockResolvedValue(undefined)
    const result = await runStaleRecordCleanup(records, mockRemoveStale)

    expect(result.scanned).toBe(3)
    expect(result.removed).toBe(2)
    expect(mockRemoveStale).toHaveBeenCalledWith(['g2', 'g3'])
  })

  it('does nothing when all files exist', async () => {
    mockCheckPathExists.mockResolvedValue(true)

    const records = [{ gid: 'g1', name: 'a.zip', dir: '/dl', status: 'complete' }]

    const mockRemoveStale = vi.fn()
    const result = await runStaleRecordCleanup(records, mockRemoveStale)

    expect(result.scanned).toBe(1)
    expect(result.removed).toBe(0)
    expect(mockRemoveStale).not.toHaveBeenCalled()
  })

  it('does nothing with empty records', async () => {
    const mockRemoveStale = vi.fn()
    const result = await runStaleRecordCleanup([], mockRemoveStale)

    expect(result.scanned).toBe(0)
    expect(result.removed).toBe(0)
    expect(mockRemoveStale).not.toHaveBeenCalled()
  })

  it('handles errors gracefully without throwing', async () => {
    mockCheckPathExists.mockRejectedValue(new Error('fs error'))

    const records = [{ gid: 'g1', name: 'a.zip', dir: '/dl', status: 'complete' }]

    const mockRemoveStale = vi.fn().mockResolvedValue(undefined)
    // Should not throw — errors mean file doesn't exist, so mark stale
    const result = await runStaleRecordCleanup(records, mockRemoveStale)
    expect(result.removed).toBe(1)
  })
})

describe('completed record retention', () => {
  it('keeps records forever when retention is disabled', () => {
    const now = new Date('2026-05-26T00:00:00.000Z')

    expect(getCompletedRecordRetentionCutoff(0, now)).toBeNull()
  })

  it('builds a cutoff from the configured day count', () => {
    const now = new Date('2026-05-26T12:00:00.000Z')

    expect(getCompletedRecordRetentionCutoff(7, now)).toBe('2026-05-19T12:00:00.000Z')
  })

  it('removes expired completed records from history and aria2 result list', async () => {
    const records = [
      { gid: 'old', name: 'old.zip', status: 'complete', completed_at: '2026-05-01T00:00:00.000Z' },
      { gid: 'new', name: 'new.zip', status: 'complete', completed_at: '2026-05-25T00:00:00.000Z' },
      { gid: 'error', name: 'error.zip', status: 'error', completed_at: '2026-05-01T00:00:00.000Z' },
    ]
    const removeHistoryRecords = vi.fn().mockResolvedValue(undefined)
    const removeTaskRecord = vi.fn().mockResolvedValue('OK')

    const result = await runCompletedRecordRetentionCleanup({
      retentionDays: 7,
      now: new Date('2026-05-26T00:00:00.000Z'),
      records,
      removeHistoryRecords,
      removeTaskRecord,
    })

    expect(result).toEqual({ scanned: 3, removed: 1 })
    expect(removeHistoryRecords).toHaveBeenCalledWith(['old'])
    expect(removeTaskRecord).toHaveBeenCalledWith({ gid: 'old' })
  })
})

describe('runHistoryMaintenance', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns 0 without querying records when both cleanups are disabled', async () => {
    const getRecords = vi.fn()
    const removed = await runHistoryMaintenance({
      autoDeleteStaleRecords: false,
      completedRecordRetentionDays: 0,
      getRecords,
      removeStaleRecords: vi.fn(),
      removeHistoryRecords: vi.fn(),
      removeTaskRecord: vi.fn(),
      extractFilePaths: () => [],
    })

    expect(removed).toBe(0)
    expect(getRecords).not.toHaveBeenCalled()
  })

  it('removes stale records from both history and aria2 engine', async () => {
    mockCheckPathExists.mockResolvedValueOnce(true).mockResolvedValueOnce(false)
    const records = [
      { gid: 'exists', name: 'exists.zip', dir: '/dl', status: 'complete' },
      { gid: 'missing', name: 'missing.zip', dir: '/dl', status: 'complete' },
    ]
    const getRecords = vi.fn().mockResolvedValue(records)
    const removeStaleRecords = vi.fn().mockResolvedValue(undefined)
    const removeTaskRecord = vi.fn().mockResolvedValue('OK')

    const removed = await runHistoryMaintenance({
      autoDeleteStaleRecords: true,
      completedRecordRetentionDays: 0,
      getRecords,
      removeStaleRecords,
      removeHistoryRecords: removeStaleRecords,
      removeTaskRecord,
      extractFilePaths: (record) => [`${record.dir}/${record.name}`],
    })

    expect(removed).toBe(1)
    expect(getRecords).toHaveBeenCalledWith('complete')
    expect(removeStaleRecords).toHaveBeenCalledWith(['missing'])
    expect(removeTaskRecord).toHaveBeenCalledTimes(1)
    expect(removeTaskRecord).toHaveBeenCalledWith({ gid: 'missing' })
  })

  it('does not re-delete stale-removed records during retention cleanup', async () => {
    mockCheckPathExists.mockResolvedValueOnce(false).mockResolvedValueOnce(true)
    const records = [
      {
        gid: 'stale-and-old',
        name: 'gone.zip',
        dir: '/dl',
        status: 'complete',
        completed_at: '2020-01-01T00:00:00.000Z',
      },
      {
        gid: 'kept-but-old',
        name: 'old.zip',
        dir: '/dl',
        status: 'complete',
        completed_at: '2020-01-01T00:00:00.000Z',
      },
    ]
    const removeStaleRecords = vi.fn().mockResolvedValue(undefined)
    const removeHistoryRecords = vi.fn().mockResolvedValue(undefined)
    const removeTaskRecord = vi.fn().mockResolvedValue('OK')

    const removed = await runHistoryMaintenance({
      autoDeleteStaleRecords: true,
      completedRecordRetentionDays: 7,
      getRecords: vi.fn().mockResolvedValue(records),
      removeStaleRecords,
      removeHistoryRecords,
      removeTaskRecord,
      extractFilePaths: (record) => [`${record.dir}/${record.name}`],
    })

    expect(removed).toBe(2)
    expect(removeStaleRecords).toHaveBeenCalledWith(['stale-and-old'])
    expect(removeHistoryRecords).toHaveBeenCalledWith(['kept-but-old'])
    expect(removeTaskRecord).toHaveBeenCalledTimes(2)
    expect(removeTaskRecord).toHaveBeenNthCalledWith(1, { gid: 'stale-and-old' })
    expect(removeTaskRecord).toHaveBeenNthCalledWith(2, { gid: 'kept-but-old' })
  })
})
