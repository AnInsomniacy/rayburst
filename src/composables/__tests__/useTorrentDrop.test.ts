import { beforeEach, describe, expect, it, vi } from 'vitest'
import { listenForTorrentDrops } from '../useTorrentDrop'

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
  const onTorrents = vi.fn()

  beforeEach(async () => {
    onTorrents.mockReset()
    await listenForTorrentDrops(onTorrents)
  })

  it('passes on only the dropped torrent files', () => {
    deliver({ payload: { type: 'drop', paths: ['C:\\a.torrent', 'C:\\b.txt', '/home/c.TORRENT'] } })

    expect(onTorrents).toHaveBeenCalledTimes(1)
    const items = onTorrents.mock.calls[0][0] as Array<{ kind: string; source: string }>
    expect(items.map(({ kind, source }) => [kind, source])).toEqual([
      ['torrent', 'C:\\a.torrent'],
      ['torrent', '/home/c.TORRENT'],
    ])
  })

  it('ignores hovering and drops without torrents', () => {
    deliver({ payload: { type: 'over' } })
    deliver({ payload: { type: 'drop', paths: ['C:\\b.txt'] } })

    expect(onTorrents).not.toHaveBeenCalled()
  })
})
