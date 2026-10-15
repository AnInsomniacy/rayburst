<script setup lang="ts">
/**
 * @fileoverview The floating orb (#669, #718, #402).
 *
 * A 44 px orb that stays on top. At rest it shows the Rayburst mark; while
 * downloading, the overall speed, ringed by the overall progress. It pulses
 * when a download starts, shows a check when the last one completes, a pause
 * sign when everything waits paused, and a dot after an error. A click opens
 * the panel beside it, dragging moves it, and its context menu reaches the main
 * window and hides it. The window never changes its size.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Menu } from '@tauri-apps/api/menu'
import { LogicalSize } from '@tauri-apps/api/window'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { NIcon } from 'naive-ui'
import { Checkmark, Pause } from '@vicons/ionicons5'
import aria2Api from '@/api/aria2'
import { useFloatUpdates, type FloatUpdate } from '@/composables/useFloatUpdates'
import { usePreferenceStore } from '@/stores/preference'
import { TASK_STATUS } from '@shared/constants'
import { bytesToSize } from '@shared/utils'
import { getErrorMessage } from '@shared/utils/errorMessage'
import { logger } from '@shared/logger'

/** Pointer travel that turns a press into a drag, in CSS pixels. */
const DRAG_THRESHOLD = 4
/** How long the check stays after the last download completed. */
const DONE_MS = 1600
/** Length of the pulse greeting a new download. */
const PULSE_MS = 700
const RING_RADIUS = 20
const RING_LENGTH = 2 * Math.PI * RING_RADIUS

const { t } = useI18n()
const preferenceStore = usePreferenceStore()
const view = getCurrentWebviewWindow()
const root = ref<HTMLElement | null>(null)
const pulsing = ref(false)
const done = ref(false)
const failed = ref(false)
const leaving = ref(false)
const unlisteners: UnlistenFn[] = []
let known: Set<string> | null = null
let pulseTimer: ReturnType<typeof setTimeout> | undefined
let doneTimer: ReturnType<typeof setTimeout> | undefined
let press: { x: number; y: number } | null = null

const { update } = useFloatUpdates(greetNewDownloads)

const zoom = computed(() => preferenceStore.config.uiScale || 1)
const active = computed(() => (update.value?.stat.numActive ?? 0) > 0)
type Mode = 'idle' | 'active' | 'paused' | 'done'
const mode = computed<Mode>(() => {
  if (active.value) return 'active'
  if (done.value) return 'done'
  const tasks = update.value?.tasks ?? []
  const paused = tasks.some((task) => task.status === TASK_STATUS.PAUSED)
  return paused && !tasks.some((task) => task.status === TASK_STATUS.WAITING) ? 'paused' : 'idle'
})
/** Sharing alone shows what goes up. */
const uploading = computed(() => {
  const stat = update.value?.stat
  return !!stat && stat.downloadSpeed === 0 && stat.uploadSpeed > 0
})
const speed = computed(() => {
  const stat = update.value?.stat
  const [value = '0', unit = ''] = bytesToSize(
    uploading.value ? (stat?.uploadSpeed ?? 0) : (stat?.downloadSpeed ?? 0),
  ).split(' ')
  return { value, unit: `${uploading.value ? '↑ ' : ''}${unit}/s` }
})
/** Downloads without a measurable size turn an arc instead. */
const indeterminate = computed(() => mode.value === 'active' && update.value?.progress == null)
const ringOffset = computed(() => {
  if (mode.value === 'done') return 0
  if (mode.value !== 'active') return RING_LENGTH
  if (indeterminate.value) return RING_LENGTH * 0.75
  return RING_LENGTH * (1 - Math.min(update.value?.progress ?? 0, 100) / 100)
})
const tooltip = computed(() => {
  const stat = update.value?.stat
  if (!stat) return 'Rayburst'
  const progress = update.value?.progress
  return [
    `↓ ${bytesToSize(stat.downloadSpeed)}/s   ↑ ${bytesToSize(stat.uploadSpeed)}/s`,
    stat.numActive > 0 ? `${t('task.status-active')} ${stat.numActive}` : '',
    progress != null && stat.numActive > 0 ? `${progress}%` : '',
  ]
    .filter(Boolean)
    .join(' · ')
})

