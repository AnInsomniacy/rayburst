/**
 * @fileoverview Composable for managing per-task options in the Task Detail drawer.
 *
 * Reads the current task's options via `getTaskOption`, exposes a reactive form
 * for UA, referer, cookie, authorization, and proxy,
 * and applies changes via `changeTaskOption`.
 *
 * Native request options use the engine's pending-option restart path; speed
 * limits apply at once. Media options are only editable while paused to
 * preserve recording continuity. The source URL of a single-file download can
 * be replaced while it is unfinished, or used to retry a failed one.
 *
 * Pure dependency-injection design — no direct store/API imports — fully testable.
 */
import { ref, reactive, computed, watch, type Ref } from 'vue'
import { isEngineReady } from '@/api/aria2'
import { sanitizeHeaderValue, sanitizeHttpHeaderOptions } from '@shared/utils/headerSanitize'
import { TASK_STATUS } from '@shared/constants'
import type { Aria2Task, Aria2EngineOptions, ProxyConfig } from '@shared/types'
import { logger } from '@shared/logger'
import { buildTaskProxyOptions, hasInvalidManualProxy, type TaskProxyMode } from '@shared/utils/proxy'
import { formatSpeedLimit, parseSpeedLimit } from '@shared/utils/speedLimit'
import { checkTaskIsBT } from '@shared/utils'

// ── Constants ─────────────────────────────────────────────────────

/** Task statuses where aria2 allows option modification. */
const MODIFIABLE_STATUSES = new Set([TASK_STATUS.ACTIVE, TASK_STATUS.WAITING, TASK_STATUS.PAUSED])
const REPLACEABLE_SOURCE = /^(https?|ftp|sftp):\/\//i

// ── Public types ──────────────────────────────────────────────────

/** Proxy configuration mode for a specific task. */
export type ProxyMode = TaskProxyMode

export interface TaskDetailOptionsForm {
  userAgent: string
  referer: string
  cookie: string
  authorization: string
  httpAuthUsername: string
  httpAuthPassword: string
  proxyMode: ProxyMode
  customProxy: string
  customProxyUsername: string
  customProxyPassword: string
  maxDownloadLimit: string
  maxUploadLimit: string
  uri: string
}

export interface UseTaskDetailOptionsConfig {
  task: Ref<Aria2Task | null>
  getTaskOption: (gid: string) => Promise<Record<string, string>>
  changeTaskOption: (payload: { gid: string; options: Aria2EngineOptions }) => Promise<void>
  /** Replaces the source of an unfinished download, keeping its data. */
  replaceTaskUri: (payload: { gid: string; uri: string }) => Promise<void>
  /** Retries a failed download from another source. */
  retryTaskWithUri: (task: Aria2Task, uri: string) => Promise<void>
  proxyConfig: () => ProxyConfig
  message: {
    success: (content: string) => void
    error: (content: string) => void
  }
  t: (key: string) => string
}

// ── Pure header utilities ─────────────────────────────────────────

/**
 * Extracts Cookie and Authorization values from aria2's `header` option.
 *
 * aria2 may return `header` as a single newline-separated string, an array
 * of strings, or undefined. This function handles all forms gracefully.
 */
export function parseHeaders(raw: string | string[] | undefined): {
  cookie: string
  authorization: string
} {
  const result = { cookie: '', authorization: '' }
  if (!raw) return result

  const lines = Array.isArray(raw) ? raw : raw.split('\n')
  for (const line of lines) {
    const trimmed = line.trim()
    if (/^Cookie:\s*/i.test(trimmed)) {
      result.cookie = trimmed.replace(/^Cookie:\s*/i, '').trim()
    } else if (/^Authorization:\s*/i.test(trimmed)) {
      result.authorization = trimmed.replace(/^Authorization:\s*/i, '').trim()
    }
  }
  return result
}

/**
 * Builds an aria2 `header` array from cookie and authorization values.
 * Returns an empty array when both are empty.
 */
export function buildHeaders(cookie: string, authorization: string): string[] {
  const headers: string[] = []
  const cleanCookie = sanitizeHeaderValue(cookie)
  const cleanAuthorization = sanitizeHeaderValue(authorization)
  if (cleanCookie) headers.push(`Cookie: ${cleanCookie}`)
  if (cleanAuthorization) headers.push(`Authorization: ${cleanAuthorization}`)
  return headers
}

