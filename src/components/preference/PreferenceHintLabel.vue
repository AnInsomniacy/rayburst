<script setup lang="ts">
defineProps<{
  label: string
  hint: string
}>()
</script>

<template>
  <span class="preference-hint-label">
    <span class="preference-hint-label__title">{{ label }}</span>
    <span class="preference-hint-label__hint">{{ hint }}</span>
  </span>
</template>

<style scoped>
.preference-hint-label {
  display: inline-flex;
  flex-direction: column;
  gap: 4px;
  max-width: 220px;
  padding-bottom: 29.25px;
  line-height: 1.35;
  white-space: normal;
}
.preference-hint-label__title,
.preference-hint-label__hint {
  display: block;
  overflow-wrap: anywhere;
}
.preference-hint-label__hint {
  color: var(--m3-on-surface-variant);
  font-size: var(--font-size-xs);
  font-weight: 400;
}
</style>
