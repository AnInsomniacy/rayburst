<script setup lang="ts">
/** @fileoverview Native-backed task toolbar actions and confirmations. */
import { ref, computed, h, nextTick, type Component } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import { useTaskStore } from '@/stores/task'

import { batchFinishMedia, saveSession, isEngineReady } from '@/api/aria2'
import { TASK_STATUS } from '@shared/constants'
import type { Aria2Task } from '@shared/types'
import type { I18nKey } from '@shared/i18nTypes'
import { canFinishMedia } from '@shared/utils/media'
import { getTaskSharingState } from '@shared/utils/task'

import { logger } from '@shared/logger'
import { getErrorMessage } from '@shared/utils/errorMessage'
import { NButton, NIcon, NCheckbox, NDropdown, useDialog, type DropdownOption } from 'naive-ui'
import MTooltip from '@/components/common/MTooltip.vue'
import { useAppMessage } from '@/composables/useAppMessage'
import { usePreferenceStore } from '@/stores/preference'
import {
  PROGRESS_SORT_FIELDS,
  TERMINAL_SORT_FIELDS,
  ALL_SORT_FIELDS,
  DEFAULT_TASK_SORT,
  type ProgressSortField,
  type TerminalSortField,
  type AllSortField,
} from '@/composables/useTaskSort'
import {
  AddOutline,
  PlayOutline,
  PauseOutline,
  StopCircleOutline,
  TrashOutline,
  RefreshOutline,
  CloseOutline,
  SwapVerticalOutline,
  ArrowUpOutline,
  ArrowDownOutline,
} from '@vicons/ionicons5'

const props = defineProps<{ scope: 'all' | 'progress' | 'failed' | 'completed' }>()
const { t } = useI18n()
const appStore = useAppStore()
const taskStore = useTaskStore()
const preferenceStore = usePreferenceStore()

// ── Sort dropdown ─────────────────────────────────────────────────
const currentTab = computed(() => props.scope)

/** Map sort field key to its i18n label. */
const SORT_LABELS: Record<ProgressSortField | TerminalSortField | AllSortField, I18nKey> = {
  manual: 'task.sort-manual',
  'added-at': 'task.sort-added-at',
  'completed-at': 'task.sort-completed-at',
  name: 'task.sort-name',
  size: 'task.sort-size',
  progress: 'task.sort-progress',
  speed: 'task.sort-speed',
}

/** Active sort config for the current tab. */
const currentSort = computed(() => {
  const cfg = preferenceStore.config?.taskSort ?? DEFAULT_TASK_SORT
  switch (currentTab.value) {
    case 'failed':
    case 'completed':
      return cfg[currentTab.value]
    case 'all':
      return cfg.all
    default:
      return cfg.progress
  }
})

/** Sort field list for the current tab. */
const currentSortFields = computed(() => {
  switch (currentTab.value) {
    case 'failed':
    case 'completed':
      return TERMINAL_SORT_FIELDS
    case 'all':
      return ALL_SORT_FIELDS
    default:
      return PROGRESS_SORT_FIELDS
  }
})

async function onSortSelect(key: ProgressSortField | TerminalSortField | AllSortField) {
  await taskStore.changeCurrentSort(key)
}
const message = useAppMessage()
const dialog = useDialog()

const refreshing = ref(false)

async function lockDialog(d: ReturnType<typeof dialog.info>) {
  d.loading = true
  d.negativeButtonProps = { disabled: true }
  d.closable = false
  d.maskClosable = false
  await nextTick()
}

const currentList = computed(() => props.scope)
const allGids = computed(() => taskStore.taskList.map((t: { gid: string }) => t.gid))
const hasActiveTasks = computed(() =>
  taskStore.taskList.some((t: Aria2Task) => t.status === TASK_STATUS.ACTIVE || t.status === TASK_STATUS.WAITING),
)
const hasPausedTasks = computed(() =>
  taskStore.taskList.some((t: { status: string }) => t.status === TASK_STATUS.PAUSED),
)
const sharingGids = computed(() =>
  taskStore.taskList.filter((task) => getTaskSharingState(task) !== null).map((task) => task.gid),
)

