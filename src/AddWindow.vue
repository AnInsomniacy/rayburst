<script setup lang="ts">
/**
 * @fileoverview Content of the standalone add-task window.
 *
 * The desktop builds this window ahead of time, hidden, and hands it every new
 * download: browser handoffs, links, torrent files and manual entry from the
 * main window. It shows itself once there is something to confirm, and puts
 * itself away after the user submits or cancels, or after handing
 * auto-submitted downloads to the engine.
 *
 * The window is transparent and draws its panel, rounded and shadowed, with a
 * margin around it. Motion stays inside the page, where it runs on the
 * compositor: the panel rises in and sinks away, and
 * grows or shrinks with its content inside a window that is resized once per
 * change instead of on every frame.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { emit, type UnlistenFn } from '@tauri-apps/api/event'
import { LogicalSize } from '@tauri-apps/api/window'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import AddTask from '@/components/task/AddTask.vue'
import WindowControls from '@/components/layout/WindowControls.vue'
import TorrentDropOverlay from '@/components/task/TorrentDropOverlay.vue'
import { useAppStore } from '@/stores/app'
import { usePreferenceStore } from '@/stores/preference'
import { useWindowScale } from '@/composables/useWindowScale'
import { useAppMessage } from '@/composables/useAppMessage'
import { useEscapeKey } from '@/composables/useEscapeKey'
import { usePlatform } from '@/composables/usePlatform'
import { listenForTorrentDrops } from '@/composables/useTorrentDrop'
import { afterReveal, nextFrame, reducedMotion } from '@/composables/useWindowReveal'
import { logger } from '@shared/logger'
import { getErrorMessage } from '@shared/utils/errorMessage'
import type { BatchItem, ExternalDownloadInput } from '@shared/types'

interface AddWindowInputs {
  inputs: ExternalDownloadInput[]
  links: string[]
  items: BatchItem[]
  manual: boolean
}

/** Logical width of the panel at 100% interface size. */
const PANEL_WIDTH = 600
/** Transparent margin around the panel for its shadow; `PANEL_INSET` in add_window.rs. */
const PANEL_INSET = 16
/** Height of the drawn title bar. */
const TITLE_BAR_HEIGHT = 32
/** How far the window may grow beyond its height with nothing expanded before its content scrolls. */
const MAX_GROWTH = 1.2
/** Share of the screen's work area the window never exceeds. */
const MAX_SCREEN_SHARE = 0.9
/** Quiet time after which the content has finished animating; longer than its transitions. */
const SETTLE_MS = 400
/** Duration of the leave animation: `--dialog-leave`. */
const LEAVE_MS = 170

const { t } = useI18n()
const appStore = useAppStore()
const preferenceStore = usePreferenceStore()
const message = useAppMessage()
const { platform, isMac, isLinux } = usePlatform()
const view = getCurrentWebviewWindow()
const addTask = ref<InstanceType<typeof AddTask> | null>(null)
const root = ref<HTMLElement | null>(null)
/** Tallest the window may be, in CSS pixels. */
const capHeight = ref(0)
/** Height of the freshly mounted window, links tab, advanced options closed, in CSS pixels. */
let baseHeight = 0
const phase = ref<'hidden' | 'shown' | 'leaving'>('hidden')
const unlisteners: UnlistenFn[] = []
let closing = false
let presenting: Promise<void> | undefined
const pendingInputs: AddWindowInputs[] = []
// Put away after an earlier download; preferences may have changed since.
let parked = false

// Window managers that force server-side decorations draw their own title bar (#533).
const systemTitleBar = computed(() => isLinux.value && preferenceStore.config.systemTitleBar)
const zoom = useWindowScale()
// The system frames the window itself under a system title bar, and on macOS,
// whose traffic lights would not follow a drawn panel: no panel shadow, margin
// or motion there; the system animates the window instead.
const framed = computed(() => !systemTitleBar.value && !isMac.value)
const inset = computed(() => (framed.value ? PANEL_INSET : 0))
const cardMaxHeight = computed(() =>
  capHeight.value ? capHeight.value - 2 * inset.value - (systemTitleBar.value ? 0 : TITLE_BAR_HEIGHT) : 0,
)

