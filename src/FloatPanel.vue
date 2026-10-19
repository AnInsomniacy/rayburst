<script setup lang="ts">
/**
 * @fileoverview The panel beside the floating orb (#669, #718, #402).
 *
 * Built hidden with the orb. A click on the orb opens it centered beside the
 * orb, out of which it grows; it lists the downloads with their pieces and
 * lets each be paused or resumed. It closes when it loses the focus, on Esc, or
 * on another click on the orb. Dragging its header takes the orb along.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { NButton, NIcon } from 'naive-ui'
import { AddOutline, ArrowForwardOutline, PauseOutline, PlayOutline } from '@vicons/ionicons5'
import aria2Api from '@/api/aria2'
import SegmentBar from '@/components/task/SegmentBar.vue'
import { useEscapeKey } from '@/composables/useEscapeKey'
import { useFloatUpdates } from '@/composables/useFloatUpdates'
import { afterReveal, nextFrame, reducedMotion } from '@/composables/useWindowReveal'
import { useWindowScale } from '@/composables/useWindowScale'
import { TASK_STATUS } from '@shared/constants'
import {
  bytesToSize,
  calcProgress,
  getTaskCompletedLength,
  getTaskName,
  timeFormat,
  timeRemaining,
} from '@shared/utils'
import { getErrorMessage } from '@shared/utils/errorMessage'
import { logger } from '@shared/logger'
import type { Aria2Task } from '@shared/types'

interface PanelSide {
  above: boolean
}

/** Duration of the leave animation: `--dialog-leave`. */
const LEAVE_MS = 170

const { t } = useI18n()
const view = getCurrentWebviewWindow()
const root = ref<HTMLElement | null>(null)
const phase = ref<'hidden' | 'shown' | 'leaving'>('hidden')
const side = ref<PanelSide>({ above: true })
const busy = ref(false)
const unlisteners: UnlistenFn[] = []
/** Counts openings and closings, so a superseded one stops where it is. */
let generation = 0

const { update } = useFloatUpdates()

const zoom = useWindowScale()
const tasks = computed(() => update.value?.tasks ?? [])
const anyActive = computed(() => (update.value?.stat.numActive ?? 0) > 0)
const origin = computed(() => `${side.value.above ? 'bottom' : 'top'} center`)

useEscapeKey(() => dismiss())

function percentOf(task: Aria2Task): number {
  return calcProgress(Number(task.totalLength) || 0, getTaskCompletedLength(task))
}

const seeds = (task: Aria2Task) => task.seeder === 'true'

function metaOf(task: Aria2Task): string {
  if (seeds(task)) {
    if (task.status === TASK_STATUS.PAUSED) return t('task.seeding-paused')
    return `${t('task.seeding')} · ↑ ${bytesToSize(Number(task.uploadSpeed) || 0)}/s`
  }
  const percent = `${percentOf(task)}%`
  if (task.status === TASK_STATUS.PAUSED) return `${percent} · ${t('task.status-paused')}`
  if (task.status === TASK_STATUS.WAITING) return `${percent} · ${t('task.status-waiting')}`
  const total = Number(task.totalLength) || 0
  const speed = Number(task.downloadSpeed) || 0
  const remaining = timeFormat(timeRemaining(total, getTaskCompletedLength(task), speed), {
    i18n: { gt1d: t('app.gt1d'), hour: t('app.hour'), minute: t('app.minute'), second: t('app.second') },
  })
  return [percent, `${bytesToSize(speed)}/s`, remaining].filter(Boolean).join(' · ')
}

// ── Open and close ──────────────────────────────────────────────────

/** Sizes the window to the card, which the desktop places beside the orb. */
async function present(): Promise<PanelSide | null> {
  const element = root.value
  if (!element) return null
  const rect = element.getBoundingClientRect()
  return invoke<PanelSide>('float_panel_present', {
    width: Math.ceil(rect.width * zoom.value),
    height: Math.ceil(rect.height * zoom.value),
  }).catch((error: unknown) => {
    logger.warn('FloatPanel.present', getErrorMessage(error))
    return null
  })
}