function greetNewDownloads(next: FloatUpdate) {
  const started = next.tasks.filter((task) => task.status === TASK_STATUS.ACTIVE).map((task) => task.gid)
  if (known && started.some((gid) => !known?.has(gid))) {
    pulsing.value = false
    requestAnimationFrame(() => (pulsing.value = true))
    clearTimeout(pulseTimer)
    pulseTimer = setTimeout(() => (pulsing.value = false), PULSE_MS)
  }
  if (started.length > 0) done.value = false
  known = new Set(started)
}

function celebrate() {
  done.value = true
  clearTimeout(doneTimer)
  doneTimer = setTimeout(() => (done.value = false), DONE_MS)
}

/** Fits the window to the orb at the current interface size. */
function fit() {
  const element = root.value
  if (!element) return
  const rect = element.getBoundingClientRect()
  view
    .setSize(new LogicalSize(Math.ceil(rect.width * zoom.value), Math.ceil(rect.height * zoom.value)))
    .catch((error: unknown) => logger.debug('FloatWindow.fit', getErrorMessage(error)))
}

const resizeObserver = new ResizeObserver(fit)

// ── Pointer ─────────────────────────────────────────────────────────

function onPointerDown(event: PointerEvent) {
  if (event.button !== 0) return
  press = { x: event.screenX, y: event.screenY }
}

function onPointerMove(event: PointerEvent) {
  if (!press) return
  if (Math.hypot(event.screenX - press.x, event.screenY - press.y) < DRAG_THRESHOLD) return
  press = null
  // The panel opens beside the orb's new place next time.
  void invoke('float_panel_dismiss').catch(() => undefined)
  view.startDragging().catch((error: unknown) => logger.debug('FloatWindow.drag', getErrorMessage(error)))
}

function onPointerUp() {
  if (!press) return
  press = null
  togglePanel()
}

function togglePanel() {
  failed.value = false
  invoke('float_panel_toggle').catch((error: unknown) => logger.warn('FloatWindow.panel', getErrorMessage(error)))
}

// ── Actions ─────────────────────────────────────────────────────────

function openMain() {
  invoke('float_window_open_main').catch((error: unknown) => logger.warn('FloatWindow.open', getErrorMessage(error)))
}

function toggleAll() {
  const action = active.value ? aria2Api.forcePauseAll() : aria2Api.resumeEligible()
  action.catch((error: unknown) => logger.warn('FloatWindow.toggleAll', getErrorMessage(error)))
}

/** Turns the preference off; the desktop closes the window on its next tick. */
async function hide() {
  if (!(await preferenceStore.updateAndSave({ showFloatWindow: false }))) return
  leaving.value = true
  await emit('float-window:hidden').catch(() => undefined)
}

async function showMenu(event: MouseEvent) {
  event.preventDefault()
  try {
    const menu = await Menu.new({
      items: [
        { id: 'show', text: t('app.show'), action: openMain },
        {
          id: 'all',
          text: t(active.value ? 'task.pause-all-task' : 'task.resume-all-task'),
          action: toggleAll,
        },
        { item: 'Separator' },
        { id: 'hide', text: t('preferences.float-window-hide'), action: () => void hide() },
      ],
    })
    await menu.popup()
  } catch (error) {
    logger.warn('FloatWindow.menu', getErrorMessage(error))
  }
}

onMounted(async () => {
  document.documentElement.classList.add('float-window-document')
  if (root.value) resizeObserver.observe(root.value)
  unlisteners.push(
    await view.listen('float:leave', () => (leaving.value = true)),
    // Lifecycle events reach every window.
    await listen('task-monitor:complete', celebrate),
    await listen('task-monitor:p2p-download-complete', celebrate),
    await listen('task-monitor:error', () => (failed.value = true)),
  )
})

onBeforeUnmount(() => {
  clearTimeout(pulseTimer)
  clearTimeout(doneTimer)
  resizeObserver.disconnect()
  for (const unlisten of unlisteners) unlisten()
})
</script>

