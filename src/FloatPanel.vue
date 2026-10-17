<script setup lang="ts">
/**
 * @fileoverview The panel beside the floating orb (#669, #718, #402).
 *
 * A click on the orb builds it; it shows itself beside the orb once it has
 * its first snapshot, lists the downloads with their pieces and lets each be
 * paused or resumed. It closes, and is destroyed, when it loses the focus, on
 * Esc, or on another click on the orb. Dragging its header takes the orb along.
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
import { usePlatform } from '@/composables/usePlatform'
import { usePreferenceStore } from '@/stores/preference'
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

const { t } = useI18n()
const preferenceStore = usePreferenceStore()
const { isLinux } = usePlatform()
const view = getCurrentWebviewWindow()
const root = ref<HTMLElement | null>(null)
/** How long the panel waits for its first snapshot before showing without it. */
const SNAPSHOT_WAIT_MS = 400

const shown = ref(false)
const busy = ref(false)
const unlisteners: UnlistenFn[] = []

const { update } = useFloatUpdates()

const zoom = computed(() => preferenceStore.config.uiScale || 1)
const tasks = computed(() => update.value?.tasks ?? [])
const anyActive = computed(() => (update.value?.stat.numActive ?? 0) > 0)

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

/** Sizes the window to the content, which the desktop places beside the orb and shows. */
async function present(): Promise<boolean> {
  const element = root.value
  if (!element) return false
  const rect = element.getBoundingClientRect()
  return invoke('float_panel_present', {
    width: Math.ceil(rect.width * zoom.value),
    height: Math.ceil(rect.height * zoom.value),
  })
    .then(() => true)
    .catch((error: unknown) => {
      logger.warn('FloatPanel.present', getErrorMessage(error))
      return false
    })
}

/** Shows the panel once, sized to its first content. */
async function open() {
  if (shown.value) return
  shown.value = true
  // Measured once its fonts are in, so the shown panel does not grow and move.
  await Promise.all([nextTick(), document.fonts.ready])
  if (!(await present())) dismiss()
}

/** Asks the desktop to close the panel, which destroys this window. */
function dismiss() {
  invoke('float_panel_dismiss').catch((error: unknown) => logger.debug('FloatPanel.dismiss', getErrorMessage(error)))
}

/**
 * Follows the content while open: rows come and go with the downloads, and the
 * interface size scales the card without changing its CSS size.
 */
let refit = 0
function scheduleRefit() {
  if (!shown.value || refit) return
  refit = requestAnimationFrame(() => {
    refit = 0
    void present()
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
    await view.onFocusChanged(({ payload: focused }) => {
      if (!focused && shown.value) dismiss()
    }),
  )
  // The desktop sends a snapshot as soon as the page listens.
  if (!update.value) {
    await new Promise<void>((resolve) => {
      const timer = setTimeout(resolve, SNAPSHOT_WAIT_MS)
      const stop = watch(update, () => {
        clearTimeout(timer)
        stop()
        resolve()
      })
    })
  }
  await open()
})

onBeforeUnmount(() => {
  cancelAnimationFrame(refit)
  resizeObserver.disconnect()
  for (const unlisten of unlisteners) unlisten()
})
</script>

<template>
  <div ref="root" class="float-panel" :class="{ 'float-panel--bordered': isLinux }">
    <section class="panel-card">
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
          v-for="task in tasks"
          :key="task.gid"
          class="panel-row"
          :class="[`panel-row--${task.status}`, { 'panel-row--seeding': seeds(task) }]"
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
/* The window is sized to the card; the system draws its frame. */
.float-panel {
  display: inline-flex;
  user-select: none;
}

.panel-card {
  display: flex;
  flex-direction: column;
  width: 320px;
  overflow: hidden;
  background: var(--main-bg);
  color: var(--m3-on-surface);
}

/* GTK draws no frame around a window without decorations. */
.float-panel--bordered .panel-card {
  box-shadow: inset 0 0 0 1px var(--m3-outline-variant);
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