async function open() {
  const run = ++generation
  phase.value = 'hidden'
  // Measured once its fonts are in, so the shown card does not grow and move.
  await Promise.all([nextTick(), document.fonts.ready])
  const placed = await present()
  if (run !== generation) return
  if (!placed) {
    dismiss()
    return
  }
  side.value = placed
  // Let the shown window paint the card's start state before it grows in.
  await afterReveal()
  if (run !== generation) return
  phase.value = 'shown'
  // Rows that changed while it opened were not followed yet.
  scheduleRefit()
}

async function close() {
  const run = ++generation
  if (phase.value !== 'hidden') {
    phase.value = 'leaving'
    if (!reducedMotion()) await new Promise((resolve) => setTimeout(resolve, LEAVE_MS))
    if (run !== generation) return
    // Paint the start state before hiding, so the next opening never flashes this one.
    phase.value = 'hidden'
    await nextFrame()
    await nextFrame()
    if (run !== generation) return
  }
  await view.hide().catch((error: unknown) => logger.debug('FloatPanel.hide', getErrorMessage(error)))
}

/** Asks the desktop to close the panel; it answers with the close event. */
function dismiss() {
  invoke('float_panel_dismiss').catch((error: unknown) => logger.debug('FloatPanel.dismiss', getErrorMessage(error)))
}

/**
 * Follows the content while open: rows come and go with the downloads, and the
 * interface size scales the card without changing its CSS size.
 */
let refit = 0
function scheduleRefit() {
  if (phase.value !== 'shown' || refit) return
  refit = requestAnimationFrame(async () => {
    refit = 0
    const placed = await present()
    if (placed) side.value = placed
  })
}
const resizeObserver = new ResizeObserver(scheduleRefit)
watch(zoom, scheduleRefit)

// ── Actions ─────────────────────────────────────────────────────────

async function run(action: () => Promise<unknown>) {
  if (busy.value) return
  busy.value = true
  try {
    await action()
  } catch (error) {
    logger.warn('FloatPanel.action', getErrorMessage(error))
  } finally {
    busy.value = false
  }
}

const toggleTask = (task: Aria2Task) =>
  run(() =>
    task.status === TASK_STATUS.PAUSED ? aria2Api.resumeTask({ gid: task.gid }) : aria2Api.pauseTask({ gid: task.gid }),
  )
const toggleAll = () => run(() => (anyActive.value ? aria2Api.forcePauseAll() : aria2Api.resumeEligible()))

function openMain() {
  dismiss()
  invoke('float_window_open_main').catch((error: unknown) => logger.warn('FloatPanel.open', getErrorMessage(error)))
}

function newTask() {
  dismiss()
  invoke('add_window_open', { batch: { items: [], manual: true } }).catch((error: unknown) =>
    logger.warn('FloatPanel.newTask', getErrorMessage(error)),
  )
}

onMounted(async () => {
  document.documentElement.classList.add('float-window-document')
  if (root.value) resizeObserver.observe(root.value)
  unlisteners.push(
    await view.listen('float-panel:open', () => void open()),
    await view.listen('float-panel:close', () => void close()),
    await view.onFocusChanged(({ payload: focused }) => {
      if (!focused && phase.value !== 'hidden') dismiss()
    }),
  )
})

onBeforeUnmount(() => {
  cancelAnimationFrame(refit)
  resizeObserver.disconnect()
  for (const unlisten of unlisteners) unlisten()
})
</script>