useEscapeKey(() => void addTask.value?.dismiss(), { fromEditable: true })

function isEmpty(batch: AddWindowInputs): boolean {
  return !batch.manual && batch.inputs.length === 0 && batch.links.length === 0 && batch.items.length === 0
}

function submissionsSettled(): Promise<void> {
  if (!appStore.externalInputSubmitting) return Promise.resolve()
  return new Promise((resolve) => {
    const stop = watch(
      () => appStore.externalInputSubmitting,
      (submitting) => {
        if (submitting) return
        stop()
        resolve()
      },
    )
  })
}

async function receive(batch: AddWindowInputs) {
  if (isEmpty(batch)) return
  if (closing) {
    pendingInputs.push(batch)
    return
  }
  if (parked) {
    parked = false
    await preferenceStore.reloadPreferenceFromDisk()
    // Closing may have started while preferences were being refreshed.
    if (closing) {
      pendingInputs.push(batch)
      return
    }
  }
  if (batch.manual && !appStore.addTaskVisible) appStore.showAddTaskDialog()
  if (batch.items.length > 0 && appStore.enqueueForwardedItems(batch.items) > 0) {
    message.warning(t('task.duplicate-task'))
  }
  if (batch.links.length > 0) appStore.handleDeepLinkUrls(batch.links)
  if (batch.inputs.length > 0) await appStore.handleExternalInputs(batch.inputs)
  if (appStore.addTaskVisible) return
  // Everything was submitted without confirmation.
  await submissionsSettled()
  if (!appStore.addTaskVisible) await close()
}

/** Replays inputs only after the previous close has released the window. */
async function resumeInputs(...inputs: AddWindowInputs[]) {
  const batches = [...pendingInputs.splice(0), ...inputs]
  closing = false
  for (const batch of batches) await receive(batch)
}

/** Cancel a close when requests already delivered to this frontend still need it. */
async function keepOpen(late?: AddWindowInputs): Promise<boolean> {
  if ((!late || isEmpty(late)) && pendingInputs.length === 0) return false
  // Detach may have stopped delivery. Reattach before replaying, including in
  // tray mode, where parking would destroy this frontend and its pending inputs.
  const queued = await invoke<AddWindowInputs>('add_window_attach')
  await resumeInputs(...(late ? [late, queued] : [queued]))
  return true
}

async function close() {
  if (closing) return
  closing = true
  clearTimeout(settleTimer)
  appStore.hideAddTaskDialog()
  // A native show already sent over IPC must finish before hide/destroy.
  await presenting
  try {
    const late = await invoke<AddWindowInputs>('add_window_detach')
    if (await keepOpen(late)) return
  } catch (error) {
    logger.warn('AddWindow.detach', getErrorMessage(error))
  }
  // The main window keeps its own copy of the preferences this window may have saved.
  await emit('add-window:closed').catch(() => undefined)
  await leave()
  // Paint the start state before hiding, so the next showing never flashes this one.
  phase.value = 'hidden'
  await nextFrame()
  await nextFrame()
  try {
    if (await keepOpen()) return
    const late = await invoke<AddWindowInputs>('add_window_park')
    parked = true
    await resumeInputs(late)
  } catch (error) {
    logger.warn('AddWindow.park', getErrorMessage(error))
    await view.destroy()
  }
}

// ── Motion ──────────────────────────────────────────────────────────

/** Fades the panel out before the window is put away. */
async function leave() {
  if (phase.value !== 'shown') return
  phase.value = 'leaving'
  if (framed.value && !reducedMotion()) await new Promise((resolve) => setTimeout(resolve, LEAVE_MS))
}

// ── Size ────────────────────────────────────────────────────────────
// Resizing the window relays out the whole webview, too slow to follow an
// animation frame by frame. When the content grows, the window grows at once to
// the most the panel may take, so the panel expands smoothly inside its
// transparent margin; once the content has stopped moving, the window shrinks
// to fit it again.

