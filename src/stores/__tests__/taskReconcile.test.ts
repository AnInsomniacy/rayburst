/** @fileoverview Tests for applying snapshots to the rendered task list in place (#799). */
import { describe, it, expect } from 'vitest'
import { effect, reactive } from 'vue'
import type { Aria2Task } from '@shared/types'
import { updateTasksInPlace } from '../task/reconcile'

function makeTask(gid: string, overrides: Partial<Aria2Task> = {}): Aria2Task {
  return {
    gid,
    status: 'active',
    totalLength: '100',
    completedLength: '10',
    uploadLength: '0',
    downloadSpeed: '5',
    uploadSpeed: '0',
    connections: '1',
    dir: '/downloads',
    files: [
      { index: '1', path: `/downloads/${gid}`, length: '100', completedLength: '10', selected: 'true', uris: [] },
    ],
    ...overrides,
  }
}

describe('updateTasksInPlace', () => {
  it('keeps the list and its tasks while only progress changes', () => {
    const list = reactive([makeTask('a'), makeTask('b')])
    const first = list[0]
    let listRuns = 0
    let progressRuns = 0
    effect(() => {
      listRuns++
      void list.map((task) => task.gid)
    })
    effect(() => {
      progressRuns++
      void list[0].completedLength
    })

    expect(updateTasksInPlace(list, [makeTask('a', { completedLength: '20' }), makeTask('b')])).toBe(true)
    expect(list[0]).toBe(first)
    expect(list[0].completedLength).toBe('20')
    expect(progressRuns).toBe(2)
    expect(listRuns).toBe(1)
  })

  it('notifies nothing for an identical snapshot', () => {
    const list = reactive([makeTask('a')])
    let runs = 0
    effect(() => {
      runs++
      void JSON.stringify(list)
    })
    expect(updateTasksInPlace(list, [makeTask('a')])).toBe(true)
    expect(runs).toBe(1)
  })

  it('drops fields the new snapshot no longer carries', () => {
    const list = reactive([makeTask('a', { errorMessage: 'timeout' })])
    updateTasksInPlace(list, [makeTask('a')])
    expect('errorMessage' in list[0]).toBe(false)
  })

  it('leaves a reordered or changed list to the caller', () => {
    const list = reactive([makeTask('a'), makeTask('b')])
    expect(updateTasksInPlace(list, [makeTask('b'), makeTask('a')])).toBe(false)
    expect(updateTasksInPlace(list, [makeTask('a')])).toBe(false)
    expect(list.map((task) => task.gid)).toEqual(['a', 'b'])
  })

  it('swaps shared frozen tasks instead of writing into them', () => {
    const restored = Object.freeze(makeTask('a', { status: 'complete' }))
    const list = reactive<Aria2Task[]>([restored])
    const live = makeTask('a')
    expect(updateTasksInPlace(list, [live])).toBe(true)
    expect(restored.status).toBe('complete')
    expect(list[0].status).toBe('active')
  })
})
