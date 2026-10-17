<script setup lang="ts">
/**
 * @fileoverview The floating bar (#669, #718, #402, #726).
 *
 * A 132 × 36 window that stays on top; the system rounds and shadows it. Its
 * background fills with the overall progress like a gauge, under the received
 * speed and the percentage while downloading, the sent speed while only
 * seeding, and otherwise what the queue waits for: servers, its turn, or the
 * user. A check follows the last download, the Rayburst mark rests in it, and
 * a dot marks an error. Only bytes arriving move it: a download of unknown
 * size sweeps its fill, everything else holds still. A click opens the panel beside it, dragging moves it, links and
 * torrents dropped on it open the add window, and its context menu reaches the
 * main window and switches when it shows. The window never changes its size.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch, type Component } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Menu } from '@tauri-apps/api/menu'
import { LogicalSize } from '@tauri-apps/api/window'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { NIcon } from 'naive-ui'
import { Add, ArrowDown, ArrowUp, Checkmark, Pause, TimeOutline } from '@vicons/ionicons5'
import aria2Api from '@/api/aria2'
import { useFloatUpdates, type FloatUpdate } from '@/composables/useFloatUpdates'
import { usePlatform } from '@/composables/usePlatform'
import { usePreferenceStore } from '@/stores/preference'
import { FLOAT_WINDOW_MODE_LABELS, FLOAT_WINDOW_MODES, TASK_STATUS } from '@shared/constants'
import { bytesToSize } from '@shared/utils'
import { createBatchItem, detectKind, encodeBase64, normalizeUriLines } from '@shared/utils/batchHelpers'
import { floatBarState, type FloatBarKind } from '@shared/utils/floatBar'
import { detectResource } from '@shared/utils/resource'
import { getErrorMessage } from '@shared/utils/errorMessage'
import { logger } from '@shared/logger'
import type { FloatWindowMode } from '@shared/types'

/** Pointer travel that turns a press into a drag, in CSS pixels. */
const DRAG_THRESHOLD = 4
/** How long the check stays after the last download completed. */
const DONE_MS = 1600
/** Length of the flash greeting a new download. */
const PULSE_MS = 700

type Face = FloatBarKind | 'drop'

const ICONS: Record<Exclude<Face, 'idle'>, Component> = {
  downloading: ArrowDown,
  connecting: ArrowDown,
  seeding: ArrowUp,
  waiting: TimeOutline,
  paused: Pause,
  done: Checkmark,
  drop: Add,
}

const { t } = useI18n()
const preferenceStore = usePreferenceStore()
const { isLinux } = usePlatform()
const view = getCurrentWebviewWindow()
const root = ref<HTMLElement | null>(null)
const pulsing = ref(false)
const done = ref(false)
const failed = ref(false)
/** Something droppable is dragged over the bar. */
const dropping = ref(false)
const unlisteners: UnlistenFn[] = []
let known: Set<string> | null = null
let pulseTimer: ReturnType<typeof setTimeout> | undefined
let doneTimer: ReturnType<typeof setTimeout> | undefined
let press: { x: number; y: number } | null = null

const { update } = useFloatUpdates(greetNewDownloads)

const zoom = computed(() => preferenceStore.config.uiScale || 1)
const state = computed(() => floatBarState(update.value, done.value))
const face = computed<Face>(() => (dropping.value ? 'drop' : state.value.kind))
const active = computed(() => state.value.kind === 'downloading' || state.value.kind === 'connecting')

const speed = computed(() => {
  const [value = '0', unit = ''] = bytesToSize(state.value.speed).split(' ')
  return { value, unit: `${unit}/s` }
})
const showsSpeed = computed(() => face.value === 'downloading' || face.value === 'seeding')
const label = computed(() => {
  switch (face.value) {
    case 'connecting':
      return t('task.connecting')
    case 'waiting':
      return t('task.status-waiting')
    case 'paused':
      return t('task.status-paused')
    case 'done':
      return t('task.status-complete')
    case 'drop':
      return t('task.drop-to-add')
    default:
      return 'Rayburst'
  }
})
/** How far the downloads got; a full fill speaks for itself. */
const aside = computed(() => {
  const { kind, progress } = state.value
  if (face.value === 'drop' || progress == null) return ''
  return kind === 'downloading' || kind === 'connecting' || kind === 'waiting' || kind === 'paused'
    ? `${Math.floor(progress)}%`
    : ''
})
/** Bytes arrive for a download whose size is unknown. */
const sweeping = computed(() => face.value === 'downloading' && state.value.progress == null)
/** The fill in percent; a drop target fills entirely. */
const fill = computed(() => (face.value === 'drop' ? 100 : Math.min(state.value.progress ?? 0, 100)))

