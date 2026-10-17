<script setup lang="ts">
/**
 * @fileoverview Content of the standalone add-task window.
 *
 * The desktop builds this window when a download arrives: browser handoffs,
 * links, torrent files and manual entry from the main window. It shows itself
 * once there is something to confirm, sized to its content, and is destroyed
 * after the user submits or cancels, or after handing auto-submitted downloads
 * to the engine. The system draws its frame and animates it, content and
 * all, as it appears and goes.
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
import { logger } from '@shared/logger'
import { getErrorMessage } from '@shared/utils/errorMessage'
import type { BatchItem, ExternalDownloadInput } from '@shared/types'

interface AddWindowInputs {
  inputs: ExternalDownloadInput[]
  links: string[]
  items: BatchItem[]
  manual: boolean
}

/** Logical width of the window at 100% interface size. */
const WINDOW_WIDTH = 600
/** Height of the drawn title bar. */
const TITLE_BAR_HEIGHT = 32
/** How far the window may grow beyond its height with nothing expanded before its content scrolls. */
const MAX_GROWTH = 1.2
/** Share of the screen's work area the window never exceeds. */
const MAX_SCREEN_SHARE = 0.9

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
const unlisteners: UnlistenFn[] = []
let closing = false

// Window managers that force server-side decorations draw their own title bar (#533).
const systemTitleBar = computed(() => isLinux.value && preferenceStore.config.systemTitleBar)
const zoom = computed(() => preferenceStore.config.uiScale || 1)
const cardMaxHeight = computed(() =>
  capHeight.value ? capHeight.value - (systemTitleBar.value ? 0 : TITLE_BAR_HEIGHT) : 0,
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
  try {
    const late = await invoke<AddWindowInputs>('add_window_close')
    closing = false
    await receive(late)
  } catch (error) {
    logger.warn('AddWindow.close', getErrorMessage(error))
    await view.destroy()
  }
}

// ── Size ────────────────────────────────────────────────────────────
// The window follows its content, one resize at a time.

let appliedHeight = 0
let wantedHeight = 0
let sizing = false

function fitHeight(): number {
  return root.value ? Math.ceil(root.value.getBoundingClientRect().height * zoom.value) : 0
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
      const width = Math.round(WINDOW_WIDTH * zoom.value)
      await view.setSize(new LogicalSize(width, next))
      appliedHeight = next
    }
  } catch (error) {
    appliedHeight = 0
    logger.warn('AddWindow.setSize', getErrorMessage(error))
  } finally {
    sizing = false
  }
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
  // A measurement holding no more than the title bar saw no content; capping
  // by it would leave the window too small to use (#712).
  const chrome = systemTitleBar.value ? 0 : TITLE_BAR_HEIGHT
  capHeight.value = baseHeight > chrome ? Math.min(screenCap, Math.round(baseHeight * MAX_GROWTH)) : screenCap
}

const resizeObserver = new ResizeObserver(() => void applyHeight(fitHeight()))

watch(
  () => appStore.addTaskVisible,
  async (visible) => {
    if (!visible) return
    await view.setTitle(`${t('task.new-task')} - Rayburst`).catch(() => undefined)
    await measureScreen()
    await nextTick()
    await applyHeight(fitHeight())
    await invoke('add_window_present').catch((error: unknown) => logger.warn('AddWindow.present', error))
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
  resizeObserver.disconnect()
  appStore.setExternalInputErrorHandler(null)
  for (const unlisten of unlisteners) unlisten()
})
</script>

<template>
  <div
    ref="root"
    class="add-window"
    :class="{ 'add-window--bordered': isLinux && !systemTitleBar }"
    :style="{
      // WebKitGTK lays out a hidden window at zero width, which wraps every
      // line and measured the content far too tall (#731); a fixed width keeps
      // measurements taken before the first showing true.
      width: `${WINDOW_WIDTH}px`,
      ...(cardMaxHeight ? { '--add-window-max-height': `${cardMaxHeight}px` } : {}),
    }"
  >
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
</template>

<style scoped>
.add-window {
  position: relative;
  display: flex;
  flex-direction: column;
  background: var(--main-bg);
}

/* GTK draws no frame around a window without decorations. */
.add-window--bordered::after {
  content: '';
  position: fixed;
  inset: 0;
  border: 1px solid var(--m3-outline-variant);
  pointer-events: none;
}

.add-window-titlebar {
  position: relative;
  display: flex;
  align-items: center;
  height: 32px;
  flex-shrink: 0;
  padding-inline: 16px;
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
/* The page never scrolls: focusing a field below the window's edge would
 * otherwise move the whole page. Only the card's content scrolls, once the
 * window is full. */
html.add-window-document,
html.add-window-document body {
  overflow: clip;
}

html.add-window-document #app {
  height: auto;
}
</style>