/** Active and all views show resume, pause, and delete actions. */
const showActiveActions = computed(() => currentList.value === 'progress' || currentList.value === 'all')

/** Terminal and All scopes expose history purge. */
const showStoppedActions = computed(
  () => currentList.value === 'failed' || currentList.value === 'completed' || currentList.value === 'all',
)

/** GIDs of live (aria2-managed) tasks only — used by Delete All in 'all' view */
const LIVE_STATUSES = new Set([TASK_STATUS.ACTIVE, TASK_STATUS.WAITING, TASK_STATUS.PAUSED])
const TERMINAL_STATUSES = new Set([TASK_STATUS.COMPLETE, TASK_STATUS.ERROR, TASK_STATUS.REMOVED])
const liveGids = computed(() =>
  taskStore.taskList.filter((t: { status: string }) => LIVE_STATUSES.has(t.status)).map((t: { gid: string }) => t.gid),
)
const terminalTasks = computed(() => taskStore.taskList.filter((t: Aria2Task) => TERMINAL_STATUSES.has(t.status)))

const deleteFilesLabel = computed(() =>
  t(
    preferenceStore.config.fileDeletionMode === 'permanent'
      ? 'task.delete-local-files-permanent-label'
      : 'task.delete-local-files-trash-label',
  ),
)

/** Queue clear disabled state: in 'all' view, check live tasks; otherwise check all tasks */
const deleteAllDisabled = computed(() =>
  currentList.value === 'all' ? liveGids.value.length === 0 : allGids.value.length === 0,
)

function showAddTask() {
  appStore.showAddTaskDialog()
}

async function onRefresh() {
  if (refreshing.value) return
  refreshing.value = true
  try {
    await taskStore.fetchList()
    message.success(t('task.refresh-list-success') || 'List refreshed')
  } catch (error) {
    logger.warn('TaskActions.onRefresh', getErrorMessage(error))
  } finally {
    refreshing.value = false
  }
}

function onDeleteAll() {
  // In 'all' view, clear only live aria2 tasks, not DB-only history items.
  deleteTasks(currentList.value === 'all' ? [...liveGids.value] : [...allGids.value], t('task.delete-task-queue'))
}

function deleteTasks(gids: string[], title: string) {
  if (!isEngineReady()) {
    message.warning(t('app.engine-not-ready'))
    return
  }
  if (gids.length === 0) return
  const deleteFiles = ref(false)
  const d = dialog.error({
    title,
    content: () =>
      h('div', {}, [
        h('p', { style: 'margin: 0 0 12px;' }, t('task.batch-delete-task-confirm', { count: gids.length })),
        h(
          NCheckbox,
          {
            checked: deleteFiles.value,
            'onUpdate:checked': (v: boolean) => {
              deleteFiles.value = v
            },
          },
          { default: () => deleteFilesLabel.value },
        ),
      ]),
    positiveText: t('app.yes'),
    negativeText: t('app.no'),
    onPositiveClick: async () => {
      await lockDialog(d)
      try {
        const result = await taskStore.batchRemoveTask(gids, {
          deleteMode: deleteFiles.value ? preferenceStore.config.fileDeletionMode : undefined,
        })
        if (result.failed.length > 0) {
          const key = result.succeeded.length > 0 ? 'task.batch-delete-task-partial' : 'task.batch-delete-task-fail'
          message[result.succeeded.length > 0 ? 'warning' : 'error'](
            t(key, { removed: result.succeeded.length, failed: result.failed.length }),
          )
        } else {
          message.success(t('task.batch-delete-task-success'))
        }
        taskStore.clearChecked()
      } catch (error) {
        logger.warn('TaskActions.onDeleteAll', getErrorMessage(error))
        message.error(t('task.batch-delete-task-fail'))
      } finally {
        d.destroy()
      }
      return false
    },
  })
}

