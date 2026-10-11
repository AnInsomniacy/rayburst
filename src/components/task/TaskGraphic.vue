<script setup lang="ts">
/** @fileoverview Visual bitfield progress graphic for download pieces. */
import { computed, ref, onMounted, onBeforeUnmount, watch, nextTick } from 'vue'
import { useAppColorTokens } from '@/composables/useColorScheme'

const props = withDefaults(
  defineProps<{
    bitfield?: string
    atomWidth?: number
    atomHeight?: number
    atomGutter?: number
    atomRadius?: number
  }>(),
  {
    bitfield: '',
    atomWidth: 8,
    atomHeight: 8,
    atomGutter: 2,
    atomRadius: 1.5,
  },
)

const container = ref<HTMLElement>()
const canvas = ref<HTMLCanvasElement>()
const containerWidth = ref(300)
const colorTokens = useAppColorTokens()

function updateWidth() {
  if (container.value) containerWidth.value = container.value.clientWidth
}

let ro: ResizeObserver | null = null
onMounted(() => {
  updateWidth()
  if (container.value) {
    ro = new ResizeObserver(() => {
      updateWidth()
      nextTick(draw)
    })
    ro.observe(container.value)
  }
})
onBeforeUnmount(() => {
  ro?.disconnect()
})

const len = computed(() => props.bitfield.length)
const atomWG = computed(() => props.atomWidth + props.atomGutter)
const atomHG = computed(() => props.atomHeight + props.atomGutter)

const columnCount = computed(() => {
  const cols = Math.floor((containerWidth.value - props.atomWidth) / atomWG.value) + 1
  return Math.max(cols, 1)
})

const rowCount = computed(() => Math.ceil(len.value / columnCount.value))

const canvasWidth = computed(() => atomWG.value * (columnCount.value - 1) + props.atomWidth)
const canvasHeight = computed(() => atomHG.value * (rowCount.value - 1) + props.atomHeight)

/** Status gradient: inactive → fully active (5 levels), derived from M3 tokens. */
function getStatusColors(): string[] {
  const surface = colorTokens.value.surfaceContainer
  const success = colorTokens.value.statusSuccess
  return [
    surface,
    mixHex(surface, success, 0.25),
    mixHex(surface, success, 0.5),
    mixHex(surface, success, 0.75),
    success,
  ]
}

/** Simple hex color lerp for Canvas 2D (which cannot parse CSS color-mix). */
function mixHex(a: string, b: string, t: number): string {
  const pa = [parseInt(a.slice(1, 3), 16), parseInt(a.slice(3, 5), 16), parseInt(a.slice(5, 7), 16)]
  const pb = [parseInt(b.slice(1, 3), 16), parseInt(b.slice(3, 5), 16), parseInt(b.slice(5, 7), 16)]
  const r = Math.round(pa[0] + (pb[0] - pa[0]) * t)
  const g = Math.round(pa[1] + (pb[1] - pa[1]) * t)
  const bl = Math.round(pa[2] + (pb[2] - pa[2]) * t)
  return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${bl.toString(16).padStart(2, '0')}`
}
const strokeColor = computed(() => colorTokens.value.outlineVariant)

// Track previous status for fade-in animation
const prevStatus = ref<number[]>([])

function draw() {
  const cvs = canvas.value
  if (!cvs || !props.bitfield) return

  const dpr = window.devicePixelRatio || 1
  const w = canvasWidth.value
  const h = canvasHeight.value

  cvs.width = w * dpr
  cvs.height = h * dpr
  cvs.style.width = w + 'px'
  cvs.style.height = h + 'px'

  const ctx = cvs.getContext('2d')
  if (!ctx) return
  ctx.scale(dpr, dpr)
  ctx.clearRect(0, 0, w, h)

  const cols = columnCount.value
  const aw = props.atomWidth
  const ah = props.atomHeight
  const awg = atomWG.value
  const ahg = atomHG.value
  const r = props.atomRadius
  const bf = props.bitfield
  const n = bf.length

  const newStatus: number[] = new Array(n)

  for (let i = 0; i < n; i++) {
    const col = i % cols
    const row = Math.floor(i / cols)
    const x = col * awg
    const y = row * ahg
    const status = Math.floor(parseInt(bf[i], 16) / 4)
    newStatus[i] = status

    const wasInactive = prevStatus.value.length > 0 && (prevStatus.value[i] || 0) === 0
    const justActivated = wasInactive && status > 0

    const colors = getStatusColors()
    ctx.fillStyle = colors[status] || colors[0]
    ctx.globalAlpha = status > 0 ? 1.0 : 0.5

    // Draw rounded rect
    ctx.beginPath()
    ctx.moveTo(x + r, y)
    ctx.lineTo(x + aw - r, y)
    ctx.arcTo(x + aw, y, x + aw, y + r, r)
    ctx.lineTo(x + aw, y + ah - r)
    ctx.arcTo(x + aw, y + ah, x + aw - r, y + ah, r)
    ctx.lineTo(x + r, y + ah)
    ctx.arcTo(x, y + ah, x, y + ah - r, r)
    ctx.lineTo(x, y + r)
    ctx.arcTo(x, y, x + r, y, r)
    ctx.closePath()
    ctx.fill()

    // Subtle stroke
    ctx.strokeStyle = strokeColor.value
    ctx.lineWidth = 0.5
    ctx.globalAlpha = status > 0 ? 0.6 : 0.3
    ctx.stroke()

    // Glow for newly activated pieces
    if (justActivated) {
      ctx.globalAlpha = 0.3
      ctx.fillStyle = colorTokens.value.statusSuccess
      ctx.fill()
    }
  }

  ctx.globalAlpha = 1.0
  prevStatus.value = newStatus
}

watch(
  () => props.bitfield,
  () => nextTick(draw),
)
watch([canvasWidth, canvasHeight], () => nextTick(draw))
watch(colorTokens, () => nextTick(draw))
onMounted(() => nextTick(draw))
</script>

<template>
  <div ref="container" class="task-graphic-container">
    <canvas v-if="bitfield" ref="canvas" class="task-graphic" />
    <div v-else class="no-bitfield">No piece data</div>
  </div>
</template>

<style scoped>
.task-graphic-container {
  width: 100%;
  padding: 8px 0;
  overflow: hidden;
}
.task-graphic {
  display: block;
}
.no-bitfield {
  color: var(--m3-on-surface-variant);
  font-size: var(--font-size-xs);
  padding: 8px 0;
}
</style>
