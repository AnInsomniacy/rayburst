<script setup lang="ts">
/**
 * @fileoverview The hero's moving layer, after the social preview: a halo of
 * light breathing around the ring, star dust drifting up and to the right, and
 * an aria that leaves the ring, runs under the copy and opens into the far
 * side, its twist rolling along it. Drawn on a canvas from the scene clock.
 */
import { useElementBounding } from '@vueuse/core'
import { ref, watch } from 'vue'
import { drawAria, drawBloom, drawHalo, paletteFor } from '@/art/motifs'
import { useTheme } from '@/composables/useTheme'
import { useCanvas } from '@/motion/useCanvas'
import { EASE, hash, prog } from '@/motion/gsap'
import { isRtlPage, rectIn } from '@/ui/geometry'

const props = defineProps<{ time: number; mark: HTMLElement | null; copy: HTMLElement | null }>()

const STARS = 90
const canvas = ref<HTMLCanvasElement | null>(null)
const { frame } = useCanvas(canvas, 1.75)
const theme = useTheme()
const markBox = useElementBounding(() => props.mark, { windowScroll: false })
const copyBox = useElementBounding(() => props.copy, { windowScroll: false })
const canvasBox = useElementBounding(canvas, { windowScroll: false })

function draw(time: number) {
  const f = frame()
  if (!f) return
  const { ctx, w, h } = f
  const lite = theme.state.value === 'light'
  const paint = paletteFor(theme.state.value)
  // The ring (see MarkRing): centred on the mark's box, radius 0.59 of it.
  const size = markBox.width.value || 200
  const cx = markBox.left.value - canvasBox.left.value + size * 0.485
  const cy = markBox.top.value - canvasBox.top.value + size * 0.5
  const r = size * 0.59
  const stacked = copyBox.top.value > markBox.bottom.value - 1

  drawHalo(ctx, { cx, cy, r: r * 2.4, time, palette: paint, alpha: EASE.text(prog(time, 0.6, 1.8)) })

  for (let i = 0; i < STARS; i++) {
    const depth = 0.3 + hash(i, 1) * 0.7
    const x = ((hash(i, 2) * (w + 80) + time * 6 * depth) % (w + 80)) - 40
    const y = ((hash(i, 3) * (h + 80) - time * 10 * depth + (h + 80) * 4) % (h + 80)) - 40
    if (Math.hypot(x - cx, y - cy) < r * 1.1) continue
    const tw = 0.5 + 0.5 * Math.sin(time * (0.8 + hash(i, 4) * 1.6) + i)
    const a = (0.1 + 0.45 * tw) * depth * (lite ? 0.5 : 1)
    ctx.fillStyle = lite ? `rgba(123,62,209,${a})` : `rgba(239,230,255,${a})`
    ctx.beginPath()
    ctx.arc(x, y, 0.5 + depth * 1.2, 0, Math.PI * 2)
    ctx.fill()
  }

  // The aria: under the copy, flat from the ring, open into the far side.
  const rtl = isRtlPage()
  const copyBottom = copyBox.bottom.value - canvasBox.top.value
  const y = Math.min(h - 60, Math.max(cy + size * 0.5, copyBottom + (stacked ? 40 : 64)))
  const near = rtl ? w - cx : cx
  const amp = Math.min(size * 0.43, h * 0.2)
  const drift = Math.min(size * 0.3, y * 0.35)
  const intro = EASE.text(prog(time, 0.9, 1.6))
  // It opens only past the last line of the copy, so it never runs under the statistics.
  let far = 0
  const foot = props.copy?.lastElementChild
  if (foot && canvas.value) {
    for (const el of foot.children) {
      const r = rectIn(el, canvas.value)
      far = Math.max(far, rtl ? w - r.x : r.x + r.w)
    }
  }
  const swell = stacked ? w * 0.4 : Math.min(w * 0.8, Math.max(near + size, w * 0.62, far - 100))
  if (!lite) drawBloom(ctx, rtl ? w * 0.08 : w * 0.92, y - drift * 0.6, amp * 2.4, '#c026d3', 0.26 * intro)
  drawAria(ctx, {
    w,
    x0: stacked ? 0 : near,
    end: w + 30,
    y,
    swell,
    amp: amp * (0.6 + 0.4 * intro),
    drift,
    twist: 0.0098,
    phase: time * 0.22,
    strands: 72,
    alpha: intro,
    flip: rtl,
    paint: paint.aria,
  })
}

watch(
  () => [props.time, theme.state.value, markBox.top.value, copyBox.bottom.value, canvasBox.width.value] as const,
  ([time]) => draw(time),
  { flush: 'post' },
)
</script>

<template>
  <canvas ref="canvas" class="hero-fx" aria-hidden="true" />
</template>

<style scoped>
.hero-fx {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  z-index: -1;
  pointer-events: none;
}
</style>