let appliedHeight = 0
let appliedWidth = 0
let wantedHeight = 0
let wantedWidth = 0
let sizing = false
let settleTimer: ReturnType<typeof setTimeout> | undefined

function fitHeight(): number {
  return root.value ? Math.ceil(root.value.getBoundingClientRect().height * zoom.value) : 0
}

/** Sends one resize at a time, always ending on the latest wanted height. */
async function applyHeight(height: number) {
  if (height <= 0) return
  wantedHeight = height
  wantedWidth = Math.ceil((PANEL_WIDTH + 2 * inset.value) * zoom.value)
  if (sizing) return
  sizing = true
  try {
    while (!closing && (wantedHeight !== appliedHeight || wantedWidth !== appliedWidth)) {
      const next = wantedHeight
      const width = wantedWidth
      await view.setSize(new LogicalSize(width, next))
      appliedHeight = next
      appliedWidth = width
    }
  } catch (error) {
    appliedHeight = 0
    logger.warn('AddWindow.setSize', getErrorMessage(error))
  } finally {
    sizing = false
  }
}

function onContentResize() {
  // Clearing the form can change its size during the leave animation.
  if (closing) return
  const fit = fitHeight()
  // Without a transparent margin the window has to follow the content directly.
  if (fit > appliedHeight) {
    void applyHeight(systemTitleBar.value ? fit : Math.max(fit, Math.round(capHeight.value * zoom.value)))
  }
  clearTimeout(settleTimer)
  settleTimer = setTimeout(() => void applyHeight(fitHeight()), systemTitleBar.value ? 0 : SETTLE_MS)
}

/**
 * Caps the window a little above its plain height, so expanding the advanced
 * options scrolls instead of stretching it, and below most of the screen.
 */
async function measureScreen() {
  // Not `currentMonitor()`: it reads the work area off the main thread, which
  // crashes the app on X11 (#725).
  const height = await invoke<number | null>('add_window_screen_height').catch(() => null)
  const logical = height ?? window.screen.availHeight
  const screenCap = Math.floor((logical * MAX_SCREEN_SHARE) / zoom.value)
  // A measurement holding no more than the margin and title bar saw no
  // content; capping by it would leave the window too small to use (#712).
  const chrome = 2 * inset.value + (systemTitleBar.value ? 0 : TITLE_BAR_HEIGHT)
  capHeight.value = baseHeight > chrome ? Math.min(screenCap, Math.round(baseHeight * MAX_GROWTH)) : screenCap
}

const resizeObserver = new ResizeObserver(onContentResize)

watch([zoom, systemTitleBar], async () => {
  if (closing) return
  await measureScreen()
  await nextTick()
  if (!closing) await applyHeight(fitHeight())
})

watch(
  () => appStore.addTaskVisible,
  async (visible, _previous, onCleanup) => {
    if (!visible || closing) return
    let active = true
    onCleanup(() => {
      active = false
    })
    phase.value = 'hidden'
    await view.setTitle(`${t('task.new-task')} - Rayburst`).catch(() => undefined)
    if (!active) return
    await measureScreen()
    if (!active) return
    await nextTick()
    if (!active) return
    clearTimeout(settleTimer)
    await applyHeight(fitHeight())
    if (!active) return
    const presentation = invoke<void>('add_window_present').catch((error: unknown) =>
      logger.warn('AddWindow.present', error),
    )
    presenting = presentation
    await presentation
    if (presenting === presentation) presenting = undefined
    if (!active) return
    // Let the shown window paint the panel's start state before it animates.
    await afterReveal()
    if (active) phase.value = 'shown'
  },
  // Invalidate even when a close and reopen occur in the same Vue tick.
  { flush: 'sync' },
)

watch(
  systemTitleBar,
  (enabled, previous) => {
    if (!isLinux.value || (previous === undefined && !enabled)) return
    view.setDecorations(enabled).catch((error: unknown) => logger.debug('AddWindow.setDecorations', error))
  },
  { immediate: true },
)

