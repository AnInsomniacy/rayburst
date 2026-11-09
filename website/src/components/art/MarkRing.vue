<script setup lang="ts">
/**
 * @fileoverview The mark in front of a ring of light, as on the README banners
 * and social previews. The mark goes in the slot and fills the box; the ring
 * is drawn around it, outside the layout. It is centred on the mark and holds
 * all of it, dark inside like an eclipse (light inside on the light theme), so
 * the mark stays clean. Its rim and corona burn hottest by the mark's point,
 * where a flare sits, and two faint echoes sit outside.
 *
 * Units: the box is 100 wide. The mark's point is at (11.742, 91.406); the
 * ring's centre sits between the point and the far top point, the mark's
 * widest span, with its radius 0.59 of the box.
 */
import { useElementSize } from '@vueuse/core'
import { computed, onBeforeUnmount, ref, useId } from 'vue'
import { EASE, gsap } from '@/motion/gsap'

withDefaults(defineProps<{ flare?: boolean }>(), { flare: true })

const CX = 48.522
const CY = 49.996
const R = 59
const REACH = R + 55
const TIP = [11.742, 91.406] as const
const K = 100 / 360
// The rim starts drawing at the point.
const TIP_TURN = (Math.atan2(TIP[1] - CY, TIP[0] - CX) * 180) / Math.PI

const id = useId()
const root = ref<HTMLElement | null>(null)
const back = ref<SVGSVGElement | null>(null)
const { width } = useElementSize(root)
// Lines keep a legible width at small sizes.
const line = (units: number, minPx: number) => Math.max(units, (minPx * 100) / (width.value || 100))
const strokes = computed(() => ({
  rim: line(1.6 * K, 1),
  glow: line(6 * K, 2.5),
  echo: line(1 * K, 0.75),
  echo2: line(0.8 * K, 0.6),
}))
const stop = (d: number) => (d / REACH).toFixed(4)

let tl: gsap.core.Timeline | null = null
/** Hide the ring and return its intro, paused: the rim draws out both ways from the point, then the corona lights. */
function intro(): gsap.core.Timeline | null {
  const el = back.value
  if (!el) return null
  const rims = el.querySelectorAll('.mr-rim')
  const halo = el.querySelectorAll('.mr-halo')
  gsap.set(rims, { drawSVG: '50% 50%' })
  gsap.set(halo, { autoAlpha: 0, scale: 0.92, transformOrigin: `${CX}px ${CY}px` })
  tl = gsap.timeline({ paused: true })
  tl.to(rims, { drawSVG: '0% 100%', duration: 1.3, ease: EASE.enter, stagger: 0.12 }, 0).to(
    halo,
    { autoAlpha: 1, scale: 1, duration: 1.4, ease: EASE.text, stagger: 0.1 },
    0.35,
  )
  return tl
}

onBeforeUnmount(() => tl?.kill())
defineExpose({ intro })
</script>

