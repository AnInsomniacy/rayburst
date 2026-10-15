<script setup lang="ts">
/** @fileoverview Shared BitTorrent file selector for local torrents and magnets: a folder tree with type shortcuts. */
import { computed, h, ref, watch, type Component } from 'vue'
import { useI18n } from 'vue-i18n'
import { NButton, NDataTable, NIcon } from 'naive-ui'
import type { DataTableColumns, DataTableRowKey } from 'naive-ui'
import {
  AppsOutline,
  ArchiveOutline,
  DocumentOutline,
  DocumentTextOutline,
  FolderOutline,
  ImageOutline,
  MusicalNotesOutline,
  VideocamOutline,
} from '@vicons/ionicons5'
import { bytesToSize } from '@shared/utils'
import { calcColumnWidth } from '@shared/utils/calcColumnWidth'
import {
  btFileType,
  buildBtFileTree,
  groupBtFilesByType,
  invertBtFileSelection,
  toggleBtFileGroup,
  type BtFileTreeRow,
  type BtFileType,
} from '@shared/utils/btFileTree'
import type { BtFileSelectionItem } from '@shared/types'

const props = withDefaults(
  defineProps<{
    files: BtFileSelectionItem[]
    selectedIndices: number[]
    maxHeight?: number
    /** Fill the parent's height instead of capping the table at `maxHeight`. */
    fill?: boolean
  }>(),
  { maxHeight: 240, fill: false },
)

const emit = defineEmits<{
  'update:selectedIndices': [indices: number[]]
}>()

const { t } = useI18n()
const countDirection = ref<'bt-value-up' | 'bt-value-down'>('bt-value-up')
const sizeDirection = ref<'bt-value-up' | 'bt-value-down'>('bt-value-up')

/** Expand everything for small torrents; large ones open at their top folders. */
const EXPAND_ALL_LIMIT = 200

const tree = computed(() => buildBtFileTree(props.files))
const hasFolders = computed(() => tree.value.some((row) => row.children))
const expandedKeys = computed(() => {
  const keys: (string | number)[] = []
  const walk = (rows: BtFileTreeRow[], deep: boolean) => {
    for (const row of rows) {
      if (!row.children) continue
      keys.push(row.key)
      if (deep) walk(row.children, deep)
    }
  }
  walk(tree.value, props.files.length <= EXPAND_ALL_LIMIT)
  return keys
})
// Remount on a different torrent so its folders open by the rule above.
const treeIdentity = computed(() => `${props.files.length}:${props.files[0]?.path ?? ''}`)
const typeGroups = computed(() => groupBtFilesByType(props.files))
const selectedSet = computed(() => new Set(props.selectedIndices))

const TYPE_ICONS: Record<BtFileType, Component> = {
  'file-category-videos': VideocamOutline,
  'file-category-music': MusicalNotesOutline,
  'file-category-images': ImageOutline,
  'file-category-documents': DocumentTextOutline,
  'file-category-archives': ArchiveOutline,
  'file-category-programs': AppsOutline,
}

function rowIcon(row: BtFileTreeRow): Component {
  if (row.children) return FolderOutline
  const type = btFileType(row.name)
  return type ? TYPE_ICONS[type] : DocumentOutline
}

function groupSelected(indices: number[]): boolean {
  return indices.every((index) => selectedSet.value.has(index))
}

const columns = computed<DataTableColumns<BtFileTreeRow>>(() => [
  { type: 'selection' },
  {
    title: t('task.file-name') || 'File Name',
    key: 'name',
    render: (row) =>
      h('span', { class: 'bt-file-name', title: row.path || row.name }, [
        h(NIcon, { size: 15, class: 'bt-file-icon', component: rowIcon(row) }),
        h('span', { class: 'bt-file-label' }, row.name),
      ]),
  },
  {
    title: t('task.file-size') || 'Size',
    key: 'length',
    width: calcColumnWidth({
      title: t('task.file-size') || 'Size',
      values: props.files.map((file) => bytesToSize(file.length)),
      sortable: true,
    }),
    align: 'right',
    className: 'bt-file-size',
    sorter: (a, b) => a.length - b.length,
    render: (row) => bytesToSize(row.length),
  },
])

const selectedFiles = computed(() => {
  const selected = new Set(props.selectedIndices)
  return props.files.filter((file) => selected.has(file.index))
})
const selectedSize = computed(() => selectedFiles.value.reduce((sum, file) => sum + file.length, 0))

watch(
  () => props.selectedIndices.length,
  (current, previous) => {
    countDirection.value = current >= previous ? 'bt-value-up' : 'bt-value-down'
  },
)

watch(selectedSize, (current, previous) => {
  sizeDirection.value = current >= previous ? 'bt-value-up' : 'bt-value-down'
})

