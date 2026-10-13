/**
 * @fileoverview Files dragged onto a window: the main window and the add-task
 * window both accept .torrent files and say so while they are dragged over.
 *
 * Each window runs its own page, so the drag state below belongs to the window
 * that loaded this module.
 */
import { readonly, ref } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { logger } from '@shared/logger'
import { createBatchItem, detectKind } from '@shared/utils/batchHelpers'
import type { BatchItem } from '@shared/types'

/** What the files being dragged over the window would do when dropped. */
export type TorrentDragState = 'none' | 'accept' | 'reject'

const dragState = ref<TorrentDragState>('none')

/** Drag state of this window, shown by `TorrentDropOverlay`. */
export const torrentDragState = readonly(dragState)

export interface TorrentDropTarget {
  /** Adds the dropped torrents; returns how many were already pending. */
  enqueue: (items: BatchItem[]) => number
  warn: (message: string) => void
  t: (key: string) => string
}

function isTorrent(path: string): boolean {
  return detectKind(path) === 'torrent'
}

/** Listens for files dragged onto this window and adds the torrents among them. */
export function listenForTorrentDrops({ enqueue, warn, t }: TorrentDropTarget): Promise<UnlistenFn> {
  return getCurrentWebview().onDragDropEvent(({ payload }) => {
    switch (payload.type) {
      case 'enter':
        dragState.value = payload.paths.some(isTorrent) ? 'accept' : 'reject'
        return
      case 'over':
        return
      case 'leave':
        dragState.value = 'none'
        return
      case 'drop': {
        dragState.value = 'none'
        const torrents = payload.paths.filter(isTorrent)
        if (torrents.length < payload.paths.length) warn(t('task.drop-torrent-only'))
        if (torrents.length === 0) return
        logger.info('DragDrop', `dropped ${torrents.length} file(s): [${torrents.join(', ')}]`)
        if (enqueue(torrents.map((path) => createBatchItem('torrent', path))) > 0) warn(t('task.duplicate-task'))
      }
    }
  })
}
