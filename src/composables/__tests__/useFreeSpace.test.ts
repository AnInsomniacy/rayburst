import { describe, expect, it } from 'vitest'
import { knownBatchSize } from '../useFreeSpace'
import type { BatchItem } from '@shared/types'

const item = (overrides: Partial<BatchItem>): BatchItem => ({
  id: crypto.randomUUID(),
  kind: 'uri',
  source: '',
  displayName: '',
  payload: '',
  status: 'pending',
  ...overrides,
})

const torrent = (selectedFileIndices?: number[]) =>
  item({
    kind: 'torrent',
    selectedFileIndices,
    torrentMeta: {
      name: 'set',
      mode: 'multi',
      infoHashV1: '',
      infoHashV2: '',
      totalLength: '600',
      files: [
        { index: '1', path: 'a', length: '100' },
        { index: '2', path: 'b', length: '200' },
        { index: '3', path: 'c', length: '300' },
      ],
    },
  })

describe('knownBatchSize', () => {
  it('adds browser-reported sizes and the selected torrent files', () => {
    expect(knownBatchSize([item({ browserContext: { size: 1000 } }), item({}), torrent([1, 3]), torrent()])).toBe(
      1000 + 400 + 600,
    )
  })
})