function resumeAll() {
  if (!isEngineReady()) {
    message.warning(t('app.engine-not-ready'))
    return
  }
  const d = dialog.info({
    title: t('task.resume-all-task'),
    content: t('task.resume-all-task-confirm') || 'Resume all tasks?',
    positiveText: t('app.yes'),
    negativeText: t('app.no'),
    onPositiveClick: async () => {
      await lockDialog(d)
      try {
        const result = await taskStore.resumeAllTask()
        if (result.resumed > 0) message.success(t('task.resume-all-task-success'))
      } catch (error) {
        logger.warn('TaskActions.resumeAll', getErrorMessage(error))
        message.error(t('task.resume-all-task-fail'))
      } finally {
        d.destroy()
      }
      return false
    },
  })
}

function pauseAll() {
  if (!isEngineReady()) {
    message.warning(t('app.engine-not-ready'))
    return
  }
  const d = dialog.info({
    title: t('task.pause-all-task'),
    content: t('task.pause-all-task-confirm') || 'Pause all tasks?',
    positiveText: t('app.yes'),
    negativeText: t('app.no'),
    onPositiveClick: async () => {
      await lockDialog(d)
      try {
        await taskStore.pauseAllTask()
        message.success(t('task.pause-all-task-success'))
      } catch (error) {
        logger.warn('TaskActions.pauseAll', getErrorMessage(error))
        message.error(t('task.pause-all-task-fail'))
      } finally {
        d.destroy()
      }
      return false
    },
  })
}

function finishAllSharing() {
  if (!isEngineReady()) {
    message.warning(t('app.engine-not-ready'))
    return
  }
  const gids = [...sharingGids.value]
  if (gids.length === 0) return
  const d = dialog.warning({
    title: t('task.finish-all-sharing'),
    content: t('task.finish-all-sharing-confirm', { count: gids.length }),
    positiveText: t('app.yes'),
    negativeText: t('app.no'),
    onPositiveClick: async () => {
      await lockDialog(d)
      try {
        const result = await taskStore.finishSharingTasks(gids)
        if (result.failed.length === 0) {
          message.success(t('task.finish-all-sharing-success', { count: result.succeeded.length }))
        } else if (result.succeeded.length > 0) {
          message.warning(
            t('task.finish-all-sharing-partial', {
              finished: result.succeeded.length,
              failed: result.failed.length,
            }),
          )
        } else {
          message.error(t('task.finish-all-sharing-fail'))
        }
      } catch (error) {
        logger.warn('TaskActions.finishAllSharing', getErrorMessage(error))
        message.error(t('task.finish-all-sharing-fail'))
      } finally {
        d.destroy()
      }
      return false
    },
  })
}

const recordingGids = computed(() => taskStore.taskList.filter(canFinishMedia).map((task) => task.gid))
const finishingMedia = ref(false)
async function finishRecordings() {
  if (finishingMedia.value) return
  finishingMedia.value = true
  try {
    const result = await batchFinishMedia(recordingGids.value)
    if (result.failed.length) message.error(result.failed.map((item) => item.message).join('; '))
    if (result.succeeded.length) message.info(t('media.finalizing'))
    await saveSession()
    await taskStore.fetchList()
  } catch (error) {
    logger.warn('TaskActions.finishMedia', getErrorMessage(error))
    message.error(getErrorMessage(error))
  } finally {
    finishingMedia.value = false
  }
}

function purgeRecord() {
  const deleteFiles = ref(false)
  const d = dialog.error({
    title: t('task.purge-record'),
    content: () =>
      h('div', {}, [
        h('p', { style: 'margin: 0 0 12px;' }, t('task.purge-record-confirm') || 'Clear all finished records?'),
        h(
          NCheckbox,
          {
            checked: deleteFiles.value,
            'onUpdate:checked': (v: boolean) => {
              deleteFiles.value = v
            },
          },
          { default: () => deleteFilesLabel.value },
        ),
      ]),
    positiveText: t('app.yes'),
    negativeText: t('app.no'),
    onPositiveClick: async () => {
      await lockDialog(d)

      try {
        if (deleteFiles.value) {
          const result = await taskStore.batchRemoveTask(
            terminalTasks.value.map((task) => task.gid),
            { deleteMode: preferenceStore.config.fileDeletionMode },
          )
          if (result.failed.length) {
            message.error(t('task.remove-task-file-fail'))
            return false
          }
        } else {
          await taskStore.purgeTaskRecord()
        }
        message.success(t('task.purge-record-success'))
      } catch (error) {
        logger.warn('TaskActions.purgeRecord', getErrorMessage(error))
        message.error(t('task.purge-record-fail'))
      } finally {
        d.destroy()
      }
      return false
    },
  })
}
function menuIcon(icon: Component) {
  return () => h(NIcon, null, { default: () => h(icon) })
}