// ── Form snapshot helpers ─────────────────────────────────────────

function createEmptyForm(): TaskDetailOptionsForm {
  return {
    userAgent: '',
    referer: '',
    cookie: '',
    authorization: '',
    httpAuthUsername: '',
    httpAuthPassword: '',
    proxyMode: 'direct',
    customProxy: '',
    customProxyUsername: '',
    customProxyPassword: '',
    maxDownloadLimit: '0',
    maxUploadLimit: '0',
    uri: '',
  }
}

/** The single source of a plain download, which the user may replace. */
export function replaceableSource(task: Aria2Task | null): string {
  if (!task || task.media || checkTaskIsBT(task) || task.files?.length !== 1) return ''
  const uri = task.files[0].uris?.[0]?.uri ?? ''
  return REPLACEABLE_SOURCE.test(uri) ? uri : ''
}

const normalizeLimit = (value: string | undefined) => {
  const { amount, unit } = parseSpeedLimit(value)
  return formatSpeedLimit(amount, unit)
}

function snapshotForm(source: TaskDetailOptionsForm, target: TaskDetailOptionsForm) {
  Object.assign(target, { ...source })
}

// ── Proxy detection ───────────────────────────────────────────────

function detectProxyMode(opts: Record<string, string>): { mode: ProxyMode; custom: string } {
  const allProxy = (opts.allProxy as string) ?? ''
  if (allProxy) return { mode: 'manual', custom: allProxy }
  return { mode: 'direct', custom: '' }
}

// ── Options loader ────────────────────────────────────────────────

function populateFormFromResponse(opts: Record<string, string>, form: TaskDetailOptionsForm) {
  form.userAgent = (opts.userAgent as string) ?? ''
  form.referer = (opts.referer as string) ?? ''

  const headerRaw = opts.header as unknown as string | string[] | undefined
  const parsed = parseHeaders(headerRaw)
  form.cookie = parsed.cookie
  form.authorization = parsed.authorization
  form.httpAuthUsername = (opts.httpUser as string) ?? ''
  form.httpAuthPassword = (opts.httpPasswd as string) ?? ''

  const detected = detectProxyMode(opts)
  form.proxyMode = detected.mode
  form.customProxy = detected.custom
  form.customProxyUsername = (opts.allProxyUser as string) ?? ''
  form.customProxyPassword = (opts.allProxyPasswd as string) ?? ''
  form.maxDownloadLimit = normalizeLimit(opts.maxDownloadLimit)
  form.maxUploadLimit = normalizeLimit(opts.maxUploadLimit)
}

// ── Changed-options diff builder ──────────────────────────────────

function buildChangedOptions(
  form: TaskDetailOptionsForm,
  loaded: TaskDetailOptionsForm,
  proxyOptions: Aria2EngineOptions,
): Aria2EngineOptions {
  const options: Aria2EngineOptions = {}
  const sanitizedHeaders = sanitizeHttpHeaderOptions({
    userAgent: form.userAgent,
    referer: form.referer,
    cookie: form.cookie,
    authorization: form.authorization,
  })

  if (form.userAgent !== loaded.userAgent) {
    options['user-agent'] = sanitizedHeaders.userAgent ?? ''
  }
  if (form.referer !== loaded.referer) {
    options.referer = sanitizedHeaders.referer ?? ''
  }
  if (form.cookie !== loaded.cookie || form.authorization !== loaded.authorization) {
    options.header = buildHeaders(form.cookie, form.authorization)
  }
  if (form.httpAuthUsername !== loaded.httpAuthUsername || form.httpAuthPassword !== loaded.httpAuthPassword) {
    const username = sanitizeHeaderValue(form.httpAuthUsername)
    options['http-user'] = username
    options['http-passwd'] = sanitizeHeaderValue(form.httpAuthPassword)
  }
  if (
    form.proxyMode !== loaded.proxyMode ||
    form.customProxy !== loaded.customProxy ||
    form.customProxyUsername !== loaded.customProxyUsername ||
    form.customProxyPassword !== loaded.customProxyPassword
  ) {
    Object.assign(options, proxyOptions)
  }
  if (form.maxDownloadLimit !== loaded.maxDownloadLimit) options['max-download-limit'] = form.maxDownloadLimit
  if (form.maxUploadLimit !== loaded.maxUploadLimit) options['max-upload-limit'] = form.maxUploadLimit

  return options
}

