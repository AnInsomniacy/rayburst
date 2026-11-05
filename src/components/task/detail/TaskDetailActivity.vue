<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { NDescriptions, NDescriptionsItem } from 'naive-ui'
import LinearProgress from '@/components/common/LinearProgress.vue'
import { TASK_STATUS } from '@shared/constants'
import { bytesToSize, calcProgress, getTaskCompletedLength, timeFormat, timeRemaining } from '@shared/utils'
import type { Aria2Task } from '@shared/types'
import type { TaskTransferSummary } from '@/composables/useTaskDetailSummary'
import TaskGraphic from '../TaskGraphic.vue'

const props = defineProps<{
  task: Aria2Task | null
  transferSummary: TaskTransferSummary
}>()

const { t } = useI18n()
const isActive = computed(() => props.task?.status === TASK_STATUS.ACTIVE)
const completedLengthValue = computed(() => (props.task ? getTaskCompletedLength(props.task) : 0))
const percent = computed(() => (props.task ? calcProgress(props.task.totalLength, completedLengthValue.value) : 0))
const remaining = computed(() => {
  if (!isActive.value || !props.task) return 0
  return timeRemaining(Number(props.task.totalLength), completedLengthValue.value, Number(props.task.downloadSpeed))
})
const remainingText = computed(() => {
  if (remaining.value <= 0) return ''
  return timeFormat(remaining.value, {
    prefix: t('task.remaining-prefix') || '',
    i18n: {
      gt1d: t('app.gt1d') || '>1d',
      hour: t('app.hour') || 'h',
      minute: t('app.minute') || 'm',
      second: t('app.second') || 's',
    },
  })
})
</script>

<template>
  <template v-if="task">
    <TaskGraphic v-if="task.bitfield" :bitfield="task.bitfield" />
    <NDescriptions :column="1" label-placement="left" bordered size="small">
      <NDescriptionsItem :label="t('task.task-progress-info') || 'Progress'">
        <div class="progress-row">
          <LinearProgress :percentage="percent" :height="10" />
          <span class="progress-pct">{{ percent }}%</span>
        </div>
      </NDescriptionsItem>
      <NDescriptionsItem :label="t('task.task-file-size') || 'Size'">
        {{ bytesToSize(completedLengthValue, 2) }}
        <span v-if="Number(task.totalLength) > 0"> / {{ bytesToSize(task.totalLength, 2) }}</span>
        <span v-if="remainingText" class="remaining-text">{{ remainingText }}</span>
      </NDescriptionsItem>
      <NDescriptionsItem :label="t('task.task-download-speed') || 'DL Speed'">
        {{ bytesToSize(task.downloadSpeed) }}/s
      </NDescriptionsItem>
      <NDescriptionsItem v-if="transferSummary.showUploadMetrics" :label="t('task.task-upload-speed') || 'UL Speed'">
        {{ bytesToSize(task.uploadSpeed) }}/s
      </NDescriptionsItem>
      <NDescriptionsItem v-if="transferSummary.showUploadMetrics" :label="t('task.task-upload-length') || 'Uploaded'">
        {{ bytesToSize(task.uploadLength) }}
      </NDescriptionsItem>
      <NDescriptionsItem v-if="transferSummary.showUploadMetrics" :label="t('task.task-ratio') || 'Ratio'">
        {{ transferSummary.ratio }}
      </NDescriptionsItem>
      <NDescriptionsItem v-if="transferSummary.showSeeders" :label="t('task.task-num-seeders') || 'Seeders'">
        {{ task.numSeeders }}
      </NDescriptionsItem>
      <NDescriptionsItem :label="t('task.task-connections') || 'Connections'">
        {{ task.connections }}
      </NDescriptionsItem>
    </NDescriptions>
  </template>
</template>
