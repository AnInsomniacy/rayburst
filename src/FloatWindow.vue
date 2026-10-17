<script setup lang="ts">
/**
 * @fileoverview The floating orb (#669, #718, #402, #726).
 *
 * A 52 px orb that stays on top, drawn with its shadow in a transparent
 * window. It fills from the bottom with the overall progress, like a level, under the received
 * speed while downloading and the sent speed while only seeding; otherwise its
 * sign tells what the queue waits for: servers, its turn, or the user. A check
 * follows the last download, the Rayburst mark rests in it, and a dot marks an
 * error. Only bytes arriving move it: a download of unknown size sends a
 * swell up its fill, everything else holds still. A click opens the panel
 * beside it, dragging moves it, links and torrents dropped on it open the add
 * window, and its context menu reaches the main window and switches when it
 * shows. The window never changes its size.
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
import { usePreferenceStore } from '@/stores/preference'
import { FLOAT_WINDOW_MODE_LABELS, FLOAT_WINDOW_MODES, TASK_STATUS } from '@shared/constants'
import { bytesToSize } from '@shared/utils'
import { createBatchItem, detectKind, encodeBase64, normalizeUriLines } from '@shared/utils/batchHelpers'
import { floatOrbState, type FloatOrbKind } from '@shared/utils/floatOrb'
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

type Face = FloatOrbKind | 'drop'

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
const view = getCurrentWebviewWindow()
const root = ref<HTMLElement | null>(null)
const pulsing = ref(false)
const done = ref(false)
const failed = ref(false)
/** The orb is about to go: it shrinks away first. */
const leaving = ref(false)
/** Something droppable is dragged over the orb. */
const dropping = ref(false)
const unlisteners: UnlistenFn[] = []
let known: Set<string> | null = null
let pulseTimer: ReturnType<typeof setTimeout> | undefined
let doneTimer: ReturnType<typeof setTimeout> | undefined
let press: { x: number; y: number } | null = null

const { update } = useFloatUpdates(greetNewDownloads)

const zoom = computed(() => preferenceStore.config.uiScale || 1)
const state = computed(() => floatOrbState(update.value, done.value))
const face = computed<Face>(() => (dropping.value ? 'drop' : state.value.kind))
const active = computed(() => state.value.kind === 'downloading' || state.value.kind === 'connecting')

