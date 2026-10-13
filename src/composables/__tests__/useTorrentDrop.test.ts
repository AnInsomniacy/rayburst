import { beforeEach, describe, expect, it, vi } from 'vitest'
import { listenForTorrentDrops, torrentDragState } from '../useTorrentDrop'
import type { BatchItem } from '@shared/types'

type DropEvent = { payload: { type: string; paths?: string[] } }
let deliver: (event: DropEvent) => void = () => undefined

vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: async (handler: (event: DropEvent) => void) => {
      deliver = handler
      return () => undefined
    },
  }),
}))

vi.mock('@shared/logger', () => ({ logger: { info: vi.fn() } }))

describe('torrent drops', () => {
  const enqueue = vi.fn<(items: BatchItem[]) => number>()
  const warn = vi.fn<(message: string) => void>()

  beforeEach(async () => {
    enqueue.mockReset().mockReturnValue(0)
    warn.mockReset()
    await listenForTorrentDrops({ enqueue, warn, t: (key) => key })
  })

  it('adds the dropped torrents and says the other files were left out', () => {
    deliver({ payload: { type: 'drop', paths: ['C:\\a.torrent', 'C:\\b.txt', '/home/c.TORRENT'] } })

    expect(enqueue.mock.calls[0][0].map(({ kind, source }) => [kind, source])).toEqual([
      ['torrent', 'C:\\a.torrent'],
      ['torrent', '/home/c.TORRENT'],
    ])
    expect(warn).toHaveBeenCalledWith('task.drop-torrent-only')
  })

  it('reports torrents that were already pending', () => {
    enqueue.mockReturnValue(1)
    deliver({ payload: { type: 'drop', paths: ['C:\\a.torrent'] } })

    expect(warn).toHaveBeenCalledWith('task.duplicate-task')
  })

  it('tells whether the files dragged over the window can be dropped', () => {
    deliver({ payload: { type: 'enter', paths: ['C:\\a.torrent'] } })
    expect(torrentDragState.value).toBe('accept')
    deliver({ payload: { type: 'leave' } })
    expect(torrentDragState.value).toBe('none')

    deliver({ payload: { type: 'enter', paths: ['C:\\b.txt'] } })
    expect(torrentDragState.value).toBe('reject')
    deliver({ payload: { type: 'drop', paths: ['C:\\b.txt'] } })
    expect(torrentDragState.value).toBe('none')
    expect(enqueue).not.toHaveBeenCalled()
  })
})
