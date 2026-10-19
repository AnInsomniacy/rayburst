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
import { useAppMessage } from '@/composables/useAppMessage'
import { useEscapeKey } from '@/composables/useEscapeKey'
import { usePlatform } from '@/composables/usePlatform'
import { listenForTorrentDrops } from '@/composables/useTorrentDrop'
import { afterReveal, nextFrame, reducedMotion } from '@/composables/useWindowReveal'
import { cssPixelScale } from '@/composables/useCssPixelScale'
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
// Put away after an earlier download; preferences may have changed since.
let parked = false

// Window managers that force server-side decorations draw their own title bar (#533).
const systemTitleBar = computed(() => isLinux.value && preferenceStore.config.systemTitleBar)
const zoom = computed(() => preferenceStore.config.uiScale || 1)
// The system frames the window itself under a system title bar, and on macOS,
// whose traffic lights would not follow a drawn panel: no panel shadow, margin
// or motion there; the system animates the window instead.
const framed = computed(() => !systemTitleBar.value && !isMac.value)
const inset = computed(() => (framed.value ? PANEL_INSET : 0))
/** Height around the card: the margin and the drawn title bar. */
const chrome = computed(() => 2 * inset.value + (systemTitleBar.value ? 0 : TITLE_BAR_HEIGHT))
const cardMaxHeight = computed(() => (capHeight.value ? capHeight.value - chrome.value : 0))
// Once shown, the card also stays within the window, so its buttons remain
// in view should the window come out smaller than asked. A hidden window is
// left unbounded: WebKitGTK gives it almost no viewport (#731).
const rootStyle = computed(() => {
  const shown = phase.value !== 'hidden'
  const cap = cardMaxHeight.value
  return {
    width: `${PANEL_WIDTH + 2 * inset.value}px`,
    maxWidth: shown ? '100vw' : undefined,
    padding: `${inset.value}px`,
    ...(cap
      ? { '--add-window-max-height': shown ? `min(${cap}px, calc(100vh - ${chrome.value}px))` : `${cap}px` }
      : {}),
  }
})

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
  if (parked) {
    parked = false
    await preferenceStore.reloadPreferenceFromDisk()
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

async function close() {
  if (closing) return
  closing = true
  appStore.hideAddTaskDialog()
  try {
    const late = await invoke<AddWindowInputs>('add_window_detach')
    if (!isEmpty(late)) {
      closing = false
      await receive(late)
      return
    }
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
    const late = await invoke<AddWindowInputs>('add_window_park')
    parked = true
    closing = false
    await receive(late)
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
// to fit it again. Heights are kept in CSS pixels and converted only when the
// window is resized.

let appliedHeight = 0
let wantedHeight = 0
let sizing = false
let settleTimer: ReturnType<typeof setTimeout> | undefined

/** The card's scrolling content area. */
function cardContent(): HTMLElement | null {
  return root.value?.querySelector<HTMLElement>('.n-card__content') ?? null
}

/** Height the content takes, including what the card scrolls, up to the cap. */
function fitHeight(): number {
  const element = root.value
  if (!element) return 0
  const content = cardContent()
  const scrolled = content ? content.scrollHeight - content.clientHeight : 0
  const natural = Math.ceil(element.getBoundingClientRect().height + scrolled)
  return capHeight.value ? Math.min(natural, capHeight.value) : natural
}

/** Sends one resize at a time, always ending on the latest wanted height. */
async function applyHeight(height: number) {
  if (height <= 0) return
  wantedHeight = height
  if (sizing) return
  sizing = true
  try {
    while (wantedHeight !== appliedHeight) {
      const next = wantedHeight
      const scale = await cssPixelScale(view, zoom.value)
      const width = Math.round((PANEL_WIDTH + 2 * inset.value) * scale)
      await view.setSize(new LogicalSize(width, Math.ceil(next * scale)))
      appliedHeight = next
    }
  } catch (error) {
    appliedHeight = 0
    logger.warn('AddWindow.setSize', getErrorMessage(error))
  } finally {
    sizing = false
  }
}

function onContentResize() {
  const fit = fitHeight()
  // Without a transparent margin the window has to follow the content directly.
  if (fit > appliedHeight) {
    void applyHeight(systemTitleBar.value ? fit : Math.max(fit, capHeight.value))
  }
  clearTimeout(settleTimer)
  settleTimer = setTimeout(() => void applyHeight(fitHeight()), systemTitleBar.value ? 0 : SETTLE_MS)
}

/** Refits the window after it moved to a screen with another scale. */
function onScaleChanged() {
  appliedHeight = 0
  void measureScreen().then(() => applyHeight(fitHeight()))
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
  const scale = await cssPixelScale(view, zoom.value)
  const screenCap = Math.floor((logical * MAX_SCREEN_SHARE) / scale)
  // A measurement holding no more than the margin and title bar saw no
  // content; capping by it would leave the window too small to use (#712).
  capHeight.value = baseHeight > chrome.value ? Math.min(screenCap, Math.round(baseHeight * MAX_GROWTH)) : screenCap
}

const resizeObserver = new ResizeObserver(onContentResize)

watch(
  () => appStore.addTaskVisible,
  async (visible) => {
    if (!visible) return
    phase.value = 'hidden'
    await view.setTitle(`${t('task.new-task')} - Rayburst`).catch(() => undefined)
    await measureScreen()
    await nextTick()
    clearTimeout(settleTimer)
    await applyHeight(fitHeight())
    await invoke('add_window_present').catch((error: unknown) => logger.warn('AddWindow.present', error))
    // Let the shown window paint the panel's start state before it animates.
    await afterReveal()
    phase.value = 'shown'
  },
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
    // Held within the window, the card keeps its size while its form grows;
    // watching the form lets the window grow first.
    const form = cardContent()?.firstElementChild
    if (form) resizeObserver.observe(form)
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
    await view.onScaleChanged(onScaleChanged),
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
    :style="rootStyle"
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
