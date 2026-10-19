<script setup lang="ts">
/**
 * @fileoverview The downloaded pieces of a task as one bar, left to right: each
 * column is shaded by the share of its pieces that are done.
 */
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useAppColorTokens } from '@/composables/useColorScheme'

const props = defineProps<{ bitfield: string; pieces: number }>()

const canvas = ref<HTMLCanvasElement>()
const colorTokens = useAppColorTokens()
let observer: ResizeObserver | null = null
let frame: number | null = null
let dirty = true

function done(index: number): boolean {
  const nibble = parseInt(props.bitfield[index >> 2] ?? '0', 16)
  return (nibble & (8 >> (index & 3))) !== 0
}

function draw() {
  frame = null
  const element = canvas.value
  if (!element) return
  const ratio = window.devicePixelRatio || 1
  const width = Math.max(1, Math.round(element.clientWidth * ratio))
  const height = Math.max(1, Math.round(element.clientHeight * ratio))
  if (!dirty && element.width === width && element.height === height) return
  const context = element.getContext('2d')
  if (!context) return
  if (element.width !== width) element.width = width
  if (element.height !== height) element.height = height
  dirty = false
  context.clearRect(0, 0, width, height)
  const pieces = props.pieces || props.bitfield.length * 4
  if (pieces <= 0) return
  context.fillStyle = colorTokens.value.statusActive
  for (let x = 0; x < width; x++) {
    const first = Math.floor((x * pieces) / width)
    const last = Math.max(first + 1, Math.floor(((x + 1) * pieces) / width))
    let count = 0
    for (let index = first; index < last; index++) if (done(index)) count++
    if (count === 0) continue
    context.globalAlpha = count / (last - first)
    context.fillRect(x, 0, 1, height)
  }
  context.globalAlpha = 1
}

function scheduleDraw() {
  if (frame === null) frame = requestAnimationFrame(draw)
}

watch(
  () => [props.bitfield, props.pieces, colorTokens.value.statusActive],
  () => {
    dirty = true
    scheduleDraw()
  },
)
onMounted(() => {
  scheduleDraw()
  if (canvas.value) {
    observer = new ResizeObserver(scheduleDraw)
    observer.observe(canvas.value)
  }
})
onBeforeUnmount(() => {
  observer?.disconnect()
  if (frame !== null) cancelAnimationFrame(frame)
})
</script>

<template>
  <canvas ref="canvas" class="segment-bar" aria-hidden="true" />
</template>

<style scoped>
.segment-bar {
  display: block;
  width: 100%;
  height: 6px;
  border-radius: var(--radius-full);
  background: var(--m3-surface-container-highest);
}
</style>