const speed = computed(() => {
  const [value = '0', unit = ''] = bytesToSize(state.value.speed).split(' ')
  return { value, unit: `${face.value === 'seeding' ? '↑ ' : ''}${unit}/s` }
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
/** How far a queue that waits got, under its sign. */
const percent = computed(() => {
  const { progress } = state.value
  const waits = face.value === 'connecting' || face.value === 'waiting' || face.value === 'paused'
  return waits && progress != null ? `${Math.floor(progress)}%` : ''
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
    showsSpeed.value || face.value === 'idle' ? '' : label.value,
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
// The interface size scales the orb without changing its CSS size.
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
    logger.warn('FloatWindow.drop', getErrorMessage(error))
  }
}

// ── Actions ─────────────────────────────────────────────────────────

function openMain() {
  invoke('float_window_open_main').catch((error: unknown) => logger.warn('FloatWindow.open', getErrorMessage(error)))
}

function toggleAll() {
  const action = active.value ? aria2Api.forcePauseAll() : aria2Api.resumeEligible()
  action.catch((error: unknown) => logger.warn('FloatWindow.toggleAll', getErrorMessage(error)))
}

/** The desktop saves the mode and opens or closes the orb on its next tick. */
function setMode(mode: FloatWindowMode) {
  invoke('float_window_set_mode', { mode }).catch((error: unknown) =>
    logger.warn('FloatWindow.mode', getErrorMessage(error)),
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
    logger.warn('FloatWindow.menu', getErrorMessage(error))
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
    await view.listen('float:leave', () => (leaving.value = true)),
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
    class="float-window"
    @dragenter="onDragEnter"
    @dragover="onDragOver"
    @dragleave="onDragLeave"
    @drop="onDrop"
  >
    <Transition name="orb" appear>
      <div
        v-if="update && !leaving"
        class="orb"
        :class="[`orb--${face}`, { 'orb--pulse': pulsing }]"
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
        <span class="orb-fill" :class="{ 'orb-fill--swell': sweeping }" aria-hidden="true">
          <span class="orb-fill-level" :style="sweeping ? undefined : { height: `${fill}%` }" />
        </span>

        <Transition name="orb-face">
          <span :key="face" class="orb-face">
            <img v-if="face === 'idle'" class="orb-logo" src="/logo.svg" alt="" draggable="false" />
            <template v-else-if="showsSpeed">
              <span class="orb-speed-value" :data-length="speed.value.length">{{ speed.value }}</span>
              <span class="orb-speed-unit">{{ speed.unit }}</span>
            </template>
            <template v-else>
              <NIcon class="orb-sign" :size="percent ? 17 : 21" :component="ICONS[face]" />
              <span v-if="percent" class="orb-percent">{{ percent }}</span>
            </template>
          </span>
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
  --orb-accent: var(--m3-primary);
  --orb-shadow:
    0 0 0 1px color-mix(in srgb, var(--m3-outline-variant) 80%, transparent), 0 1px 2px var(--m3-shadow),
    0 4px 10px -2px var(--m3-shadow);
  position: relative;
  display: grid;
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background:
    radial-gradient(120% 90% at 50% 0%, color-mix(in srgb, var(--m3-primary) 7%, transparent), transparent 62%),
    var(--main-bg);
  box-shadow: var(--orb-shadow);
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

/* Something droppable is over it: it rises and takes the accent. */
.orb--drop,
.orb--drop:hover {
  transform: scale(1.1);
  box-shadow:
    0 0 0 2px var(--m3-primary),
    0 6px 14px -2px var(--m3-shadow);
}

.orb:focus-visible {
  box-shadow:
    0 0 0 2px var(--m3-primary),
    0 4px 10px -2px var(--m3-shadow);
}

.orb--connecting,
.orb--paused {
  --orb-accent: var(--m3-outline);
}

.orb--waiting {
  --orb-accent: var(--m3-info);
}

.orb--seeding,
.orb--done {
  --orb-accent: var(--m3-success);
}

/* ── Fill ─────────────────────────────────────────────────────────── */

/* The progress fills the orb from the bottom, its surface marked. */
.orb-fill {
  position: absolute;
  inset: 0;
  overflow: hidden;
  border-radius: inherit;
  pointer-events: none;
}

.orb-fill-level {
  position: absolute;
  inset: auto 0 0;
  height: 0;
  background: linear-gradient(
    0deg,
    color-mix(in srgb, var(--orb-accent) 24%, transparent),
    color-mix(in srgb, var(--orb-accent) 10%, transparent)
  );
  box-shadow: inset 0 2px 0 color-mix(in srgb, var(--orb-accent) 55%, transparent);
  transition:
    height var(--task-motion-progress) var(--task-motion-ease),
    background 240ms ease;
}

/* A full fill needs no surface. */
.orb--seeding .orb-fill-level,
.orb--done .orb-fill-level,
.orb--drop .orb-fill-level {
  box-shadow: none;
}

/* Bytes arrive but the size is unknown: a swell rises through the orb. */
.orb-fill--swell .orb-fill-level {
  height: 45%;
  background: linear-gradient(0deg, transparent, color-mix(in srgb, var(--orb-accent) 24%, transparent), transparent);
  box-shadow: none;
  animation: orb-swell 1.6s var(--task-motion-ease) infinite;
}

@keyframes orb-swell {
  from {
    translate: 0 100%;
  }
  to {
    translate: 0 -225%;
  }
}

/* ── Face ─────────────────────────────────────────────────────────── */

.orb-face {
  position: relative;
  grid-area: 1 / 1;
  display: grid;
  align-content: center;
  justify-items: center;
  line-height: 1;
  pointer-events: none;
}

.orb-logo {
  width: 28px;
  height: 28px;
}

.orb-speed-value {
  font-size: 15px;
  font-weight: 650;
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.02em;
}

.orb-speed-value[data-length='5'] {
  font-size: 13.5px;
}

.orb-speed-unit {
  margin-top: 2px;
  color: var(--m3-on-surface-variant);
  font-size: 10px;
  font-weight: 500;
  white-space: nowrap;
}

.orb-sign {
  color: var(--orb-accent);
}

.orb--connecting .orb-sign,
.orb--paused .orb-sign {
  color: var(--m3-on-surface-variant);
}

.orb-percent {
  margin-top: 3px;
  color: var(--m3-on-surface-variant);
  font-size: 10px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.orb-face-enter-active {
  transition:
    opacity 260ms var(--dialog-ease-enter),
    scale 260ms var(--dialog-ease-enter);
}

.orb-face-leave-active {
  transition:
    opacity 160ms var(--dialog-ease-leave),
    scale 160ms var(--dialog-ease-leave);
}

.orb-face-enter-from,
.orb-face-leave-to {
  opacity: 0;
  scale: 0.7;
}

/* ── Pulse, error dot, arrival ────────────────────────────────────── */

/* A new download: a ring of the accent spreads from the orb. */
.orb--pulse {
  animation: orb-pulse 700ms var(--dialog-ease-enter);
}

@keyframes orb-pulse {
  0% {
    box-shadow:
      0 0 0 0 color-mix(in srgb, var(--m3-primary) 45%, transparent),
      var(--orb-shadow);
  }
  35% {
    transform: scale(1.08);
  }
  100% {
    box-shadow:
      0 0 0 8px color-mix(in srgb, var(--m3-primary) 0%, transparent),
      var(--orb-shadow);
  }
}

.orb-dot {
  position: absolute;
  top: 1px;
  right: 1px;
  width: 10px;
  height: 10px;
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