<template>
  <div ref="root" class="mark-ring">
    <svg ref="back" class="mr-back" viewBox="-70 -70 240 240" aria-hidden="true">
      <defs>
        <radialGradient :id="`${id}-bloom`" gradientUnits="userSpaceOnUse" :cx="CX" :cy="CY" :r="REACH * 1.5">
          <stop offset="0" class="mr-bloom" />
          <stop offset="0.4" class="mr-bloom" style="stop-opacity: calc(var(--mr-bloom-a) * 0.45)" />
          <stop offset="1" class="mr-bloom" style="stop-opacity: 0" />
        </radialGradient>
        <radialGradient :id="`${id}-corona`" gradientUnits="userSpaceOnUse" :cx="CX" :cy="CY" :r="REACH">
          <stop :offset="stop(R - 1.5 * K)" class="mr-c0" />
          <stop :offset="stop(R)" class="mr-c1" />
          <stop :offset="stop(R + 3 * K)" class="mr-c2" />
          <stop :offset="stop(R + 16 * K)" class="mr-c3" />
          <stop :offset="stop(R + 48 * K)" class="mr-c4" />
          <stop offset="1" class="mr-c5" />
        </radialGradient>
        <radialGradient :id="`${id}-eclipse`" gradientUnits="userSpaceOnUse" :cx="CX" :cy="CY" :r="R">
          <stop offset="0" class="mr-e0" />
          <stop offset="0.85" class="mr-e1" />
          <stop offset="1" class="mr-e2" />
        </radialGradient>
        <linearGradient
          :id="`${id}-heat`"
          gradientUnits="userSpaceOnUse"
          :x1="TIP[0]"
          :y1="TIP[1]"
          :x2="CX + R * 0.8"
          :y2="CY - R * 0.8"
        >
          <stop offset="0" stop-color="#fff" />
          <stop offset="0.55" stop-color="#fff" stop-opacity="0.5" />
          <stop offset="1" stop-color="#fff" stop-opacity="0.14" />
        </linearGradient>
        <mask :id="`${id}-mask`" maskUnits="userSpaceOnUse" x="-70" y="-70" width="240" height="240">
          <rect x="-70" y="-70" width="240" height="240" :fill="`url(#${id}-heat)`" />
        </mask>
        <filter :id="`${id}-glow`" x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur :stdDeviation="4 * K" />
        </filter>
      </defs>
      <circle class="mr-halo" :cx="CX" :cy="CY" :r="REACH * 1.5" :fill="`url(#${id}-bloom)`" />
      <circle :cx="CX" :cy="CY" :r="R" :fill="`url(#${id}-eclipse)`" />
      <g :mask="`url(#${id}-mask)`">
        <circle class="mr-halo" :cx="CX" :cy="CY" :r="REACH" :fill="`url(#${id}-corona)`" />
        <g fill="none" :transform="`rotate(${TIP_TURN} ${CX} ${CY})`">
          <circle
            class="mr-rim mr-glow"
            :cx="CX"
            :cy="CY"
            :r="R"
            :stroke-width="strokes.glow"
            :filter="`url(#${id}-glow)`"
          />
          <circle class="mr-rim mr-line" :cx="CX" :cy="CY" :r="R" :stroke-width="strokes.rim" />
          <circle class="mr-rim mr-echo" :cx="CX" :cy="CY" :r="R * 1.09" :stroke-width="strokes.echo" />
          <circle class="mr-rim mr-echo2" :cx="CX" :cy="CY" :r="R * 1.18" :stroke-width="strokes.echo2" />
        </g>
      </g>
    </svg>
    <div class="mr-mark"><slot /></div>
    <svg v-if="flare" class="mr-front" viewBox="-70 -70 240 240" aria-hidden="true">
      <defs>
        <radialGradient v-for="(f, i) in ['a', 'b', 'c']" :id="`${id}-flare-${f}`" :key="f">
          <stop offset="0" :class="`mr-f${i}`" />
          <stop offset="0.4" :class="`mr-f${i}`" style="stop-opacity: calc(var(--a) * 0.45)" />
          <stop offset="1" :class="`mr-f${i}`" style="stop-opacity: 0" />
        </radialGradient>
      </defs>
      <circle class="mr-halo" :cx="TIP[0]" :cy="TIP[1]" :r="110 * K" :fill="`url(#${id}-flare-a)`" />
      <circle class="mr-halo" :cx="TIP[0]" :cy="TIP[1]" :r="46 * K" :fill="`url(#${id}-flare-b)`" />
      <circle class="mr-halo" :cx="TIP[0]" :cy="TIP[1]" :r="16 * K" :fill="`url(#${id}-flare-c)`" />
    </svg>
  </div>
</template>

