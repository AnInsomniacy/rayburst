<script setup lang="ts">
/**
 * @fileoverview A section's backdrop in the artwork's language, as behind the
 * feature posters: a dot grid, a halo of light breathing behind the heading
 * (or around a ring) and an aria opening to one side, in the dark stage's
 * colours or the light field's. The halo drifts and the aria's twist rolls
 * while the section is on screen; the edges fade into the page.
 */
import { ref, watch } from 'vue'
import { drawAria, drawHalo, paletteFor } from '@/art/motifs'
import { useTheme } from '@/composables/useTheme'
import { useCanvas } from '@/motion/useCanvas'
import { useScene } from '@/motion/useScene'
import { isRtlPage, rectIn } from '@/ui/geometry'

export interface FieldAria {
  /** Its line, in pixels from the top: in the gap above the heading. */
  y: number
  /** The side it opens into; the other end starts flat. */
  open: 'start' | 'end'
  /** Where it starts to open, as a fraction of the width from the flat end. */
  swell?: number
  amp?: number
  drift?: number
}

const props = withDefaults(
  defineProps<{
    /** Where the halo sits: a fraction of the width from the start side, and pixels from the top. */
    at?: readonly [number, number]
    /** Or from a ring around this element (see MarkRing). */
    ring?: HTMLElement | null
    aria?: FieldAria | null
    grid?: boolean
  }>(),
  { at: () => [0.5, 215] as const, ring: null, aria: null, grid: true },
)

const root = ref<HTMLElement | null>(null)
const canvas = ref<HTMLCanvasElement | null>(null)
const { frame, width, height } = useCanvas(canvas, 1.25)
const { time } = useScene(root, { still: 4 })
const theme = useTheme()

function draw(t: number) {
  const f = frame()
  if (!f) return
  const { ctx, w } = f
  const lite = theme.state.value === 'light'
  const paint = paletteFor(theme.state.value)
  const rtl = isRtlPage()
  // Measured each frame: the ring may still be rising into place.
  const ring = props.ring && root.value ? rectIn(props.ring, root.value) : null
  const size = ring?.w ?? 0
  const [cx, cy] = ring
    ? [ring.x + size * 0.485, ring.y + size * 0.5]
    : [(rtl ? 1 - props.at[0] : props.at[0]) * w, props.at[1]]
  drawHalo(ctx, {
    cx,
    cy,
    r: size ? size * 2.6 : Math.min(w * 0.42, 560),
    time: t,
    palette: paint,
    alpha: lite ? 0.8 : 0.75,
  })
  const a = props.aria
  if (!a) return
  const swell = a.swell ?? 0.5
  drawAria(ctx, {
    w,
    x0: 0,
    end: w + 30,
    y: a.y,
    swell: w * swell,
    amp: a.amp ?? 120,
    drift: a.drift ?? -40,
    twist: 0.0085,
    phase: t * 0.18,
    strands: 56,
    alpha: lite ? 0.85 : 0.7,
    flip: (a.open === 'start') !== rtl,
    paint: paint.aria,
  })
}

watch(
  () => [time.value, theme.state.value, width.value, height.value] as const,
  ([t]) => draw(t),
  { flush: 'post' },
)
</script>

<template>
  <div ref="root" class="field" aria-hidden="true">
    <div v-if="grid" class="field-grid" />
    <canvas ref="canvas" class="field-fx" />
  </div>
</template>

<style scoped>
.field {
  position: absolute;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  -webkit-mask-image: linear-gradient(180deg, transparent, #000 80px, #000 calc(100% - 160px), transparent);
  mask-image: linear-gradient(180deg, transparent, #000 80px, #000 calc(100% - 160px), transparent);
}
.field-fx,
.field-grid {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
}
.field-grid {
  background-image: radial-gradient(circle, rgba(201, 184, 255, 0.5) 1px, transparent 1.4px);
  background-size: 26px 26px;
  -webkit-mask-image: radial-gradient(55% 45% at 50% 30%, rgba(0, 0, 0, 0.55), transparent);
  mask-image: radial-gradient(55% 45% at 50% 30%, rgba(0, 0, 0, 0.55), transparent);
}
:root[data-theme='light'] .field-grid {
  background-image: radial-gradient(circle, rgba(124, 58, 237, 0.42) 0.9px, transparent 1.3px);
  background-size: 22px 22px;
}
</style>