<template>
  <div ref="root" class="float-panel" :class="`float-panel--${phase}`">
    <section class="panel-card" :style="{ transformOrigin: origin }">
      <header class="panel-header" data-tauri-drag-region>
        <img class="panel-logo" src="/logo.svg" alt="" draggable="false" data-tauri-drag-region />
        <span class="panel-title" data-tauri-drag-region>Rayburst</span>
        <span v-if="update" class="panel-speeds" data-tauri-drag-region>
          <span data-tauri-drag-region>↓ {{ bytesToSize(update.stat.downloadSpeed) }}/s</span>
          <span v-if="update.stat.uploadSpeed > 0" data-tauri-drag-region
            >↑ {{ bytesToSize(update.stat.uploadSpeed) }}/s</span
          >
        </span>
      </header>

      <ul v-if="tasks.length" class="panel-list">
        <li
          v-for="(task, index) in tasks"
          :key="task.gid"
          class="panel-row"
          :class="[`panel-row--${task.status}`, { 'panel-row--seeding': seeds(task) }]"
          :style="{ '--row': Math.min(index, 8) }"
          @click="openMain"
        >
          <div class="panel-row-head">
            <span class="panel-row-name" :title="getTaskName(task)">{{ getTaskName(task) }}</span>
            <NButton
              quaternary
              circle
              size="tiny"
              :disabled="busy"
              :aria-label="t(task.status === TASK_STATUS.PAUSED ? 'task.resume-task' : 'task.pause-task')"
              @click.stop="toggleTask(task)"
            >
              <template #icon>
                <NIcon><PlayOutline v-if="task.status === TASK_STATUS.PAUSED" /><PauseOutline v-else /></NIcon>
              </template>
            </NButton>
          </div>
          <SegmentBar
            v-if="task.status === TASK_STATUS.ACTIVE && task.bitfield && !seeds(task)"
            class="panel-row-bar"
            :bitfield="task.bitfield"
            :pieces="Number(task.numPieces) || 0"
          />
          <div v-else class="panel-row-bar panel-row-bar--plain">
            <span :style="{ width: `${percentOf(task)}%` }" />
          </div>
          <span class="panel-row-meta">{{ metaOf(task) }}</span>
        </li>
      </ul>

      <div v-else class="panel-empty">
        <img class="panel-empty-logo" src="/logo.svg" alt="" draggable="false" />
        <span>{{ t('task.no-active-tasks') }}</span>
        <div class="panel-empty-actions">
          <NButton size="small" secondary @click="newTask">
            <template #icon
              ><NIcon><AddOutline /></NIcon
            ></template>
            {{ t('task.new-task') }}
          </NButton>
          <NButton size="small" quaternary @click="openMain">{{ t('app.show') }}</NButton>
        </div>
      </div>

      <footer v-if="tasks.length" class="panel-footer">
        <NButton text size="small" :disabled="busy" @click="toggleAll">
          <template #icon
            ><NIcon><PauseOutline v-if="anyActive" /><PlayOutline v-else /></NIcon
          ></template>
          {{ t(anyActive ? 'task.pause-all-task' : 'task.resume-all-task') }}
        </NButton>
        <NButton text size="small" type="primary" icon-placement="right" @click="openMain">
          <template #icon
            ><NIcon><ArrowForwardOutline /></NIcon
          ></template>
          {{ t('app.show') }}
        </NButton>
      </footer>
    </section>
  </div>
</template>

<style scoped>
/* The margin holds the card's shadow inside the transparent window;
 * `PANEL_INSET` in float_window.rs. */
.float-panel {
  display: inline-flex;
  width: max-content;
  padding: 12px;
  user-select: none;
}

.panel-card {
  display: flex;
  flex-direction: column;
  width: 320px;
  overflow: hidden;
  border-radius: var(--radius-xl);
  background: var(--main-bg);
  box-shadow:
    0 0 0 1px color-mix(in srgb, var(--m3-outline-variant) 80%, transparent),
    0 2px 4px var(--m3-shadow),
    0 10px 24px -6px var(--m3-shadow);
  color: var(--m3-on-surface);
  transition:
    opacity var(--dialog-fade) cubic-bezier(0.2, 0, 0, 1),
    transform var(--dialog-enter) var(--dialog-ease-enter);
}

