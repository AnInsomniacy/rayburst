<script setup lang="ts">
/**
 * @fileoverview Aria2 Next's paper, after its README banner: the icon set in a
 * gold guilloché rosette that turns very
 * slowly in a warm pool of light, and one fine gold
 * line that leaves it, runs under the name marked in segments as the engine
 * splits a file, then swells into a full aria whose twist rolls along it.
 */
import { ref, watch } from 'vue'
import { drawAria, drawHalo, GOLD } from '@/art/motifs'
import { EASE, prog } from '@/motion/gsap'
import { useCanvas } from '@/motion/useCanvas'
import { isRtlPage, rectIn } from '@/ui/geometry'

const props = defineProps<{
  time: number
  section: HTMLElement | null
  icon: HTMLElement | null
  name: HTMLElement | null
}>()

const canvas = ref<HTMLCanvasElement | null>(null)
const { frame, width, height } = useCanvas(canvas, 1.5)

function draw(time: number) {
  const f = frame()
  if (!f) return
  const { ctx, w, h } = f

  // Measured each frame: the icon and the name may still be rising into place.
  const { section, icon, name } = props
  if (!section || !icon || !name) return
  const ib = rectIn(icon, section)
  const nb = rectIn(name, section)
  const is = ib.w
  const ix = ib.x + is / 2
  const iy = ib.y + ib.h / 2
  const r0 = is * 0.6
  const r1 = is * 0.743
  const intro = EASE.text(prog(time, 0.2, 1.6))

  drawHalo(ctx, { cx: ix, cy: iy, r: is * 2.8, time, palette: GOLD, alpha: intro })

  // The rosette: rippled circles, each turned a little from the last, weave a band.
  ctx.save()
  ctx.strokeStyle = '#b07d26'
  ctx.lineWidth = 0.6
  ctx.globalAlpha = 0.6 * intro
  const turn = time * 0.02
  for (let i = 0; i < 8; i++) {
    ctx.beginPath()
    for (let j = 0; j <= 720; j++) {
      const t = (j / 720) * Math.PI * 2
      const r = (r0 + r1) / 2 + ((r1 - r0) / 2) * Math.sin(18 * t + (i * Math.PI) / 4)
      const a = t + turn
      if (j === 0) ctx.moveTo(ix + r * Math.cos(a), iy + r * Math.sin(a))
      else ctx.lineTo(ix + r * Math.cos(a), iy + r * Math.sin(a))
    }
    ctx.stroke()
  }
  ctx.strokeStyle = '#b88a35'
  for (const [r, a, lw] of [
    [r1 + 7, 0.6, 0.8],
    [r1 + 11, 0.4, 0.5],
    [r0 - 4, 0.45, 0.5],
  ] as const) {
    ctx.globalAlpha = a * intro
    ctx.lineWidth = lw
    ctx.beginPath()
    ctx.arc(ix, iy, r, 0, Math.PI * 2)
    ctx.stroke()
  }
  ctx.restore()

  // The line: flat under the name, then the aria; mirrored on right-to-left pages.
  const rtl = isRtlPage()
  const nameEnd = rtl ? w - nb.x : nb.x + nb.w
  const near = rtl ? w - ix : ix
  const swell = Math.max(nameEnd + 40, w * 0.5)
  ctx.save()
  if (rtl) {
    ctx.translate(w, 0)
    ctx.scale(-1, 1)
  }
  ctx.strokeStyle = '#a87526'
  ctx.lineWidth = 0.8
  ctx.globalAlpha = 0.7 * intro
  ctx.beginPath()
  const reach = near + r1 + 18 + (swell - 30 - near - r1 - 18) * intro
  for (let x = near + r1 + 18, n = 0; x < reach; x += 8, n++) {
    ctx.moveTo(x, iy + 3)
    ctx.lineTo(x, iy + 3 + (n % 5 ? 3 : 7))
  }
  ctx.stroke()
  ctx.restore()
  drawAria(ctx, {
    w,
    x0: near + is * 0.4,
    end: w + 30,
    y: iy,
    swell,
    amp: Math.min(150, h * 0.16) * (0.5 + 0.5 * intro),
    drift: 36,
    twist: 0.0105,
    phase: time * 0.2,
    strands: 72,
    stroke: 0.75,
    alpha: 0.7 * intro,
    flip: rtl,
    paint: GOLD.aria,
  })
}

watch(
  () => [props.time, width.value, height.value] as const,
  ([time]) => draw(time),
  { flush: 'post' },
)
</script>

<template>
  <canvas ref="canvas" class="engine-fx" aria-hidden="true" />
</template>

<style scoped>
.engine-fx {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  z-index: -1;
  pointer-events: none;
  -webkit-mask-image: var(--paper-fade);
  mask-image: var(--paper-fade);
}
</style>
