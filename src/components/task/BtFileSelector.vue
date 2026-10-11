<script setup lang="ts">
/** @fileoverview Shared BitTorrent file selector for local torrents and magnets: a folder tree with type shortcuts. */
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { NButton, NDataTable } from 'naive-ui'
import type { DataTableColumns, DataTableRowKey } from 'naive-ui'
import { bytesToSize } from '@shared/utils'
import { calcColumnWidth } from '@shared/utils/calcColumnWidth'
import {
  buildBtFileTree,
  groupBtFilesByType,
  invertBtFileSelection,
  toggleBtFileGroup,
  type BtFileTreeRow,
} from '@shared/utils/btFileTree'
import type { BtFileSelectionItem } from '@shared/types'

const props = withDefaults(
  defineProps<{
    files: BtFileSelectionItem[]
    selectedIndices: number[]
    maxHeight?: number
  }>(),
  { maxHeight: 240 },
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

function groupSelected(indices: number[]): boolean {
  return indices.every((index) => selectedSet.value.has(index))
}

const columns = computed<DataTableColumns<BtFileTreeRow>>(() => [
  { type: 'selection' },
  {
    title: t('task.file-name') || 'File Name',
    key: 'name',
    ellipsis: { tooltip: true },
  },
  {
    title: t('task.file-index') || '#',
    key: 'index',
    width: calcColumnWidth({
      title: t('task.file-index') || '#',
      values: props.files.map((file) => String(file.index)),
    }),
    render: (row) => row.index ?? '',
  },
  {
    title: t('task.file-size') || 'Size',
    key: 'length',
    width: calcColumnWidth({
      title: t('task.file-size') || 'Size',
      values: props.files.map((file) => bytesToSize(file.length)),
      sortable: true,
    }),
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
  <div class="bt-file-selector">
    <div v-if="files.length > 1" class="bt-file-toolbar">
      <NButton
        v-for="group in typeGroups"
        :key="group.type"
        size="tiny"
        round
        secondary
        :type="groupSelected(group.indices) ? 'primary' : 'default'"
        @click="toggleGroup(group.indices)"
      >
        {{ t(`preferences.${group.type}`) }} {{ group.indices.length }}
      </NButton>
      <NButton size="tiny" round quaternary class="bt-file-invert" @click="invertSelection">
        {{ t('task.invert-selection') }}
      </NButton>
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
      :max-height="maxHeight"
      :virtual-scroll="files.length > 100"
      :min-row-height="28"
      size="small"
      @update:checked-row-keys="updateSelection"
    />
    <div class="file-summary" aria-live="polite">
      <Transition :name="countDirection" mode="out-in">
        <span :key="selectedIndices.length" class="summary-value">
          {{ selectedIndices.length }}/{{ files.length }}
        </span>
      </Transition>
      <span class="summary-divider">—</span>
      <Transition :name="sizeDirection" mode="out-in">
        <span :key="bytesToSize(selectedSize)" class="summary-value">{{ bytesToSize(selectedSize) }}</span>
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.bt-file-selector {
  display: grid;
  gap: 10px;
}

.bt-file-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.bt-file-invert {
  margin-left: auto;
}

.bt-file-selector :deep(.bt-folder-row td) {
  font-weight: 600;
}

.file-summary {
  display: inline-flex;
  justify-self: end;
  align-items: baseline;
  gap: 6px;
  min-height: 20px;
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