// ── Composable ────────────────────────────────────────────────────

export function useTaskDetailOptions(config: UseTaskDetailOptionsConfig) {
  const { task, getTaskOption, changeTaskOption, proxyConfig, message, t } = config
  const optionKeys = (Object.keys(createEmptyForm()) as (keyof TaskDetailOptionsForm)[]).filter((key) => key !== 'uri')

  const form = reactive<TaskDetailOptionsForm>(createEmptyForm())
  const loaded = reactive<TaskDetailOptionsForm>(createEmptyForm())
  const applying = ref(false)

  const canModify = computed(() => {
    if (!task.value || !isEngineReady()) return false
    if (task.value.media && task.value.status !== 'paused') return false
    return MODIFIABLE_STATUSES.has(task.value.status)
  })

  /** A failed download may only take another source; it retries from there. */
  const canReplaceUri = computed(
    () =>
      !!replaceableSource(task.value) &&
      isEngineReady() &&
      (MODIFIABLE_STATUSES.has(task.value?.status ?? '') || task.value?.status === TASK_STATUS.ERROR),
  )
  const optionsDirty = computed(() => optionKeys.some((key) => form[key] !== loaded[key]))
  const uriDirty = computed(() => canReplaceUri.value && form.uri.trim() !== loaded.uri)
  const dirty = computed(() => (canModify.value && optionsDirty.value) || uriDirty.value)
  const retriesWithUri = computed(() => uriDirty.value && task.value?.status === TASK_STATUS.ERROR)

  function resetForm() {
    Object.assign(form, createEmptyForm())
    form.uri = replaceableSource(task.value)
    snapshotForm(form, loaded)
  }

  async function loadOptions(gid: string) {
    try {
      const opts = await getTaskOption(gid)
      populateFormFromResponse(opts, form)
      form.uri = replaceableSource(task.value)

      snapshotForm(form, loaded)
    } catch (err) {
      logger.debug('[useTaskDetailOptions] getTaskOption failed', err)
      resetForm()
    }
  }

  watch(
    () => task.value?.gid,
    (gid) => (gid && canModify.value ? void loadOptions(gid) : resetForm()),
    { immediate: true },
  )

  async function applyOptions(): Promise<void> {
    if (applying.value || !task.value || !dirty.value) return
    applying.value = true
    try {
      if (uriDirty.value) {
        const uri = form.uri.trim()
        if (!REPLACEABLE_SOURCE.test(uri)) {
          message.error(t('task.source-url-invalid'))
          return
        }
        if (task.value.status === TASK_STATUS.ERROR) {
          await config.retryTaskWithUri(task.value, uri)
          message.success(t('task.source-url-retried'))
          return
        }
        await config.replaceTaskUri({ gid: task.value.gid, uri })
        form.uri = uri
        loaded.uri = uri
        if (!optionsDirty.value) {
          message.success(t('task.source-url-replaced'))
          return
        }
      }

      const proxyOptions = buildTaskProxyOptions(
        form.proxyMode,
        form.customProxy,
        proxyConfig(),
        form.customProxyUsername,
        form.customProxyPassword,
      )

      // Validate proxy format before sending to aria2 — prevents errorCode=28 crash
      if (hasInvalidManualProxy(proxyOptions)) {
        message.error(t('preferences.invalid-proxy-url'))
        return
      }

      const options = buildChangedOptions(form, loaded, proxyOptions)
      await changeTaskOption({ gid: task.value.gid, options })
      snapshotForm(form, loaded)
      // Speed limits apply at once; request options restart the transfer.
      const restarts = Object.keys(options).some((name) => !name.endsWith('-limit'))
      const key =
        task.value.status === TASK_STATUS.ACTIVE && restarts ? 'task.options-applied-restart' : 'task.options-applied'
      message.success(t(key))
    } catch (err) {
      logger.debug('[useTaskDetailOptions] changeTaskOption failed', err)
      message.error(t('task.options-apply-failed'))
    } finally {
      applying.value = false
    }
  }

  return { form, canModify, canReplaceUri, dirty, retriesWithUri, applying, applyOptions }
}