<style scoped>
.mark-ring {
  position: relative;
  aspect-ratio: 1;
  isolation: isolate;
  /* The dark stage. */
  --mr-bloom: #5b21b6;
  --mr-bloom-a: 0.4;
  --mr-e0: rgba(5, 3, 11, 0.7);
  --mr-e1: rgba(11, 6, 24, 0.5);
  --mr-e2: rgba(42, 17, 80, 0.6);
  --mr-c0: rgba(245, 208, 254, 0);
  --mr-c1: rgba(255, 255, 255, 0.95);
  --mr-c2: rgba(245, 208, 254, 0.7);
  --mr-c3: rgba(232, 121, 249, 0.34);
  --mr-c4: rgba(168, 85, 247, 0.14);
  --mr-c5: rgba(124, 58, 237, 0);
  --mr-glow: rgba(240, 171, 252, 0.55);
  --mr-line: rgba(255, 255, 255, 0.95);
  --mr-echo: rgba(240, 171, 252, 0.45);
  --mr-echo2: rgba(196, 181, 253, 0.22);
  --mr-f0: #e879f9;
  --mr-f0-a: 0.45;
  --mr-f1: #f5d0fe;
  --mr-f1-a: 0.85;
  --mr-f2: #fff;
  --mr-f2-a: 1;
}
/* The light field: a bright disc, violet rim. */
:root[data-theme='light'] .mark-ring {
  --mr-bloom: #c4b5fd;
  --mr-bloom-a: 0.45;
  --mr-e0: rgba(255, 255, 255, 0.8);
  --mr-e1: rgba(250, 245, 255, 0.65);
  --mr-e2: rgba(233, 213, 255, 0.75);
  --mr-c0: rgba(243, 232, 255, 0);
  --mr-c1: rgba(123, 62, 209, 0.95);
  --mr-c2: rgba(168, 85, 247, 0.55);
  --mr-c3: rgba(217, 70, 239, 0.22);
  --mr-c4: rgba(168, 85, 247, 0.1);
  --mr-c5: rgba(168, 85, 247, 0);
  --mr-glow: rgba(192, 132, 252, 0.6);
  --mr-line: rgba(123, 62, 209, 0.95);
  --mr-echo: rgba(168, 85, 247, 0.4);
  --mr-echo2: rgba(139, 85, 230, 0.24);
  --mr-f0: #e879f9;
  --mr-f0-a: 0.3;
  --mr-f1: #f5d0fe;
  --mr-f1-a: 0.7;
  --mr-f2: #fff;
  --mr-f2-a: 0.95;
}
.mr-back,
.mr-front {
  position: absolute;
  inset: -70%;
  width: 240%;
  height: 240%;
  overflow: visible;
  pointer-events: none;
}
.mr-back {
  z-index: -1;
}
.mr-mark {
  position: relative;
  width: 100%;
  height: 100%;
}
.mr-mark > :deep(*) {
  width: 100%;
  height: 100%;
}
.mr-bloom {
  stop-color: var(--mr-bloom);
  stop-opacity: var(--mr-bloom-a);
}
.mr-e0 {
  stop-color: var(--mr-e0);
}
.mr-e1 {
  stop-color: var(--mr-e1);
}
.mr-e2 {
  stop-color: var(--mr-e2);
}
.mr-c0 {
  stop-color: var(--mr-c0);
}
.mr-c1 {
  stop-color: var(--mr-c1);
}
.mr-c2 {
  stop-color: var(--mr-c2);
}
.mr-c3 {
  stop-color: var(--mr-c3);
}
.mr-c4 {
  stop-color: var(--mr-c4);
}
.mr-c5 {
  stop-color: var(--mr-c5);
}
.mr-glow {
  stroke: var(--mr-glow);
}
.mr-line {
  stroke: var(--mr-line);
}
.mr-echo {
  stroke: var(--mr-echo);
}
.mr-echo2 {
  stroke: var(--mr-echo2);
}
.mr-f0 {
  --a: var(--mr-f0-a);
  stop-color: var(--mr-f0);
  stop-opacity: var(--a);
}
.mr-f1 {
  --a: var(--mr-f1-a);
  stop-color: var(--mr-f1);
  stop-opacity: var(--a);
}
.mr-f2 {
  --a: var(--mr-f2-a);
  stop-color: var(--mr-f2);
  stop-opacity: var(--a);
}
</style>
