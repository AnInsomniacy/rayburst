<script setup lang="ts">
/** @fileoverview Speed limit field: a number and a KiB/s or MiB/s unit; 0 means unlimited. */
import { computed } from 'vue'
import { speedUnitOptions as speedUnits } from '@shared/utils/format'
import { NInputGroup, NInputNumber, NSelect } from 'naive-ui'
import { parseSpeedLimit, formatSpeedLimit, type SpeedLimitUnit } from '@shared/utils/speedLimit'

const props = defineProps<{ value: string; disabled?: boolean; placeholder?: string }>()
const emit = defineEmits<{ 'update:value': [value: string] }>()

const unitOptions = computed(speedUnits)
const parsed = computed(() => parseSpeedLimit(props.value))

function update(amount: number | null, unit: SpeedLimitUnit) {
  emit('update:value', formatSpeedLimit(amount ?? 0, unit))
}
</script>

<template>
  <NInputGroup>
    <NInputNumber
      :value="parsed.amount || null"
      :min="0"
      :max="1048576"
      :step="1"
      :disabled="disabled"
      :placeholder="placeholder"
      class="speed-limit-amount"
      @update:value="(amount) => update(amount, parsed.unit)"
    />
    <NSelect
      :value="parsed.unit"
      :options="unitOptions"
      :disabled="disabled"
      class="speed-limit-unit"
      @update:value="(unit: SpeedLimitUnit) => update(parsed.amount, unit)"
    />
  </NInputGroup>
</template>

<style scoped>
.speed-limit-amount {
  flex: 1;
  min-width: 0;
}
.speed-limit-unit {
  width: 96px;
  flex-shrink: 0;
}
</style>
