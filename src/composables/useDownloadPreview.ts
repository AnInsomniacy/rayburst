/**
 * @fileoverview Size and name of the downloads in the add dialog, read ahead
 * from their first byte while the user looks at them.
 */
import { computed, onBeforeUnmount, shallowRef, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { logger } from '@shared/logger'
import { getErrorMessage } from '@shared/utils/errorMessage'
import type { DownloadPreviewRequest } from './useAddTaskSubmit'

export interface DownloadPreview {
  /** Bytes the download would write, when the server says. */
  size: number | null
  /** The name the engine would give the file. */
  filename: string
}

export interface PreviewOutcome {
  preview?: DownloadPreview
  error?: string
}

/** Typing settles before anything is requested. */
const SETTLE_MS = 400
/** As many as the app looks at together. */
const CONCURRENCY = 4
/** A long list is not looked at beyond its start. */
const LIMIT = 50

const keyOf = (request: DownloadPreviewRequest) => `${request.uri}\n${JSON.stringify(request.options)}`

/** `known` downloads are read only when asked to. */
export function useDownloadPreview(
  requests: () => DownloadPreviewRequest[],
  active: () => boolean,
  known: (uri: string) => boolean = () => false,
) {
  const wanted = computed(() =>
    active()
      ? requests()
          .slice(0, LIMIT)
          .map((request) => ({ key: keyOf(request), request }))
      : [],
  )
  const keys = computed(() => new Map(wanted.value.map(({ key, request }) => [request.uri, key])))
  const outcomes = shallowRef(new Map<string, PreviewOutcome>())
  const running = shallowRef(new Map<string, string>())
  let timer: ReturnType<typeof setTimeout> | undefined
  let settling = false

  function settle(key: string, outcome: PreviewOutcome) {
    outcomes.value = new Map(outcomes.value).set(key, outcome)
  }

  function stop(key: string) {
    const id = running.value.get(key)
    if (!id) return
    const next = new Map(running.value)
    next.delete(key)
    running.value = next
    void invoke('cancel_download_preview', { id }).catch((error) => logger.debug('DownloadPreview.cancel', error))
  }

  async function run(key: string, request: DownloadPreviewRequest): Promise<PreviewOutcome | undefined> {
    const id = crypto.randomUUID()
    running.value = new Map(running.value).set(key, id)
    let outcome: PreviewOutcome
    try {
      outcome = {
        preview: await invoke<DownloadPreview>('preview_download', {
          request: { id, url: request.uri, options: request.options },
        }),
      }
    } catch (error) {
      outcome = { error: getErrorMessage(error) }
    }
    // Cancelled: the download changed or the dialog went away.
    if (running.value.get(key) !== id) return undefined
    const next = new Map(running.value)
    next.delete(key)
    running.value = next
    settle(key, outcome)
    if (!settling) pump()
    return outcome
  }

  function pump() {
    for (const { key, request } of wanted.value) {
      if (running.value.size >= CONCURRENCY) return
      if (!outcomes.value.has(key) && !running.value.has(key) && !known(request.uri)) void run(key, request)
    }
  }

  watch(
    () => wanted.value.map(({ key }) => key),
    (next) => {
      clearTimeout(timer)
      const kept = new Set(next)
      for (const key of running.value.keys()) if (!kept.has(key)) stop(key)
      if (!active()) {
        settling = false
        outcomes.value = new Map()
        return
      }
      settling = true
      timer = setTimeout(() => {
        settling = false
        pump()
      }, SETTLE_MS)
    },
    { immediate: true },
  )

  onBeforeUnmount(() => {
    clearTimeout(timer)
    for (const key of running.value.keys()) stop(key)
  })

  return {
    /** What the download from `uri` would receive, once read. */
    previewOf: (uri: string) => outcomes.value.get(keys.value.get(uri) ?? '')?.preview,
    /** Whether `uri` is being read now. */
    isPreviewing: (uri: string) => running.value.has(keys.value.get(uri) ?? ''),
    /** Reads `uri` again at once, for an explicit request. */
    refresh(uri: string): Promise<PreviewOutcome | undefined> {
      const entry = wanted.value.find(({ request }) => request.uri === uri)
      if (!entry) return Promise.resolve(undefined)
      stop(entry.key)
      return run(entry.key, entry.request)
    },
  }
}

export type DownloadPreviews = ReturnType<typeof useDownloadPreview>
