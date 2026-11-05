<script setup lang="ts">
/** @fileoverview Determinate progress bar that moves only on the compositor.
 *
 * The fill is scaled, never resized: a width or max-width change re-lays out
 * and repaints the list on every frame of its transition, and an endless
 * sweep keeps doing so for every active download (#799).
 */
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    /** 0–100 */
    percentage: number
    color?: string
    height?: number
  }>(),
  { color: 'var(--m3-primary)', height: 6 },
)

const ratio = computed(() => Math.min(Math.max(props.percentage, 0), 100) / 100)
</script>

<template>
  <div
    class="linear-progress"
    role="progressbar"
    aria-valuemin="0"
    aria-valuemax="100"
    :aria-valuenow="Math.round(ratio * 100)"
    :style="{ height: `${height}px` }"
  >
    <div class="linear-progress__fill" :style="{ transform: `scaleX(${ratio})`, backgroundColor: color }" />
  </div>
</template>

<style scoped>
.linear-progress {
  position: relative;
  overflow: hidden;
  border-radius: 999px;
  background: var(--m3-outline-variant);
  contain: strict;
  width: 100%;
}
.linear-progress__fill {
  position: absolute;
  inset: 0;
  transform-origin: 0 50%;
  transition:
    transform var(--task-motion-progress) var(--task-motion-ease),
    background-color var(--task-motion-state) var(--task-motion-ease);
}
</style>
