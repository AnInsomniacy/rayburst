<script setup lang="ts">
import { NCheckbox, NCheckboxGroup } from 'naive-ui'

defineProps<{
  value: string[]
  options: Array<{ label: string; value: string }>
}>()

const emit = defineEmits<{
  'update:value': [value: string[]]
}>()
</script>

<template>
  <NCheckboxGroup :value="value" @update:value="(next) => emit('update:value', next as string[])">
    <div class="preference-checkbox-grid">
      <NCheckbox v-for="option in options" :key="option.value" :value="option.value">
        {{ option.label }}
      </NCheckbox>
    </div>
  </NCheckboxGroup>
</template>

<style scoped>
.preference-checkbox-grid {
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-width: 540px;
  padding: 3px 0;
}

.preference-checkbox-grid :deep(.n-checkbox) {
  min-height: 28px;
  align-items: center;
}

.preference-checkbox-grid :deep(.n-checkbox__label) {
  color: var(--m3-on-surface);
  font-size: var(--font-size-sm);
  line-height: 1.35;
}
</style>
