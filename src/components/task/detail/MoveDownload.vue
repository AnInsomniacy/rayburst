<script setup lang="ts">
/** @fileoverview Move an unfinished download to another folder (#696). */
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { NButton, NIcon } from 'naive-ui'
import { MoveOutline } from '@vicons/ionicons5'
import { useI18n } from 'vue-i18n'
import type { Aria2Task } from '@shared/types'
import { getErrorMessage } from '@shared/utils/errorMessage'
import { useAppMessage } from '@/composables/useAppMessage'

const props = defineProps<{ task: Aria2Task }>()
const emit = defineEmits<{ moved: [] }>()
const { t } = useI18n()
const message = useAppMessage()
const busy = ref(false)
// The engine moves HTTP, FTP and BitTorrent files; media and ED2K keep theirs.
const available = computed(
  () => ['active', 'waiting', 'paused'].includes(props.task.status) && !props.task.media && !props.task.ed2k,
)
async function move() {
  if (busy.value) return
  const selected = await openDialog({ directory: true, defaultPath: props.task.dir || undefined })
  if (typeof selected !== 'string' || selected === props.task.dir) return
  busy.value = true
  // Across disks the files are copied, which can take a while.
  const progress = message.progress(t('task.moving', { dir: selected }))
  try {
    await invoke('aria2_move_download', { gid: props.task.gid, dir: selected })
    progress.finish(t('task.move-success', { dir: selected }), 'success')
    emit('moved')
  } catch (cause) {
    progress.finish(getErrorMessage(cause), 'error')
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <NButton v-if="available" quaternary size="tiny" :loading="busy" :aria-label="t('task.move-to')" @click="move"
    ><template #icon
      ><NIcon :size="14"><MoveOutline /></NIcon></template
  ></NButton>
</template>