<template>
  <div ref="root" class="float-window">
    <Transition name="orb" appear>
      <div
        v-if="update && !leaving"
        class="orb"
        :class="[`orb--${mode}`, { 'orb--pulse': pulsing, 'orb--spinning': indeterminate }]"
        role="button"
        tabindex="0"
        :title="tooltip"
        :aria-label="tooltip"
        @pointerdown="onPointerDown"
        @pointermove="onPointerMove"
        @pointerup="onPointerUp"
        @keydown.enter.prevent="togglePanel"
        @contextmenu="showMenu"
      >
        <svg class="orb-ring" viewBox="0 0 44 44" aria-hidden="true">
          <defs>
            <linearGradient id="orb-ring-gradient" x1="0" y1="0" x2="1" y2="1">
              <stop class="orb-ring-stop" offset="0" />
              <stop class="orb-ring-stop orb-ring-stop--end" offset="1" />
            </linearGradient>
          </defs>
          <circle class="orb-ring-track" cx="22" cy="22" :r="RING_RADIUS" />
          <circle
            class="orb-ring-value"
            cx="22"
            cy="22"
            :r="RING_RADIUS"
            :stroke-dasharray="RING_LENGTH"
            :stroke-dashoffset="ringOffset"
          />
        </svg>

        <Transition name="orb-face">
          <img v-if="mode === 'idle'" key="idle" class="orb-face orb-logo" src="/logo.svg" alt="" draggable="false" />
          <span v-else-if="mode === 'active'" key="active" class="orb-face orb-speed">
            <span class="orb-speed-value" :data-length="speed.value.length">{{ speed.value }}</span>
            <span class="orb-speed-unit">{{ speed.unit }}</span>
          </span>
          <NIcon v-else-if="mode === 'paused'" key="paused" class="orb-face orb-symbol" :size="16"><Pause /></NIcon>
          <NIcon v-else key="done" class="orb-face orb-symbol orb-symbol--done" :size="20"><Checkmark /></NIcon>
        </Transition>

        <Transition name="orb-dot">
          <span v-if="failed" class="orb-dot" />
        </Transition>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
/* The margin holds the orb's shadow and its hover growth inside the window. */
.float-window {
  display: inline-flex;
  padding: 8px;
  user-select: none;
}

.orb {
  position: relative;
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background:
    radial-gradient(120% 90% at 50% 0%, color-mix(in srgb, var(--m3-primary) 7%, transparent), transparent 62%),
    var(--main-bg);
  box-shadow:
    0 0 0 1px color-mix(in srgb, var(--m3-outline-variant) 80%, transparent),
    0 1px 2px var(--m3-shadow),
    0 4px 10px -2px var(--m3-shadow);
  color: var(--m3-on-surface);
  cursor: default;
  outline: none;
  transition:
    transform 220ms var(--dialog-ease-enter),
    box-shadow 220ms var(--dialog-ease-enter);
}

.orb:hover {
  transform: scale(1.06);
  box-shadow:
    0 0 0 1px color-mix(in srgb, var(--m3-outline-variant) 80%, transparent),
    0 2px 4px var(--m3-shadow),
    0 6px 14px -2px var(--m3-shadow);
}

.orb:active {
  transform: scale(0.96);
  transition-duration: 100ms;
}

.orb:focus-visible {
  box-shadow:
    0 0 0 2px var(--m3-primary),
    0 4px 10px -2px var(--m3-shadow);
}

/* ── Ring ─────────────────────────────────────────────────────────── */

.orb-ring {
  position: absolute;
  inset: 0;
  transform: rotate(-90deg);
  pointer-events: none;
}

.orb-ring circle {
  fill: none;
  stroke-width: 2.5;
}

.orb-ring-stop {
  stop-color: var(--m3-primary);
}

.orb-ring-stop--end {
  stop-color: color-mix(in srgb, var(--m3-primary) 55%, var(--m3-tertiary));
}

.orb-ring-track {
  stroke: color-mix(in srgb, var(--m3-outline-variant) 60%, transparent);
  opacity: 0;
  transition: opacity 280ms var(--dialog-ease-enter);
}

