<script setup lang="ts">
/**
 * @fileoverview The master logo, drawn in: each facet's outline traces itself
 * (DrawSVG), the artwork fills in, and a glow blooms and settles.
 */
import { onBeforeUnmount, ref } from 'vue'
import { EASE, gsap } from '@/motion/gsap'

const svg = ref<SVGSVGElement | null>(null)
let tl: gsap.core.Timeline | null = null

/** Hide the artwork and return the intro timeline, paused (the hero places it at 0). */
function intro(): gsap.core.Timeline | null {
  const el = svg.value
  if (!el) return null
  const traces = el.querySelectorAll('.trace')
  const fill = el.querySelector('.fill')
  gsap.set(fill, { autoAlpha: 0 })
  gsap.set(traces, { drawSVG: '0%', autoAlpha: 0 })
  tl = gsap.timeline({ paused: true })
  tl.to(traces, { drawSVG: '100%', autoAlpha: 1, duration: 0.8, stagger: 0.09, ease: EASE.enter }, 0)
    .to(traces, { autoAlpha: 0, duration: 0.35, stagger: 0.09, ease: 'power1.out' }, 0.8)
    .to(fill, { autoAlpha: 1, duration: 0.9, ease: EASE.text }, 0.75)
    .fromTo(
      el,
      { filter: 'drop-shadow(0px 0px 0px rgba(177,124,236,0))' },
      {
        keyframes: [
          { filter: 'drop-shadow(0px 0px 28px rgba(177,124,236,0.9))', duration: 0.6, ease: 'power2.out' },
          { filter: 'drop-shadow(0px 12px 40px rgba(123,62,209,0.35))', duration: 1.8, ease: 'power1.inOut' },
        ],
      },
      0.7,
    )
  return tl
}

onBeforeUnmount(() => tl?.kill())
defineExpose({ intro })
</script>

<template>
  <svg ref="svg" class="hero-logo" viewBox="0 0 512 512" role="img" aria-label="Rayburst">
    <defs>
      <linearGradient id="hl-lower" x1="100%" y1="10%" x2="0%" y2="85%">
        <stop offset="0%" stop-color="#AE7AE7" />
        <stop offset="50%" stop-color="#945AD7" />
        <stop offset="100%" stop-color="#7C40C4" />
      </linearGradient>
      <linearGradient id="hl-up" x1="382.808" y1="102.686" x2="312.000" y2="667.000" gradientUnits="userSpaceOnUse">
        <stop offset="0%" stop-color="#A76FE7" />
        <stop offset="50%" stop-color="#8B4FD7" />
        <stop offset="100%" stop-color="#7538C4" />
      </linearGradient>
      <linearGradient id="hl-bolt" x1="80%" y1="0%" x2="20%" y2="100%">
        <stop offset="0%" stop-color="#8F52DF" />
        <stop offset="50%" stop-color="#7B3ED1" />
        <stop offset="100%" stop-color="#682BBC" />
      </linearGradient>
      <linearGradient id="hl-hi" x1="80%" y1="0%" x2="20%" y2="100%">
        <stop offset="0%" stop-color="#B17CEC" />
        <stop offset="50%" stop-color="#9B62DF" />
        <stop offset="100%" stop-color="#864AD1" />
      </linearGradient>
    </defs>
    <g transform="matrix(0.73247291 0 0 0.73247291 -168.41092 -20.55943)">
      <g class="fill">
        <polygon points="378.886,656.640 765.520,474.844 846.844,586.742" fill="url(#hl-lower)" />
        <polygon points="380.288,453.717 469.099,433.560 312.000,667.000" fill="url(#hl-up)" />
        <polygon points="382.778,102.923 457.794,231.149 312.000,667.000" fill="url(#hl-up)" />
        <polygon
          points="826.262,88.139 687.772,331.537 846.827,326.564 313.152,666.266 574.884,401.725 424.606,424.957 522.000,240.000"
          fill="url(#hl-bolt)"
        />
        <polygon points="826.262,88.139 522.000,240.000 424.606,424.957 492.982,414.387" fill="url(#hl-hi)" />
      </g>
      <g aria-hidden="true">
        <polygon class="trace" points="312.000,667.000 382.778,102.923 457.794,231.149" />
        <polygon class="trace" points="312.000,667.000 380.288,453.717 469.099,433.560" />
        <polygon
          class="trace"
          points="312.000,667.000 378.886,656.640 765.520,474.844 846.844,586.742 378.886,656.640"
        />
        <polygon
          class="trace"
          points="313.152,666.266 574.884,401.725 424.606,424.957 522.000,240.000 826.262,88.139 687.772,331.537 846.827,326.564"
        />
      </g>
    </g>
  </svg>
</template>

<style scoped>
.hero-logo {
  width: 100%;
  height: auto;
  overflow: visible;
}
.trace {
  fill: none;
  stroke: #e9d8ff;
  stroke-width: 3;
  stroke-linejoin: round;
  opacity: 0;
}
</style>
