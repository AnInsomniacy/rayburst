import { beforeEach, describe, expect, it, vi } from 'vitest'
import { resubmitTask } from '../task/resubmit'
import type { Aria2Task, TaskStatus } from '@shared/types'

vi.mock('@tauri-apps/api/path', async (importOriginal) => {
  const { win32 } = await import('node:path')
  return {
    ...(await importOriginal<typeof import('@tauri-apps/api/path')>()),
    basename: async (path: string) => win32.basename(path),
    dirname: async (path: string) => {
      const parent = win32.dirname(path)
      // Tauri returns an empty parent for a bare filename, unlike Node.
      return parent === '.' ? '' : parent
    },
  }
})

const makeTask = (status: TaskStatus, extra: Partial<Aria2Task> = {}): Aria2Task => ({
  gid: 'old-gid',
  status,
  totalLength: '100',
  completedLength: status === 'complete' ? '100' : '50',
  uploadLength: '0',
  downloadSpeed: '0',
  uploadSpeed: '0',
  connections: '0',
  dir: '/downloads',
  files: [
    {
      index: '1',
      path: '/downloads/file.zip',
      length: '100',
      completedLength: status === 'complete' ? '100' : '50',
      selected: 'true',
      uris: [
        { uri: 'https://example.com/file.zip', status: 'used' },
        { uri: 'https://example.com/file.zip', status: 'waiting' },
      ],
    },
  ],
  ...extra,
})

function createApi() {
  return {
    addUriAtomic: vi.fn().mockResolvedValue('new-gid'),
    fetchTaskItem: vi.fn().mockResolvedValue(makeTask('active', { gid: 'new-gid' })),
    getOption: vi.fn().mockResolvedValue({ dir: '/downloads', out: '/downloads/file.zip', continue: 'true' }),
    removeTask: vi.fn().mockResolvedValue('OK'),
    removeTaskRecord: vi.fn().mockResolvedValue('OK'),
  }
}

const history = { removeRecord: vi.fn().mockResolvedValue(undefined) }

describe('resubmitTask', () => {
  beforeEach(() => vi.clearAllMocks())

  it('retries an errored task with continuation and deduplicated mirrors', async () => {
    const api = createApi()
    await resubmitTask(makeTask('error'), 'retry', api, history)

    expect(api.addUriAtomic).toHaveBeenCalledWith({
      uris: ['https://example.com/file.zip'],
      options: {
        dir: '/downloads',
        out: 'file.zip',
        continue: 'true',
        allowOverwrite: 'false',
        autoFileRenaming: 'false',
      },
    })
    expect(api.removeTaskRecord).toHaveBeenCalledWith({ gid: 'old-gid' })
  })

  it.each([
    { path: 'C:\\Downloads\\安装包.1.exe', dir: 'C:\\Downloads', out: '安装包.1.exe' },
    { path: '\\\\server\\share\\file.zip', dir: '\\\\server\\share\\', out: 'file.zip' },
  ])('re-downloads $path using a filename and preserves request options', async ({ path, dir, out }) => {
    const api = createApi()
    const task = makeTask('complete', { dir })
    task.files[0].path = path
    api.getOption.mockResolvedValue({ dir, out: path, header: 'Authorization: test', userAgent: 'test-agent' })
    await resubmitTask(task, 'redownload', api, history)

    expect(api.addUriAtomic.mock.calls[0][0].options).toMatchObject({
      dir,
      out,
      header: 'Authorization: test',
      userAgent: 'test-agent',
      continue: 'false',
      allowOverwrite: 'false',
      autoFileRenaming: 'true',
    })
  })

  it('restores the saved filename when the engine no longer has the old GID', async () => {
    const api = createApi()
    api.getOption.mockRejectedValue(new Error('GID not found'))
    const task = makeTask('complete')
    task.files[0].path = 'C:\\Downloads\\100% complete.zip'
    await resubmitTask(task, 'redownload', api, history)

    expect(api.addUriAtomic.mock.calls[0][0].options).toMatchObject({
      dir: 'C:\\Downloads',
      out: '100% complete.zip',
      allowOverwrite: 'false',
    })
  })

  it('retains a configured filename if the task failed before resolving its path', async () => {
    const api = createApi()
    api.getOption.mockResolvedValue({ dir: '/downloads', out: 'chosen.zip' })
    const task = makeTask('error')
    task.files[0].path = ''
    await resubmitTask(task, 'retry', api, history)

    expect(api.addUriAtomic.mock.calls[0][0].options).toMatchObject({ dir: '/downloads', out: 'chosen.zip' })
  })

  it('keeps the old record and rolls back when the submitted task is already terminal', async () => {
    const api = createApi()
    api.fetchTaskItem.mockResolvedValue(makeTask('error', { gid: 'new-gid', errorMessage: 'rejected' }))

    await expect(resubmitTask(makeTask('complete'), 'redownload', api, history)).rejects.toThrow('rejected')
    expect(api.removeTask).toHaveBeenCalledWith({ gid: 'new-gid' })
    expect(api.removeTaskRecord).not.toHaveBeenCalled()
    expect(history.removeRecord).not.toHaveBeenCalled()
  })
})
