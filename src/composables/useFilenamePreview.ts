/** @fileoverview Suggested names and explicit, cancellable HTTP filename inspection. */
import { computed, onBeforeUnmount, ref, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { extractDecodedFilename, normalizeUriLines, safeFilename } from '@shared/utils/batchHelpers'
import { logger } from '@shared/logger'
import { getErrorMessage } from '@shared/utils/errorMessage'
import { buildTaskProxyOptions, buildProxyUrlWithCredentials } from '@shared/utils/proxy'
import type { AddTaskForm } from './useAddTaskSubmit'

export function useFilenamePreview(
  form: Ref<AddTaskForm>,
  visible: () => boolean,
  reportError: (message: string) => void,
) {
  const suggested = ref('')
  const edited = ref(false)
  const pending = ref<string | null>(null)
  const singleUrl = computed(() => {
    const urls = normalizeUriLines(form.value.uris)
    return urls.length === 1 && /^https?:\/\//i.test(urls[0]) ? urls[0] : ''
  })
  const extensionOf = (name: string) => /\.[A-Za-z0-9]{1,8}$/.exec(name)?.[0] ?? ''
  /** The detected extension a typed name without one keeps. */
  const missingExtension = computed(() =>
    edited.value && form.value.out.trim() && !extensionOf(form.value.out.trim()) ? extensionOf(suggested.value) : '',
  )
  const clean = (name: string) => safeFilename(name.replace(/^.*[/\\]/, ''))
  function cancel() {
    const id = pending.value
    pending.value = null
    if (id) void invoke('cancel_filename_probe', { id }).catch((error) => logger.debug('Filename.cancel', error))
  }
  watch(
    [visible, singleUrl, () => form.value.uriRequestContexts],
    () => {
      cancel()
      if (!visible()) {
        edited.value = false
        suggested.value = ''
        return
      }
      const context = form.value.uriRequestContexts?.[singleUrl.value]
      suggested.value = clean(context?.filename || extractDecodedFilename(singleUrl.value) || '')
    },
    { immediate: true },
  )
  onBeforeUnmount(cancel)
  const filename = computed({
    get: () => (edited.value ? form.value.out : suggested.value),
    set: (value: string) => {
      cancel()
      edited.value = true
      form.value.out = value
    },
  })
  async function resolve() {
    if (!singleUrl.value || pending.value) return
    const id = crypto.randomUUID()
    pending.value = id
    const context = form.value.uriRequestContexts?.[singleUrl.value]
    const value = form.value
    const proxy = buildTaskProxyOptions(
      value.proxyMode,
      value.customProxy,
      value.appProxy,
      value.customProxyUsername,
      value.customProxyPassword,
    )
    try {
      const name = await invoke<string>('resolve_remote_filename', {
        request: {
          id,
          url: singleUrl.value,
          proxy: buildProxyUrlWithCredentials({
            server: typeof proxy['all-proxy'] === 'string' ? proxy['all-proxy'] : undefined,
            username: typeof proxy['all-proxy-user'] === 'string' ? proxy['all-proxy-user'] : undefined,
            password: typeof proxy['all-proxy-passwd'] === 'string' ? proxy['all-proxy-passwd'] : undefined,
          }),
          referer: context?.referer || value.referer || null,
          cookie: context?.cookie || value.cookie || null,
          userAgent: context?.userAgent || value.userAgent || null,
          requestHeaders: context?.requestHeaders ?? value.requestHeaders,
        },
      })
      if (pending.value !== id) return
      suggested.value = clean(name)
      form.value.out = ''
      edited.value = false
    } catch (error) {
      if (pending.value === id) reportError(getErrorMessage(error))
    } finally {
      if (pending.value === id) pending.value = null
    }
  }
  return {
    filename,
    missingExtension,
    /** The name to submit: the typed name, with the detected extension when it has none. */
    outputName: () => (form.value.out.trim() ? form.value.out.trim() + missingExtension.value : form.value.out),
    resolvingFilename: computed(() => pending.value !== null),
    canResolveFilename: computed(() => !!singleUrl.value),
    resolveFilename: resolve,
  }
}
