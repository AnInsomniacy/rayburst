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
 * compositor: the panel is lifted in with its content following in layers,
 * sinks away, and grows or shrinks with its content inside a window that is
 * resized once per change instead of on every frame. On macOS the system
 * frames the window around its traffic lights, and the window itself rises
 * and sinks instead of the panel (`add_window/motion.rs`).
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { emit, type UnlistenFn } from '@tauri-apps/api/event'
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
/** Until its layers land, the card's content holds still instead of scrolling. */
const entering = ref(false)
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
// or motion there. On macOS the window itself moves instead.
const framed = computed(() => !systemTitleBar.value && !isMac.value)
// Without a margin to hide it, the window's edge is the card's: the window
// follows the content as it moves and the card fills the window.
const follows = computed(() => !framed.value)
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
    ...(shown && follows.value ? { '--add-window-min-height': `calc(100vh - ${chrome.value}px)` } : {}),
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

/** Empties the form and the batch for the next showing. */
function discard() {
  appStore.hideAddTaskDialog()
  addTask.value?.reset()
}

/** Cancel a close when requests already delivered to this frontend still need it. */
async function keepOpen(late?: AddWindowInputs): Promise<boolean> {
  if ((!late || isEmpty(late)) && pendingInputs.length === 0) return false
  // The dismissed form gives way to the new requests.
  discard()
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
  // The form stays as it was while the panel leaves, and empties only once
  // unseen. Paint that start state before hiding, so the next showing never
  // flashes this one.
  phase.value = 'hidden'
  discard()
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

/** Fades the panel, or on macOS the window, out before the window is put away. */
async function leave() {
  if (phase.value !== 'shown') return
  phase.value = 'leaving'
  if (reducedMotion()) return
  if (framed.value) await new Promise((resolve) => setTimeout(resolve, LEAVE_MS))
  else if (isMac.value)
    await invoke('add_window_leave').catch((error: unknown) => logger.warn('AddWindow.leave', error))
}

/** Resolves once the entrance's transitions have ended or been cancelled. */
async function entranceFinished() {
  await nextTick()
  // Reading the animations flushes styles, so the entrance's have started.
  const animations = root.value?.getAnimations?.({ subtree: true }) ?? []
  await Promise.allSettled(animations.map((animation) => animation.finished))
}

// ── Size ────────────────────────────────────────────────────────────
// With a transparent margin, the window is not resized frame by frame: that
// relays out the whole webview, too slow for WebView2 and WebKitGTK. When the
// content grows, the window grows at once to the most the panel may take, so
// the panel expands smoothly inside the margin; once the content has stopped
// moving, the window shrinks to fit it again. Without a margin (macOS, a
// system title bar) the window's edge would show that, so the window follows
// the content frame by frame, a frame behind at most, while the card fills it
// and hides the difference. Heights are kept in CSS pixels and converted only
// when the window is resized.

let appliedHeight = 0
let appliedScale = 0
let wantedHeight = 0
let sizing = false
let settleTimer: ReturnType<typeof setTimeout> | undefined

/** The card's scrolling content area. */
function cardContent(): HTMLElement | null {
  return root.value?.querySelector<HTMLElement>('.n-card-content') ?? null
}

/**
 * Height the content takes, up to the cap: the window around the card's
 * content area, plus the form's own height and the area's padding. That
 * counts what the card scrolls, and not what a card filling the window
 * stretches. The form's computed height is its layout height: the window is
 * measured while the panel still has its entrance scale, which a bounding box
 * would include.
 */
function fitHeight(): number {
  const element = root.value
  if (!element) return 0
  const content = cardContent()
  const form = content?.firstElementChild
  let natural = element.getBoundingClientRect().height
  if (content && form) {
    const style = getComputedStyle(content)
    const own =
      (parseFloat(getComputedStyle(form).height) || 0) +
      (parseFloat(style.paddingTop) || 0) +
      (parseFloat(style.paddingBottom) || 0)
    natural += own - content.clientHeight
  }
  natural = Math.ceil(natural)
  return capHeight.value ? Math.min(natural, capHeight.value) : natural
}

/** Sends one resize at a time, always ending on the latest wanted height. */
async function applyHeight(height: number) {
  if (height <= 0) return
  wantedHeight = height
  if (sizing) return
  sizing = true
  try {
    // A changed scale resizes the window even when its CSS height is the same.
    while (!closing && (wantedHeight !== appliedHeight || zoom.value !== appliedScale)) {
      const next = wantedHeight
      const scale = zoom.value
      const width = Math.round((PANEL_WIDTH + 2 * inset.value) * scale)
      await invoke('add_window_fit', { width, height: Math.ceil(next * scale) })
      appliedHeight = next
      appliedScale = scale
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
  clearTimeout(settleTimer)
  if (follows.value) {
    void applyHeight(fit)
    return
  }
  if (fit > appliedHeight) void applyHeight(Math.max(fit, capHeight.value))
  settleTimer = setTimeout(() => void applyHeight(fitHeight()), SETTLE_MS)
}

/** Refits the window after it moved to another screen, whose work area caps it. */
function onScaleChanged() {
  if (closing) return
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
  const screenCap = Math.floor((logical * MAX_SCREEN_SHARE) / zoom.value)
  // A measurement holding no more than the margin and title bar saw no
  // content; capping by it would leave the window too small to use (#712).
  capHeight.value = baseHeight > chrome.value ? Math.min(screenCap, Math.round(baseHeight * MAX_GROWTH)) : screenCap
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
    // A close started meanwhile ends this presentation too.
    const current = () => active && !closing
    phase.value = 'hidden'
    entering.value = true
    await view.setTitle(`${t('task.new-task')} - Rayburst`).catch(() => undefined)
    if (!current()) return
    await measureScreen()
    if (!current()) return
    await nextTick()
    if (!current()) return
    clearTimeout(settleTimer)
    await applyHeight(fitHeight())
    if (!current()) return
    const presentation = invoke<void>('add_window_present', { animate: !reducedMotion() }).catch((error: unknown) =>
      logger.warn('AddWindow.present', error),
    )
    presenting = presentation
    await presentation
    if (presenting === presentation) presenting = undefined
    if (!current()) return
    // Let the shown window paint the panel's start state before it animates.
    await afterReveal()
    if (!current()) return
    phase.value = 'shown'
    await entranceFinished()
    if (active) entering.value = false
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
    :class="[`add-window--${phase}`, { 'add-window--framed': framed, 'add-window--entering': entering }]"
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
  transform-origin: 50% 100%;
  transition:
    opacity var(--add-window-fade) cubic-bezier(0.2, 0, 0, 1),
    transform var(--add-window-enter) var(--add-window-ease-enter),
    box-shadow var(--add-window-enter) var(--add-window-ease-enter);
}

.add-window--framed .add-window-panel {
  border-radius: var(--radius-md);
  box-shadow:
    0 2px 12px var(--m3-shadow),
    0 0 0 1px var(--m3-outline-variant);
}

/* At rest the card lies low and close to the desktop, its shadow tight. */
.add-window--framed.add-window--hidden .add-window-panel {
  opacity: 0;
  transform: translateY(14px) scale(0.96);
  box-shadow:
    0 1px 3px var(--m3-shadow),
    0 0 0 1px var(--m3-outline-variant);
  transition: none;
}

.add-window--framed.add-window--leaving .add-window-panel {
  opacity: 0;
  transform: translateY(6px) scale(0.98);
  transition:
    opacity var(--dialog-leave) var(--dialog-ease-leave),
    transform var(--dialog-leave) var(--dialog-ease-leave);
}

/* The content follows the card, or on macOS the window, in layers: the
 * links, the settings, then the buttons, each a short rise behind the last. */
.add-window :deep([data-reveal]) {
  transition:
    opacity var(--add-window-layer-enter) cubic-bezier(0.2, 0, 0, 1),
    transform var(--add-window-layer-enter) var(--add-window-ease-enter);
}

.add-window :deep([data-reveal='2']) {
  transition-delay: var(--add-window-layer-delay);
}

.add-window :deep([data-reveal='3']) {
  transition-delay: calc(2 * var(--add-window-layer-delay));
}

/* A rising layer reaches below the content it lands in, which would scroll
 * the card for a moment. The window fits the landed content. */
.add-window--entering :deep(.n-card-content) {
  overflow-y: hidden !important;
}

.add-window--hidden :deep([data-reveal]) {
  opacity: 0;
  transform: translateY(6px);
  transition: none;
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
