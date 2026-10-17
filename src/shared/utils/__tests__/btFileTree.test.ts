/** @fileoverview Tests for the BitTorrent file tree and type groups. */
import { describe, expect, it } from 'vitest'
import {
  btFileType,
  buildBtFileTree,
  groupBtFilesByType,
  invertBtFileSelection,
  toggleBtFileGroup,
} from '../btFileTree'
import type { BtFileSelectionItem } from '@shared/types'

function file(index: number, path: string, length = 10): BtFileSelectionItem {
  return { index, path, name: path.split(/[/\\]/).pop() ?? path, length }
}

describe('buildBtFileTree', () => {
  it('drops the folders every file shares, including an absolute save path', () => {
    const tree = buildBtFileTree([
      file(1, 'C:\\Downloads\\Show\\S01\\e01.mkv'),
      file(2, 'C:\\Downloads\\Show\\S02\\e01.mkv'),
      file(3, 'C:\\Downloads\\Show\\readme.txt'),
    ])
    expect(tree.map((row) => row.name)).toEqual(['S01', 'S02', 'readme.txt'])
    expect(tree[0]).toMatchObject({ key: 'dir:S01', length: 10, children: [{ key: 1, name: 'e01.mkv' }] })
    expect(tree[2]).toMatchObject({ key: 3, index: 3 })
  })

  it('puts folders first, sums their sizes and merges single-folder chains', () => {
    const tree = buildBtFileTree([
      file(1, 'Pack/notes.txt', 1),
      file(2, 'Pack/disc/a/b/track1.flac', 5),
      file(3, 'Pack/disc/a/b/track2.flac', 7),
      file(4, 'Pack/extra/cover.jpg', 2),
    ])
    expect(tree.map((row) => row.name)).toEqual(['disc/a/b', 'extra', 'notes.txt'])
    expect(tree[0]).toMatchObject({ key: 'dir:disc/a/b', length: 12 })
    expect(tree[0].children?.map((row) => row.key)).toEqual([2, 3])
  })

  it('keeps a single file as a flat row', () => {
    expect(buildBtFileTree([file(1, 'movie.mp4')])).toEqual([
      { key: 1, name: 'movie.mp4', path: 'movie.mp4', length: 10, index: 1 },
    ])
  })
})

describe('file type groups', () => {
  const files = [file(1, 'a/clip.MKV'), file(2, 'a/song.flac'), file(3, 'a/clip2.mp4'), file(4, 'a/LICENSE')]

  it('classifies by built-in category extensions', () => {
    expect(btFileType('clip.MKV')).toBe('file-category-videos')
    expect(btFileType('LICENSE')).toBeNull()
    expect(btFileType('.hidden')).toBeNull()
  })

  it('files parts of split archives with the archive', () => {
    expect(btFileType('backup.7z.001')).toBe('file-category-archives')
    expect(btFileType('disk.z01')).toBe('file-category-archives')
    expect(btFileType('disk.R00')).toBe('file-category-archives')
    expect(btFileType('track.001')).toBeNull()
  })

  it('groups indices in category order and skips empty categories', () => {
    expect(groupBtFilesByType(files)).toEqual([
      { type: 'file-category-videos', indices: [1, 3] },
      { type: 'file-category-music', indices: [2] },
    ])
  })

  it('toggles a whole group and inverts a selection', () => {
    expect(toggleBtFileGroup([2], [1, 3])).toEqual([1, 2, 3])
    expect(toggleBtFileGroup([1, 2, 3], [1, 3])).toEqual([2])
    expect(invertBtFileSelection(files, [1, 2])).toEqual([3, 4])
  })
})