const tooltip = computed(() => {
  const stat = update.value?.stat
  if (!stat) return 'Rayburst'
  const { kind, count, progress } = state.value
  return [
    `↓ ${bytesToSize(stat.downloadSpeed)}/s   ↑ ${bytesToSize(stat.uploadSpeed)}/s`,
    active.value ? `${t('task.status-active')} ${count}` : '',
    kind === 'seeding' ? `${t('task.seeding')} ${count}` : '',
    active.value && progress != null ? `${progress}%` : '',
  ]
    .filter(Boolean)
    .join(' · ')
})

/** A torrent that turns to seeding as it completes is not a new download. */
function greetNewDownloads(next: FloatUpdate) {
  const started = next.tasks
    .filter((task) => task.status === TASK_STATUS.ACTIVE && task.seeder !== 'true')
    .map((task) => task.gid)
  if (known && started.some((gid) => !known?.has(gid))) pulse()
  if (started.length > 0) done.value = false
  known = new Set(started)
}

function pulse() {
  pulsing.value = false
  requestAnimationFrame(() => (pulsing.value = true))
  clearTimeout(pulseTimer)
  pulseTimer = setTimeout(() => (pulsing.value = false), PULSE_MS)
}

function celebrate() {
  done.value = true
  clearTimeout(doneTimer)
  doneTimer = setTimeout(() => (done.value = false), DONE_MS)
}

/** Fits the window to the bar at the current interface size. */
function fit() {
  const element = root.value
  if (!element) return
  const rect = element.getBoundingClientRect()
  view
    .setSize(new LogicalSize(Math.ceil(rect.width * zoom.value), Math.ceil(rect.height * zoom.value)))
    .catch((error: unknown) => logger.debug('FloatBar.fit', getErrorMessage(error)))
}

const resizeObserver = new ResizeObserver(fit)
// The interface size scales the bar without changing its CSS size.
watch(zoom, fit)

// ── Pointer ─────────────────────────────────────────────────────────

function onPointerDown(event: PointerEvent) {
  if (event.button !== 0) return
  press = { x: event.screenX, y: event.screenY }
}

function onPointerMove(event: PointerEvent) {
  if (!press) return
  if (Math.hypot(event.screenX - press.x, event.screenY - press.y) < DRAG_THRESHOLD) return
  press = null
  // The panel opens beside the bar's new place next time.
  void invoke('float_panel_dismiss').catch(() => undefined)
  view.startDragging().catch((error: unknown) => logger.debug('FloatBar.drag', getErrorMessage(error)))
}

function onPointerUp() {
  if (!press) return
  press = null
  togglePanel()
}

function togglePanel() {
  failed.value = false
  invoke('float_panel_toggle').catch((error: unknown) => logger.warn('FloatBar.panel', getErrorMessage(error)))
}

// ── Drops ───────────────────────────────────────────────────────────

/** Entered elements, so leaving one for another does not end the drag. */
let dragDepth = 0

function droppable(event: DragEvent): boolean {
  const types = event.dataTransfer?.types ?? []
  return types.includes('Files') || types.includes('text/uri-list') || types.includes('text/plain')
}

function onDragEnter(event: DragEvent) {
  if (!droppable(event)) return
  dragDepth += 1
  dropping.value = true
}

function onDragOver(event: DragEvent) {
  if (!droppable(event)) return
  event.preventDefault()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy'
}

function onDragLeave() {
  dragDepth = Math.max(0, dragDepth - 1)
  if (dragDepth === 0) dropping.value = false
}