const sortOptions = computed<DropdownOption[]>(() =>
  currentSortFields.value.map((field) => ({
    key: field,
    label: t(SORT_LABELS[field]),
    icon:
      field === currentSort.value.field
        ? menuIcon(
            field === 'manual'
              ? SwapVerticalOutline
              : currentSort.value.direction === 'asc'
                ? ArrowUpOutline
                : ArrowDownOutline,
          )
        : undefined,
  })),
)

function selectSort(key: string | number) {
  const field = currentSortFields.value.find((field) => field === key)
  if (field) void onSortSelect(field)
}

type BatchAction = DropdownOption & {
  key: string
  action: () => unknown
}

// ── Picked tasks ──────────────────────────────────────────────────
const checkedTasks = computed(() => {
  const checked = new Set(taskStore.checkedGids)
  return taskStore.taskList.filter((task) => checked.has(task.gid))
})

async function runOnChecked(action: (tasks: Aria2Task[]) => Promise<number>, failKey: I18nKey) {
  if (!isEngineReady()) {
    message.warning(t('app.engine-not-ready'))
    return
  }
  try {
    const failed = await action(checkedTasks.value)
    if (failed > 0) message.warning(t(failKey))
  } catch (error) {
    logger.warn('TaskActions.checked', getErrorMessage(error))
    message.error(t(failKey))
  }
}

function checkedOptions(): BatchAction[] {
  const tasks = checkedTasks.value
  return [
    {
      key: 'checked-resume',
      label: t('task.resume-selected'),
      icon: menuIcon(PlayOutline),
      disabled: !tasks.some((task) => task.status === TASK_STATUS.PAUSED),
      action: () => runOnChecked(taskStore.resumeTasks, 'task.resume-all-task-fail'),
    },
    {
      key: 'checked-pause',
      label: t('task.pause-selected'),
      icon: menuIcon(PauseOutline),
      disabled: !tasks.some((task) => task.status === TASK_STATUS.ACTIVE || task.status === TASK_STATUS.WAITING),
      action: () => runOnChecked(taskStore.pauseTasks, 'task.pause-all-task-fail'),
    },
    {
      key: 'checked-delete',
      label: t('task.delete-selected'),
      icon: menuIcon(TrashOutline),
      disabled: tasks.length === 0,
      action: () =>
        deleteTasks(
          tasks.map((task) => task.gid),
          t('task.delete-selected'),
        ),
    },
    {
      key: 'checked-clear',
      label: t('task.clear-selection'),
      icon: menuIcon(CloseOutline),
      action: () => taskStore.clearChecked(),
    },
  ]
}

const batchOptions = computed<BatchAction[]>(() => {
  if (checkedTasks.value.length > 0) return checkedOptions()
  const options: BatchAction[] = []
  if (showActiveActions.value) {
    options.push(
      {
        key: 'resume',
        label: t('task.resume-all-task'),
        icon: menuIcon(PlayOutline),
        disabled: !hasPausedTasks.value,
        action: resumeAll,
      },
      {
        key: 'pause',
        label: t('task.pause-all-task'),
        icon: menuIcon(PauseOutline),
        disabled: !hasActiveTasks.value,
        action: pauseAll,
      },
      {
        key: 'sharing',
        label: t('task.finish-all-sharing'),
        icon: menuIcon(StopCircleOutline),
        disabled: !sharingGids.value.length,
        action: finishAllSharing,
      },
    )
    if (recordingGids.value.length)
      options.push({
        key: 'recording',
        label: `${t('media.finish')} (${recordingGids.value.length})`,
        disabled: finishingMedia.value,
        action: () => void finishRecordings(),
      })
    options.push({
      key: 'delete',
      label: t('task.delete-all-task'),
      icon: menuIcon(CloseOutline),
      disabled: deleteAllDisabled.value,
      action: onDeleteAll,
    })
  }
  if (showStoppedActions.value)
    options.push({
      key: 'purge',
      label: t('task.purge-record'),
      icon: menuIcon(TrashOutline),
      disabled: !terminalTasks.value.length,
      action: purgeRecord,
    })
  return options
})
function selectBatchAction(key: string | number) {
  const option = batchOptions.value.find((option) => option.key === key)
  if (option && !option.disabled) void option.action()
}
</script>