onMounted(async () => {
  document.documentElement.classList.add('add-window-document')
  if (root.value) {
    // Freshly mounted, nothing is expanded yet.
    baseHeight = root.value.getBoundingClientRect().height
    resizeObserver.observe(root.value)
  }
  appStore.setExternalInputErrorHandler((error) => {
    message.error(getErrorMessage(error, { fallback: t('task.error-unknown') }), { closable: true })
    appStore.showAddTaskDialog()
  })
  unlisteners.push(
    await view.listen<AddWindowInputs>('add-window:inputs', ({ payload }) => void receive(payload)),
    await listenForTorrentDrops({ enqueue: (items) => appStore.enqueueBatch(items), warn: message.warning, t }),
    await view.onCloseRequested((event) => {
      event.preventDefault()
      void addTask.value?.dismiss()
    }),
  )
  const queued = await invoke<AddWindowInputs>('add_window_attach')
  await receive(queued)
})

onBeforeUnmount(() => {
  clearTimeout(settleTimer)
  resizeObserver.disconnect()
  appStore.setExternalInputErrorHandler(null)
  for (const unlisten of unlisteners) unlisten()
})
</script>

<template>
  <div
    ref="root"
    class="add-window"
    :class="[`add-window--${phase}`, { 'add-window--framed': framed }]"
    :style="{
      // WebKitGTK lays out a hidden window at zero width, which wraps every
      // line and measured the content far too tall (#731); a fixed width keeps
      // measurements taken before the first showing true.
      width: `${PANEL_WIDTH + 2 * inset}px`,
      padding: `${inset}px`,
      ...(cardMaxHeight ? { '--add-window-max-height': `${cardMaxHeight}px` } : {}),
    }"
  >
    <div class="add-window-panel">
      <header v-if="!systemTitleBar" class="add-window-titlebar" :class="{ mac: isMac }" data-tauri-drag-region>
        <span class="add-window-title" data-tauri-drag-region>{{ t('task.new-task') }}</span>
        <WindowControls :is-maximized="false" :platform="platform" :maximizable="false" @close="addTask?.dismiss()" />
      </header>
      <AddTask
        ref="addTask"
        :show="appStore.addTaskVisible"
        @close="close"
        @submitted="emit('add-window:submitted').catch(() => undefined)"
      />
      <TorrentDropOverlay />
    </div>
  </div>
</template>

<style scoped>
.add-window-panel {
  position: relative;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--main-bg);
  transform-origin: center;
  transition:
    opacity var(--dialog-fade) cubic-bezier(0.2, 0, 0, 1),
    transform var(--dialog-enter) var(--dialog-ease-enter);
}

.add-window--framed .add-window-panel {
  border-radius: var(--radius-md);
  box-shadow:
    0 2px 12px var(--m3-shadow),
    0 0 0 1px var(--m3-outline-variant);
}

.add-window--framed.add-window--hidden .add-window-panel {
  opacity: 0;
  transform: translateY(12px) scale(0.95);
  transition: none;
}

.add-window--framed.add-window--leaving .add-window-panel {
  opacity: 0;
  transform: translateY(6px) scale(0.97);
  transition:
    opacity var(--dialog-leave) var(--dialog-ease-leave),
    transform var(--dialog-leave) var(--dialog-ease-leave);
}

.add-window-titlebar {
  position: relative;
  display: flex;
  align-items: center;
  height: 32px;
  flex-shrink: 0;
  padding-inline: 16px;
}

/* The caption buttons belong to the panel's corner, not the window's. */
.add-window-titlebar :deep(.caption-bar) {
  position: absolute;
}

/* macOS draws its traffic lights over the left of the title bar. */
.add-window-titlebar.mac {
  justify-content: center;
}

.add-window-title {
  font-size: var(--font-size-sm);
  color: var(--m3-on-surface-variant);
  white-space: nowrap;
  overflow: clip visible;
  text-overflow: ellipsis;
}
</style>

<style>
/* The window is transparent around the panel, and the page never scrolls:
 * focusing a field below the window's edge would otherwise move the whole
 * page. Only the card's content scrolls, once the window is full. */
html.add-window-document,
html.add-window-document body {
  background: transparent;
  overflow: clip;
}

html.add-window-document #app {
  height: auto;
}
</style>