.orb--active .orb-ring-track,
.orb--paused .orb-ring-track {
  opacity: 1;
}

.orb--paused .orb-ring-track {
  stroke: var(--m3-outline);
  stroke-dasharray: 2.5 3.5;
}

.orb-ring-value {
  stroke: url(#orb-ring-gradient);
  stroke-linecap: round;
  transition:
    stroke-dashoffset var(--task-motion-progress) var(--task-motion-ease),
    stroke 280ms ease,
    opacity 280ms ease;
}

.orb--idle .orb-ring-value,
.orb--paused .orb-ring-value {
  opacity: 0;
  transition:
    stroke-dashoffset 320ms var(--dialog-ease-leave),
    opacity 320ms ease;
}

.orb--done .orb-ring-value {
  stroke: var(--m3-success);
}

.orb--spinning .orb-ring {
  animation: orb-spin 1.4s linear infinite;
}

@keyframes orb-spin {
  to {
    transform: rotate(270deg);
  }
}

/* ── Face ─────────────────────────────────────────────────────────── */

.orb-face {
  grid-area: 1 / 1;
  pointer-events: none;
}

.orb-logo {
  width: 24px;
  height: 24px;
}

.orb-speed {
  display: grid;
  justify-items: center;
  line-height: 1;
}

.orb-speed-value {
  font-size: 13px;
  font-weight: 650;
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.02em;
}

.orb-speed-value[data-length='5'] {
  font-size: 11.5px;
}

.orb-speed-unit {
  margin-top: 2px;
  color: var(--m3-on-surface-variant);
  font-size: 8.5px;
  font-weight: 500;
  white-space: nowrap;
}

.orb-symbol {
  color: var(--m3-on-surface-variant);
}

.orb-symbol--done {
  color: var(--m3-success);
}

.orb-face-enter-active {
  transition:
    opacity 280ms var(--dialog-ease-enter),
    transform 280ms var(--dialog-ease-enter);
}

.orb-face-leave-active {
  transition:
    opacity 180ms var(--dialog-ease-leave),
    transform 180ms var(--dialog-ease-leave);
}

.orb-face-enter-from {
  opacity: 0;
  transform: translateY(6px);
}

.orb-face-leave-to {
  opacity: 0;
  transform: scale(0.6);
}

/* The mark grows back from the middle rather than rising. */
.orb-logo.orb-face-enter-from {
  transform: scale(0.6);
}

/* ── Pulse, error dot, arrival ────────────────────────────────────── */

.orb--pulse {
  animation: orb-pulse 700ms var(--dialog-ease-enter);
}

@keyframes orb-pulse {
  0% {
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--m3-outline-variant) 80%, transparent),
      0 0 0 0 color-mix(in srgb, var(--m3-primary) 45%, transparent),
      0 4px 10px -2px var(--m3-shadow);
  }
  35% {
    transform: scale(1.08);
  }
  100% {
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--m3-outline-variant) 80%, transparent),
      0 0 0 8px color-mix(in srgb, var(--m3-primary) 0%, transparent),
      0 4px 10px -2px var(--m3-shadow);
  }
}

.orb-dot {
  position: absolute;
  top: 1px;
  right: 1px;
  width: 9px;
  height: 9px;
  border: 2px solid var(--main-bg);
  border-radius: 50%;
  background: var(--m3-error);
}

.orb-dot-enter-active,
.orb-dot-leave-active {
  transition: transform 200ms var(--dialog-ease-enter);
}

.orb-dot-enter-from,
.orb-dot-leave-to {
  transform: scale(0);
}

.orb-enter-active {
  transition:
    opacity 200ms var(--dialog-ease-enter),
    transform 200ms var(--dialog-ease-enter);
}

.orb-leave-active {
  transition:
    opacity 150ms var(--dialog-ease-leave),
    transform 150ms var(--dialog-ease-leave);
}

.orb-enter-from,
.orb-leave-to {
  opacity: 0;
  transform: scale(0.8);
}
</style>
