<script setup lang="ts">
/**
 * @fileoverview Content of the standalone add-task window.
 *
 * The desktop opens this window, hidden, for every new download: browser
 * handoffs, links, torrent files and manual entry from the main window. It
 * shows itself once there is something to confirm, and closes after the user
 * submits or cancels, or after handing auto-submitted downloads to the engine.
 */
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { emit, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import AddTask from '@/components/task/AddTask.vue'
import { useAppStore } from '@/stores/app'
import { useAppMessage } from '@/composables/useAppMessage'
import { useEscapeKey } from '@/composables/useEscapeKey'
import { logger } from '@shared/logger'
import { getErrorMessage } from '@shared/utils/errorMessage'
import type { BatchItem, ExternalDownloadInput } from '@shared/types'

interface AddWindowInputs {
  inputs: ExternalDownloadInput[]
  links: string[]
  items: BatchItem[]
  manual: boolean
}

const { t } = useI18n()
const appStore = useAppStore()
const message = useAppMessage()
const view = getCurrentWebviewWindow()
const addTask = ref<InstanceType<typeof AddTask> | null>(null)
const unlisteners: UnlistenFn[] = []
let closing = false

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
  await view.destroy()
}

watch(
  () => appStore.addTaskVisible,
  async (visible) => {
    if (!visible) return
    await view.setTitle(`${t('task.new-task')} - Rayburst`).catch(() => undefined)
    await view.show()
    await view.setFocus()
  },
)

onMounted(async () => {
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
  appStore.setExternalInputErrorHandler(null)
  for (const unlisten of unlisteners) unlisten()
})
</script>

<template>
  <AddTask
    ref="addTask"
    standalone
    :show="appStore.addTaskVisible"
    @close="close"
    @submitted="emit('add-window:submitted').catch(() => undefined)"
  />
</template>
