import { beforeEach, describe, expect, it, vi } from 'vitest'
import { prepareTaskPreferenceOptions } from '../taskPreferenceOptions'

const api = vi.hoisted(() => ({
  fetchTaskList: vi.fn(),
  getOption: vi.fn(),
  changeOption: vi.fn(),
  saveSession: vi.fn(),
}))
vi.mock('@/api/aria2', () => api)

describe('task preference transaction', () => {
  beforeEach(() => {
    vi.resetAllMocks()
    api.fetchTaskList.mockResolvedValue([{ gid: 'one' }, { gid: 'two' }, { gid: 'one' }])
    api.getOption.mockImplementation(async ({ gid }) => ({ streamMaxConnections: gid === 'one' ? '4' : '64' }))
    api.changeOption.mockResolvedValue(undefined)
    api.saveSession.mockResolvedValue('OK')
  })

  it('captures overrides before mutation, deduplicates tasks and restores each original', async () => {
    const transaction = await prepareTaskPreferenceOptions({ 'stream-max-connections': '16' })
    expect(api.changeOption).not.toHaveBeenCalled()
    expect(api.getOption).toHaveBeenCalledTimes(2)
    await transaction.apply()
    await transaction.rollback()
    expect(api.changeOption.mock.calls).toEqual([
      [{ gid: 'one', options: { 'stream-max-connections': '16' } }],
      [{ gid: 'two', options: { 'stream-max-connections': '16' } }],
      [{ gid: 'one', options: { 'stream-max-connections': '4' } }],
      [{ gid: 'two', options: { 'stream-max-connections': '64' } }],
    ])
    expect(api.saveSession).toHaveBeenCalledTimes(2)
  })

  it('rejects partial failure and only rolls back attempted tasks', async () => {
    const transaction = await prepareTaskPreferenceOptions({ 'stream-max-connections': '16' })
    api.changeOption.mockRejectedValueOnce(new Error('RPC failure'))
    await expect(transaction.apply()).rejects.toThrow('RPC failure')
    await transaction.rollback()
    expect(api.changeOption).toHaveBeenLastCalledWith({ gid: 'one', options: { 'stream-max-connections': '4' } })
    expect(api.changeOption).toHaveBeenCalledTimes(2)
  })

  it('continues restoration after one rollback error and reports the failure', async () => {
    const transaction = await prepareTaskPreferenceOptions({ 'stream-max-connections': '16' })
    await transaction.apply()
    api.changeOption.mockRejectedValueOnce(new Error('rollback failure'))
    await expect(transaction.rollback()).rejects.toThrow('Task option rollback failed')
    expect(api.changeOption).toHaveBeenLastCalledWith({ gid: 'two', options: { 'stream-max-connections': '64' } })
    expect(api.saveSession).toHaveBeenCalledTimes(2)
  })

  it('refuses missing snapshots without changing any task', async () => {
    api.getOption.mockResolvedValue({})
    await expect(prepareTaskPreferenceOptions({ 'stream-max-connections': '16' })).rejects.toThrow(
      'Missing task option',
    )
    expect(api.changeOption).not.toHaveBeenCalled()
  })

  it('does no task RPC for nonpropagatable changes', async () => {
    const transaction = await prepareTaskPreferenceOptions({ 'max-concurrent-downloads': '8' })
    await transaction.apply()
    await transaction.rollback()
    expect(api.fetchTaskList).not.toHaveBeenCalled()
    expect(api.saveSession).not.toHaveBeenCalled()
  })
})
