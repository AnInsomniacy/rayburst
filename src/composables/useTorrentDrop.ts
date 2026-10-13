/** @fileoverview Turns .torrent files dropped on the current window into add-task items. */
import { getCurrentWebview } from '@tauri-apps/api/webview'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { logger } from '@shared/logger'
import { createBatchItem, detectKind } from '@shared/utils/batchHelpers'
import type { BatchItem } from '@shared/types'

/**
 * Listens for files dropped on this window and passes on the torrents among them.
 * Both the main window and the add-task window accept drops.
 */
export function listenForTorrentDrops(onTorrents: (items: BatchItem[]) => void): Promise<UnlistenFn> {
  return getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type !== 'drop') return
    const torrents = (event.payload.paths ?? []).filter((path) => detectKind(path) === 'torrent')
    if (torrents.length === 0) return
    logger.info('DragDrop', `dropped ${torrents.length} file(s): [${torrents.join(', ')}]`)
    onTorrents(torrents.map((path) => createBatchItem('torrent', path)))
  })
}
