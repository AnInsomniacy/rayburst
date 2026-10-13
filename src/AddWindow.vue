<script setup lang="ts">
/**
 * @fileoverview Content of the standalone add-task window.
 *
 * The desktop builds this window ahead of time, hidden, and hands it every new
 * download: browser handoffs, links, torrent files and manual entry from the
 * main window. It shows itself once there is something to confirm, and puts
 * itself away after the user submits or cancels, or after handing
 * auto-submitted downloads to the engine. Its height follows its content, so
 * expanding the advanced options grows the window with them.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { emit, type UnlistenFn } from '@tauri-apps/api/event'
import { LogicalSize, currentMonitor } from '@tauri-apps/api/window'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import AddTask from '@/components/task/AddTask.vue'
import WindowControls from '@/components/layout/WindowControls.vue'
import { useAppStore } from '@/stores/app'
import { usePreferenceStore } from '@/stores/preference'
import { useAppMessage } from '@/composables/useAppMessage'
import { useEscapeKey } from '@/composables/useEscapeKey'
import { usePlatform } from '@/composables/usePlatform'
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
/** Share of the screen's work area the window may take before its content scrolls. */
const MAX_SCREEN_SHARE = 0.9

const { t } = useI18n()
const appStore = useAppStore()
const preferenceStore = usePreferenceStore()
const message = useAppMessage()
const { platform, isMac, isLinux } = usePlatform()
const view = getCurrentWebviewWindow()
const addTask = ref<InstanceType<typeof AddTask> | null>(null)
const root = ref<HTMLElement | null>(null)
const maxHeight = ref(0)
const unlisteners: UnlistenFn[] = []
let closing = false
// Put away after an earlier download; preferences may have changed since.
let parked = false

// Window managers that force server-side decorations draw their own title bar (#533).
const systemTitleBar = computed(() => isLinux.value && preferenceStore.config.systemTitleBar)
const zoom = computed(() => preferenceStore.config.uiScale || 1)

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

// ── Size ────────────────────────────────────────────────────────────
// The window follows its content's height. Resizes are sent one at a time and
// coalesced, so an animated expansion moves the window frame by frame.

let sentHeight = 0
let sizing = false
let resizeAgain = false

async function syncSize() {
  if (!root.value) return
  if (sizing) {
    resizeAgain = true
    return
  }
  sizing = true
  try {
    do {
      resizeAgain = false
      const height = Math.ceil(root.value.getBoundingClientRect().height * zoom.value)
      if (height > 0 && height !== sentHeight) {
        sentHeight = height
        await view.setSize(new LogicalSize(Math.round(WINDOW_WIDTH * zoom.value), height))
      }
    } while (resizeAgain)
  } catch (error) {
    logger.debug('AddWindow.setSize', error)
  } finally {
    sizing = false
  }
}

/** Caps the content at a share of the work area of the screen it opens on. */
async function measureScreen() {
  const monitor = await currentMonitor().catch(() => null)
  if (!monitor) return
  const logical = monitor.workArea.size.height / monitor.scaleFactor
  maxHeight.value = Math.floor((logical * MAX_SCREEN_SHARE) / zoom.value)
}

const resizeObserver = new ResizeObserver(() => void syncSize())

watch(
  () => appStore.addTaskVisible,
  async (visible) => {
    if (!visible) return
    await view.setTitle(`${t('task.new-task')} - Rayburst`).catch(() => undefined)
    await measureScreen()
    await nextTick()
    await syncSize()
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
  if (root.value) resizeObserver.observe(root.value)
  appStore.setExternalInputErrorHandler((error) => {
    message.error(getErrorMessage(error, { fallback: t('task.error-unknown') }), { closable: true })
    appStore.showAddTaskDialog()
  })
  unlisteners.push(
    await view.listen<AddWindowInputs>('add-window:inputs', ({ payload }) => void receive(payload)),
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
    :style="maxHeight ? { '--add-window-max-height': `${maxHeight - (systemTitleBar ? 0 : 32)}px` } : undefined"
  >
    <header v-if="!systemTitleBar" class="add-window-titlebar" :class="{ mac: isMac }" data-tauri-drag-region>
      <span class="add-window-title" data-tauri-drag-region>{{ t('task.new-task') }}</span>
      <WindowControls :is-maximized="false" :platform="platform" :maximizable="false" @close="addTask?.dismiss()" />
    </header>
    <AddTask
      ref="addTask"
      standalone
      :show="appStore.addTaskVisible"
      @close="close"
      @submitted="emit('add-window:submitted').catch(() => undefined)"
    />
  </div>
</template>

<style scoped>
.add-window {
  display: flex;
  flex-direction: column;
  background: var(--main-bg);
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
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>

<style>
/* The page itself stays the content's color while the window catches up. */
html.add-window-document,
html.add-window-document body {
  background: var(--main-bg);
}

html.add-window-document #app {
  height: auto;
}
</style>
