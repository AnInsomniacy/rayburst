import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, nextTick, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import { invoke } from '@tauri-apps/api/core'
import { useDownloadPreview, type DownloadPreview } from '../useDownloadPreview'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

interface Call {
  id: string
  url: string
  resolve: (preview: DownloadPreview) => void
  reject: (error: Error) => void
}
let calls: Call[] = []
const cancelled: string[] = []

beforeEach(() => {
  vi.useFakeTimers()
  calls = []
  cancelled.length = 0
  vi.mocked(invoke).mockImplementation((command, args) => {
    const payload = args as { id?: string; request?: { id: string; url: string } }
    if (command === 'cancel_download_preview') {
      cancelled.push(payload.id ?? '')
      return Promise.resolve()
    }
    return new Promise((resolve, reject) => {
      calls.push({ id: payload.request!.id, url: payload.request!.url, resolve, reject })
    })
  })
})
afterEach(() => vi.useRealTimers())

function setup(urls: string[], known: string[] = []) {
  const uris = ref(urls)
  const shown = ref(true)
  let previews!: ReturnType<typeof useDownloadPreview>
  const wrapper = mount(
    defineComponent({
      setup() {
        previews = useDownloadPreview(
          () => uris.value.map((uri) => ({ uri, options: { 'user-agent': 'UA' } })),
          () => shown.value,
          (uri) => known.includes(uri),
        )
        return () => null
      },
    }),
  )
  return { uris, shown, previews, wrapper }
}

const url = (n: number) => `https://example.test/${n}.zip`

describe('download previews', () => {
  it('wait for typing to settle, read four at a time and remember what they read', async () => {
    const { previews, wrapper } = setup([1, 2, 3, 4, 5].map(url))
    await vi.advanceTimersByTimeAsync(399)
    expect(calls).toHaveLength(0)
    await vi.advanceTimersByTimeAsync(1)
    expect(calls.map((call) => call.url)).toEqual([1, 2, 3, 4].map(url))
    expect(previews.isPreviewing(url(1))).toBe(true)

    calls[0].resolve({ size: 1000, filename: '1.zip' })
    await flushPromises()
    expect(previews.previewOf(url(1))).toEqual({ size: 1000, filename: '1.zip' })
    expect(previews.isPreviewing(url(1))).toBe(false)
    expect(calls.map((call) => call.url)).toEqual([1, 2, 3, 4, 5].map(url))
    wrapper.unmount()
  })

  it('drop a read whose download changed and forget everything once hidden', async () => {
    const { uris, shown, previews, wrapper } = setup([url(1), url(2)])
    await vi.advanceTimersByTimeAsync(400)
    calls[1].resolve({ size: 2, filename: '2.zip' })
    await flushPromises()

    uris.value = [url(2)]
    await nextTick()
    expect(cancelled).toEqual([calls[0].id])
    calls[0].resolve({ size: 1, filename: '1.zip' })
    await flushPromises()
    expect(previews.previewOf(url(1))).toBeUndefined()
    expect(previews.previewOf(url(2))?.size).toBe(2)

    shown.value = false
    await nextTick()
    shown.value = true
    await nextTick()
    expect(previews.previewOf(url(2))).toBeUndefined()
    wrapper.unmount()
  })

  it('read again on request and report why a read failed', async () => {
    // The browser already sized download 2: it is read only on request.
    const { previews, wrapper } = setup([url(1), url(2)], [url(2)])
    await vi.advanceTimersByTimeAsync(400)
    calls[0].reject(new Error('The server answered HTTP 403'))
    await flushPromises()
    expect(previews.previewOf(url(1))).toBeUndefined()
    // A failed read is not repeated on its own.
    await vi.advanceTimersByTimeAsync(1000)
    expect(calls).toHaveLength(1)

    const outcome = previews.refresh(url(1))
    calls[1].resolve({ size: null, filename: 'index.html' })
    expect(await outcome).toEqual({ preview: { size: null, filename: 'index.html' } })
    void previews.refresh(url(2))
    expect(calls.map((call) => call.url)).toEqual([url(1), url(1), url(2)])
    wrapper.unmount()
  })
})