.float-panel--hidden .panel-card {
  opacity: 0;
  transform: scale(0.92);
  transition: none;
}

.float-panel--leaving .panel-card {
  opacity: 0;
  transform: scale(0.96);
  transition:
    opacity var(--dialog-leave) var(--dialog-ease-leave),
    transform var(--dialog-leave) var(--dialog-ease-leave);
}

/* ── Header ───────────────────────────────────────────────────────── */

.panel-header {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 44px;
  padding: 0 16px;
  border-bottom: 1px solid color-mix(in srgb, var(--m3-outline-variant) 70%, transparent);
  cursor: default;
}

.panel-logo {
  width: 18px;
  height: 18px;
}

.panel-title {
  font-size: var(--font-size-sm);
  font-weight: 600;
}

.panel-speeds {
  display: flex;
  gap: 10px;
  margin-left: auto;
  color: var(--m3-on-surface-variant);
  font-size: var(--font-size-2xs);
  font-variant-numeric: tabular-nums;
}

/* ── Rows ─────────────────────────────────────────────────────────── */

.panel-list {
  max-height: 324px;
  margin: 0;
  padding: 4px 0;
  overflow-y: auto;
  list-style: none;
  scrollbar-width: thin;
}

.panel-row {
  display: grid;
  gap: 4px;
  padding: 7px 10px 7px 16px;
  cursor: default;
  transition: background-color 120ms ease;
}

.panel-row:hover {
  background: color-mix(in srgb, var(--m3-on-surface) 4%, transparent);
}

/* Rows settle in one after another as the panel opens. */
.float-panel--shown .panel-row {
  animation: panel-row-in 260ms var(--dialog-ease-enter) both;
  animation-delay: calc(40ms + var(--row) * 20ms);
}

@keyframes panel-row-in {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
}

.panel-row-head {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 22px;
}

.panel-row-name {
  flex: 1;
  min-width: 0;
  overflow: clip visible;
  font-size: var(--font-size-xs);
  font-weight: 500;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.panel-row-bar {
  height: 4px;
  margin-right: 6px;
}

.panel-row-bar--plain {
  overflow: hidden;
  border-radius: var(--radius-full);
  background: var(--m3-surface-container-highest);
}

.panel-row-bar--plain span {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: var(--m3-primary);
  transition: width var(--task-motion-progress) var(--task-motion-ease);
}

.panel-row-meta {
  overflow: clip visible;
  color: var(--m3-on-surface-variant);
  font-size: var(--font-size-2xs);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.panel-row--paused .panel-row-name,
.panel-row--waiting .panel-row-name {
  color: var(--m3-on-surface-variant);
}

.panel-row--paused .panel-row-bar--plain span {
  background: var(--m3-outline);
}

.panel-row--waiting .panel-row-bar--plain span {
  background: var(--m3-info);
}

.panel-row--seeding .panel-row-bar--plain span {
  background: var(--m3-success);
}

.panel-row--seeding.panel-row--paused .panel-row-bar--plain span {
  background: var(--m3-outline);
}

/* ── Empty state and footer ───────────────────────────────────────── */

.panel-empty {
  display: grid;
  justify-items: center;
  gap: 10px;
  padding: 22px 16px 18px;
  color: var(--m3-on-surface-variant);
  font-size: var(--font-size-xs);
}

.panel-empty-logo {
  width: 36px;
  height: 36px;
  opacity: 0.35;
}

.panel-empty-actions {
  display: flex;
  gap: 8px;
  margin-top: 2px;
}

.panel-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  height: 40px;
  padding: 0 16px;
  border-top: 1px solid color-mix(in srgb, var(--m3-outline-variant) 70%, transparent);
}
</style>