/** Links come as text; files are read here, since the page gets no paths. */
async function onDrop(event: DragEvent) {
  event.preventDefault()
  dragDepth = 0
  dropping.value = false
  const data = event.dataTransfer
  if (!data) return
  const files = Array.from(data.files)
  const torrents = files.filter((file) => detectKind(file.name) === 'torrent')
  // A file also comes as its path or `file:` link, which is not a download.
  const text = files.length > 0 ? '' : data.getData('text/uri-list') || data.getData('text/plain')
  const links = normalizeUriLines(text.replace(/^#.*$/gm, '')).filter((line) => detectResource(line))
  try {
    const items = await Promise.all(
      torrents.map(async (file) =>
        createBatchItem('torrent', file.name, encodeBase64(new Uint8Array(await file.arrayBuffer()))),
      ),
    )
    if (items.length === 0 && links.length === 0) return
    pulse()
    await invoke('add_window_open', { batch: { links, items } })
  } catch (error) {
    logger.warn('FloatBar.drop', getErrorMessage(error))
  }
}

// ── Actions ─────────────────────────────────────────────────────────

function openMain() {
  invoke('float_window_open_main').catch((error: unknown) => logger.warn('FloatBar.open', getErrorMessage(error)))
}

function toggleAll() {
  const action = active.value ? aria2Api.forcePauseAll() : aria2Api.resumeEligible()
  action.catch((error: unknown) => logger.warn('FloatBar.toggleAll', getErrorMessage(error)))
}

/** The desktop saves the mode and opens or closes the bar on its next tick. */
function setMode(mode: FloatWindowMode) {
  invoke('float_window_set_mode', { mode }).catch((error: unknown) =>
    logger.warn('FloatBar.mode', getErrorMessage(error)),
  )
}

async function showMenu(event: MouseEvent) {
  event.preventDefault()
  try {
    // The mode may have changed in the settings or the tray since.
    await preferenceStore.reloadPreferenceFromDisk()
    const current = preferenceStore.config.floatWindow
    const menu = await Menu.new({
      items: [
        { id: 'show', text: t('app.show'), action: openMain },
        {
          id: 'all',
          text: t(active.value ? 'task.pause-all-task' : 'task.resume-all-task'),
          action: toggleAll,
        },
        { item: 'Separator' },
        {
          id: 'float-mode',
          text: t('preferences.float-window'),
          items: FLOAT_WINDOW_MODES.map((mode) => ({
            id: `float-${mode}`,
            text: t(FLOAT_WINDOW_MODE_LABELS[mode]),
            checked: mode === current,
            action: () => setMode(mode),
          })),
        },
      ],
    })
    await menu.popup()
  } catch (error) {
    logger.warn('FloatBar.menu', getErrorMessage(error))
  }
}

onMounted(async () => {
  document.documentElement.classList.add('float-window-document')
  if (root.value) resizeObserver.observe(root.value)
  unlisteners.push(
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
  <div
    ref="root"
    class="float-bar"
    @dragenter="onDragEnter"
    @dragover="onDragOver"
    @dragleave="onDragLeave"
    @drop="onDrop"
  >
    <div
      class="bar"
      :class="[`bar--${face}`, { 'bar--pulse': pulsing, 'bar--bordered': isLinux }]"
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
      <span class="bar-fill" :class="{ 'bar-fill--sweep': sweeping }" aria-hidden="true">
        <span class="bar-fill-level" :style="sweeping ? undefined : { width: `${fill}%` }" />
      </span>

      <Transition name="bar-face">
        <span :key="face" class="bar-face" :class="{ 'bar-face--centered': !aside }">
          <img v-if="face === 'idle'" class="bar-icon bar-logo" src="/logo.svg" alt="" draggable="false" />
          <NIcon v-else class="bar-icon" :size="15" :component="ICONS[face]" />
          <span v-if="showsSpeed" class="bar-text bar-speed">
            <span class="bar-speed-value">{{ speed.value }}</span>
            <span class="bar-speed-unit">{{ speed.unit }}</span>
          </span>
          <span v-else class="bar-text">{{ label }}</span>
          <span v-if="aside" class="bar-aside">{{ aside }}</span>
        </span>
      </Transition>

      <Transition name="bar-dot">
        <span v-if="failed" class="bar-dot" />
      </Transition>
    </div>
  </div>
</template>

<style scoped>
/* The window is sized to the bar; the system draws its corners and shadow. */
.float-bar {
  display: inline-flex;
  user-select: none;
}

.bar {
  --bar-accent: var(--m3-primary);
  position: relative;
  display: grid;
  width: 132px;
  height: 36px;
  overflow: hidden;
  background: var(--main-bg);
  color: var(--m3-on-surface);
  cursor: default;
  outline: none;
  transition: box-shadow 160ms ease;
}

/* Hover and press tint the whole bar, as a button does; a new download flashes it. */
.bar::before,
.bar::after {
  content: '';
  position: absolute;
  inset: 0;
  opacity: 0;
  pointer-events: none;
}

.bar::before {
  background: var(--m3-on-surface);
  transition: opacity 160ms ease;
}

.bar:hover::before {
  opacity: 0.04;
}

.bar:active::before {
  opacity: 0.08;
  transition-duration: 80ms;
}

.bar::after {
  background: var(--m3-primary);
}

.bar--pulse::after {
  animation: bar-pulse 700ms var(--dialog-ease-enter);
}

@keyframes bar-pulse {
  30% {
    opacity: 0.14;
  }
  100% {
    opacity: 0;
  }
}

/* GTK draws no frame around a window without decorations. */
.bar--bordered {
  box-shadow: inset 0 0 0 1px var(--m3-outline-variant);
}

/* Something droppable is over it, or it has the keyboard focus. */
.bar--drop,
.bar:focus-visible {
  box-shadow: inset 0 0 0 2px var(--m3-primary);
}

.bar--connecting,
.bar--paused {
  --bar-accent: var(--m3-outline);
}

.bar--waiting {
  --bar-accent: var(--m3-info);
}

.bar--seeding,
.bar--done {
  --bar-accent: var(--m3-success);
}

/* ── Fill ─────────────────────────────────────────────────────────── */

/* The progress fills the bar from the left, edged where it stands. */
.bar-fill {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.bar-fill-level {
  position: absolute;
  inset: 0 auto 0 0;
  width: 0;
  background: linear-gradient(
    90deg,
    color-mix(in srgb, var(--bar-accent) 8%, transparent),
    color-mix(in srgb, var(--bar-accent) 20%, transparent)
  );
  box-shadow: inset -2px 0 0 color-mix(in srgb, var(--bar-accent) 55%, transparent);
  transition:
    width var(--task-motion-progress) var(--task-motion-ease),
    background 240ms ease;
}

/* A full fill needs no edge. */
.bar--seeding .bar-fill-level,
.bar--done .bar-fill-level,
.bar--drop .bar-fill-level {
  box-shadow: none;
}

/* Bytes arrive but the size is unknown: a band of light sweeps across. */
.bar-fill--sweep .bar-fill-level {
  width: 45%;
  background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--bar-accent) 22%, transparent), transparent);
  box-shadow: none;
  animation: bar-sweep 1.6s var(--task-motion-ease) infinite;
}

@keyframes bar-sweep {
  from {
    translate: -100%;
  }
  to {
    translate: 225%;
  }
}

/* ── Face ─────────────────────────────────────────────────────────── */

.bar-face {
  position: relative;
  grid-area: 1 / 1;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 6px;
  padding: 0 11px 0 10px;
  pointer-events: none;
  transition: transform 120ms var(--dialog-ease-enter);
}

/* Without a figure at its end, the face sits in the middle. */
.bar-face--centered {
  grid-template-columns: auto minmax(0, max-content);
  justify-content: center;
}

/* A press sinks the face a little, as a button gives under the finger. */
.bar:active .bar-face {
  transform: scale(0.96);
}

.bar-icon {
  color: var(--bar-accent);
}

.bar--connecting .bar-icon,
.bar--paused .bar-icon {
  color: var(--m3-on-surface-variant);
}

.bar-logo {
  width: 18px;
  height: 18px;
}

.bar-text {
  min-width: 0;
  overflow: hidden;
  font-size: var(--font-size-xs);
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.bar--connecting .bar-text,
.bar--waiting .bar-text,
.bar--paused .bar-text {
  color: var(--m3-on-surface-variant);
  font-weight: 500;
}

.bar-speed {
  display: flex;
  align-items: baseline;
  gap: 3px;
}

.bar-speed-value {
  font-size: 14px;
  font-weight: 650;
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.02em;
}

.bar-speed-unit {
  color: var(--m3-on-surface-variant);
  font-size: var(--font-size-2xs);
  font-weight: 500;
}

.bar-aside {
  color: var(--m3-on-surface-variant);
  font-size: var(--font-size-2xs);
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.bar--downloading .bar-aside {
  color: var(--bar-accent);
}

.bar-face-enter-active {
  transition:
    opacity 220ms var(--dialog-ease-enter),
    translate 220ms var(--dialog-ease-enter);
}

.bar-face-leave-active {
  transition: opacity 140ms var(--dialog-ease-leave);
}

.bar-face-enter-from {
  opacity: 0;
  translate: 0 4px;
}

.bar-face-leave-to {
  opacity: 0;
}

/* ── Error dot ────────────────────────────────────────────────────── */

.bar-dot {
  position: absolute;
  top: 5px;
  right: 5px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--m3-error);
  box-shadow: 0 0 0 2px var(--main-bg);
}

.bar-dot-enter-active,
.bar-dot-leave-active {
  transition: transform 200ms var(--dialog-ease-enter);
}

.bar-dot-enter-from,
.bar-dot-leave-to {
  transform: scale(0);
}
</style>
