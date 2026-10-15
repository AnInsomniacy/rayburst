/** @fileoverview The name a single download will get, as the browser, the server or the user gives it. */
import { computed, ref, watch, type Ref } from 'vue'
import { extractDecodedFilename, normalizeUriLines, safeFilename } from '@shared/utils/batchHelpers'
import type { AddTaskForm } from './useAddTaskSubmit'
import type { DownloadPreviews } from './useDownloadPreview'

export function useFilenamePreview(
  form: Ref<AddTaskForm>,
  visible: () => boolean,
  previews: DownloadPreviews,
  reportError: (message: string) => void,
) {
  /** The server's name from an explicit lookup; it outranks the browser's. */
  const looked = ref('')
  const edited = ref(false)
  const looking = ref(false)
  const singleUrl = computed(() => {
    const urls = normalizeUriLines(form.value.uris)
    return urls.length === 1 && /^(?:https?|thunder):\/\//i.test(urls[0]) ? urls[0] : ''
  })
  const clean = (name: string) => safeFilename(name.replace(/^.*[/\\]/, ''))
  const suggested = computed(() => {
    const url = singleUrl.value
    if (!url) return ''
    const context = form.value.uriRequestContexts?.[url]
    return clean(
      looked.value || context?.filename || previews.previewOf(url)?.filename || extractDecodedFilename(url) || '',
    )
  })
  watch([visible, singleUrl, () => form.value.uriRequestContexts], () => {
    looked.value = ''
    if (!visible()) edited.value = false
  })
  const filename = computed({
    get: () => (edited.value ? form.value.out : suggested.value),
    set: (value: string) => {
      edited.value = true
      form.value.out = value
    },
  })
  async function resolve() {
    const url = singleUrl.value
    if (!url || looking.value) return
    looking.value = true
    try {
      const outcome = await previews.refresh(url)
      if (singleUrl.value !== url || !outcome) return
      if (outcome.error) reportError(outcome.error)
      if (!outcome.preview) return
      looked.value = outcome.preview.filename
      form.value.out = ''
      edited.value = false
    } finally {
      looking.value = false
    }
  }
  return {
    /** The one HTTP or Thunder URL in the form, if there is exactly one. */
    singleUrl,
    filename,
    /** The name to submit: exactly what was typed, extension included. */
    outputName: () => form.value.out.trim(),
    resolvingFilename: looking,
    canResolveFilename: computed(() => !!singleUrl.value),
    resolveFilename: resolve,
  }
}