function updateSelection(keys: DataTableRowKey[]) {
  // Folder keys are strings; only file indices are selections.
  emit(
    'update:selectedIndices',
    keys.filter((key): key is number => typeof key === 'number'),
  )
}

function toggleGroup(indices: number[]) {
  emit('update:selectedIndices', toggleBtFileGroup(props.selectedIndices, indices))
}

function invertSelection() {
  emit('update:selectedIndices', invertBtFileSelection(props.files, props.selectedIndices))
}
</script>

<template>
  <div :class="['bt-file-selector', { 'bt-file-selector--fill': fill }]">
    <div class="bt-file-toolbar">
      <template v-if="files.length > 1">
        <NButton
          v-for="group in typeGroups"
          :key="group.type"
          size="tiny"
          round
          :secondary="groupSelected(group.indices)"
          :tertiary="!groupSelected(group.indices)"
          :type="groupSelected(group.indices) ? 'primary' : 'default'"
          @click="toggleGroup(group.indices)"
        >
          <template #icon>
            <NIcon :component="TYPE_ICONS[group.type]" />
          </template>
          {{ t(`preferences.${group.type}`) }} {{ group.indices.length }}
        </NButton>
      </template>
      <div class="file-summary" aria-live="polite">
        <Transition :name="countDirection" mode="out-in">
          <span :key="selectedIndices.length" class="summary-value">
            {{ selectedIndices.length }}/{{ files.length }}
          </span>
        </Transition>
        <span class="summary-divider">·</span>
        <Transition :name="sizeDirection" mode="out-in">
          <span :key="bytesToSize(selectedSize)" class="summary-value">{{ bytesToSize(selectedSize) }}</span>
        </Transition>
        <NButton v-if="files.length > 1" size="tiny" quaternary @click="invertSelection">
          {{ t('task.invert-selection') }}
        </NButton>
      </div>
    </div>
    <NDataTable
      :key="treeIdentity"
      :columns="columns"
      :data="tree"
      :row-key="(row: BtFileTreeRow) => row.key"
      :row-class-name="(row: BtFileTreeRow) => (row.children ? 'bt-folder-row' : '')"
      :default-expanded-row-keys="expandedKeys"
      :indent="hasFolders ? 16 : 0"
      :checked-row-keys="selectedIndices"
      :max-height="fill ? undefined : maxHeight"
      :flex-height="fill"
      :virtual-scroll="files.length > 100"
      :min-row-height="34"
      :bordered="false"
      size="small"
      class="bt-file-table"
      @update:checked-row-keys="updateSelection"
    />
  </div>
</template>

<style scoped>
.bt-file-selector {
  display: grid;
  gap: 10px;
}

.bt-file-selector--fill {
  height: 100%;
  min-height: 0;
  grid-template-rows: auto minmax(0, 1fr);
}

.bt-file-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  min-height: 28px;
}

.bt-file-table {
  border: 1px solid var(--m3-outline-variant);
  border-radius: var(--radius-md);
  overflow: hidden;
}

.bt-file-selector--fill .bt-file-table {
  height: 100%;
}

.bt-file-selector :deep(.bt-folder-row td) {
  font-weight: 600;
}

.bt-file-selector :deep(.bt-file-name) {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  max-width: 100%;
  vertical-align: middle;
}

.bt-file-selector :deep(.bt-file-icon) {
  flex-shrink: 0;
  color: var(--m3-on-surface-variant);
}

.bt-file-selector :deep(.bt-file-label) {
  overflow: clip visible;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bt-file-selector :deep(.bt-file-size) {
  color: var(--m3-on-surface-variant);
  font-variant-numeric: tabular-nums;
}

.file-summary {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  margin-left: auto;
  color: var(--m3-on-surface-variant);
  font-size: var(--font-size-sm);
  font-variant-numeric: tabular-nums;
}

.summary-value {
  color: var(--m3-on-surface);
  font-weight: 600;
}

.summary-divider {
  color: var(--m3-outline);
}
</style>

<style>
.bt-value-up-enter-active,
.bt-value-up-leave-active,
.bt-value-down-enter-active,
.bt-value-down-leave-active {
  transition:
    opacity 0.15s ease-out,
    transform 0.15s ease-out;
}

.bt-value-up-enter-from,
.bt-value-down-leave-to {
  opacity: 0;
  transform: translateY(4px);
}

.bt-value-up-leave-to,
.bt-value-down-enter-from {
  opacity: 0;
  transform: translateY(-4px);
}

@media (prefers-reduced-motion: reduce) {
  .bt-value-up-enter-active,
  .bt-value-up-leave-active,
  .bt-value-down-enter-active,
  .bt-value-down-leave-active {
    transition: none;
  }
}
</style>