<template>
  <TransitionGroup name="toolbar-action" tag="div" class="task-actions">
    <span key="add" class="toolbar-action"
      ><MTooltip>
        <template #trigger>
          <NButton type="primary" circle size="small" :aria-label="t('task.new-task')" @click="showAddTask">
            <template #icon>
              <NIcon><AddOutline /></NIcon>
            </template>
          </NButton>
        </template>
        {{ t('task.new-task') || 'New Task' }}
      </MTooltip></span
    >
    <span key="sort" class="toolbar-action"
      ><NDropdown
        trigger="click"
        placement="bottom-end"
        :options="sortOptions"
        :value="currentSort.field"
        @select="selectSort"
      >
        <NButton quaternary circle size="small" :aria-label="t('task.sort-by')">
          <template #icon
            ><NIcon><SwapVerticalOutline /></NIcon
          ></template>
        </NButton> </NDropdown
    ></span>
    <span key="refresh" class="toolbar-action"
      ><MTooltip>
        <template #trigger>
          <NButton
            quaternary
            circle
            size="small"
            :aria-label="t('task.refresh-list')"
            :loading="refreshing"
            :disabled="refreshing"
            @click="onRefresh"
          >
            <template #icon>
              <NIcon><RefreshOutline /></NIcon>
            </template>
          </NButton>
        </template>
        {{ t('task.refresh-list') || 'Refresh' }}
      </MTooltip></span
    >
    <span v-if="checkedTasks.length" key="checked-count" class="toolbar-action checked-count">
      {{ t('task.selected-count', { count: checkedTasks.length }) }}
    </span>
    <span v-for="action in batchOptions" :key="action.key" class="toolbar-action">
      <MTooltip>
        <template #trigger>
          <NButton
            quaternary
            circle
            size="small"
            :aria-label="String(action.label)"
            :disabled="Boolean(action.disabled)"
            @click="selectBatchAction(action.key)"
          >
            <template #icon>
              <component :is="action.icon" v-if="action.icon" />
              <NIcon v-else><StopCircleOutline /></NIcon>
            </template>
          </NButton>
        </template>
        {{ action.label }}
      </MTooltip>
    </span>
  </TransitionGroup>
</template>

<style scoped>
.task-actions {
  position: relative;
  display: flex;
  flex-shrink: 0;
  align-items: center;
  gap: 4px;
}
.toolbar-action {
  display: inline-flex;
  flex-shrink: 0;
}
.checked-count {
  align-items: center;
  padding: 0 6px;
  color: var(--m3-primary);
  font-size: var(--font-size-sm);
  white-space: nowrap;
}
.toolbar-action-move,
.toolbar-action-enter-active,
.toolbar-action-leave-active {
  transition:
    transform 180ms cubic-bezier(0.2, 0, 0, 1),
    opacity 140ms ease;
}
.toolbar-action-leave-active {
  position: absolute;
  pointer-events: none;
}
.toolbar-action-enter-from,
.toolbar-action-leave-to {
  opacity: 0;
  transform: scale(0.8);
}
@media (prefers-reduced-motion: reduce) {
  .toolbar-action-move,
  .toolbar-action-enter-active,
  .toolbar-action-leave-active {
    transition: none;
  }
}
</style>
