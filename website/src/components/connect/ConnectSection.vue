<script setup lang="ts">
/**
 * @fileoverview Rayburst Connect: the store buttons (the visitor's browser is
 * filled in), then the popup's real flow beside the desktop window. A pointer
 * works the popup; a beam carries the task to Rayburst, where it lands at the
 * top of the list as "Downloading media". Loops while visible.
 */
import { computed, ref, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '@/components/AppIcon.vue'
import LightRibbon from '@/components/mock/LightRibbon.vue'
import RbWindow from '@/components/mock/RbWindow.vue'
import { detectBrowser } from '@/composables/github'
import { LINKS, STORES } from '@/links'
import { vReveal, vSpotlight } from '@/motion/directives'
import { clamp, EASE, prog } from '@/motion/gsap'
import { useScene } from '@/motion/useScene'
import { KINDS, scenario } from '@/sim/tasks'
import { centerIn, rectIn, type Point } from '@/ui/geometry'
import connectLogo from '@/assets/img/connect.svg'
import MarkRing from '@/components/art/MarkRing.vue'
import SectionField from '@/components/art/SectionField.vue'
import ConnectBrowser from './ConnectBrowser.vue'
import { CLICKS, CUE, LOOP } from './timeline'

const { t } = useI18n()
const tr = (key: string, named?: Record<string, unknown>) => t(key, named ?? {})
const yours = detectBrowser()

const model = scenario(
  [KINDS.sftp(-20, 0.4), KINDS.bt(-50, 0.2, 14, 0), KINDS.http(-30, 0), KINDS.media(CUE.arrive, 0)],
  { newestFirst: true },
)

const stage = ref<HTMLElement | null>(null)
const mark = ref<HTMLElement | null>(null)
const browser = ref<InstanceType<typeof ConnectBrowser> | null>(null)
const win = ref<InstanceType<typeof RbWindow> | null>(null)
const { time } = useScene(stage, { loop: LOOP, still: 11 })

const frame = computed(() => model(time.value, tr))
const focus = computed(() => (frame.value.byId.media && time.value < CUE.arrive + 1 ? ['media' as const] : []))
// Soft reset around the loop point: dim out, restart, brighten back.
const appOpacity = computed(() => {
  const dim = Math.max(prog(time.value, CUE.fadeOut, LOOP - CUE.fadeOut), 1 - prog(time.value, 0, 0.5))
  return (1 - 0.65 * clamp(dim)).toFixed(3)
})

// ── Pointer: keyframes on the popup's controls, eased between keys ─────
type Key = { t: number; at: () => Point }
const center = (pick: () => Element | null | undefined) => (): Point => {
  const el = pick()
  return el && stage.value ? centerIn(el, stage.value) : [0, 0]
}
const rest = (): Point => {
  const el = browser.value?.frame()
  if (!el || !stage.value) return [0, 0]
  const r = rectIn(el, stage.value)
  return [r.x + r.w * 0.45, r.y + r.h * 0.55]
}
const KEYS: Key[] = [
  { t: 0, at: rest },
  { t: 0.4, at: rest },
  { t: CUE.openClick - 0.05, at: center(() => browser.value?.ext()) },
  { t: CUE.openClick + 0.35, at: center(() => browser.value?.ext()) },
  { t: CUE.rowClick - 0.05, at: center(() => browser.value?.rowDownload()) },
  { t: CUE.rowClick + 0.6, at: center(() => browser.value?.rowDownload()) },
  { t: CUE.mkvClick - 0.05, at: center(() => browser.value?.mkv()) },
  { t: CUE.mkvClick + 0.3, at: center(() => browser.value?.mkv()) },
  { t: CUE.dlClick - 0.05, at: center(() => browser.value?.download()) },
  { t: CUE.dlClick + 0.5, at: center(() => browser.value?.download()) },
]
function cursorAt(time: number): Point {
  const first = KEYS[0]
  if (time <= first.t) return first.at()
  for (let i = 1; i < KEYS.length; i++) {
    const b = KEYS[i]
    if (time > b.t) continue
    const a = KEYS[i - 1]
    const u = EASE.inOut((time - a.t) / (b.t - a.t))
    const pa = a.at()
    const pb = b.at()
    return [pa[0] + (pb[0] - pa[0]) * u, pa[1] + (pb[1] - pa[1]) * u]
  }
  return KEYS[KEYS.length - 1].at()
}
const cursor = computed(() => {
  const [x, y] = cursorAt(time.value)
  return { translate: `${x - 4}px ${y - 3}px`, hidden: time.value < 0.3 || time.value > CUE.dlClick + 0.8 }
})

// Clicks: a ripple where the pointer is.
const ripples = shallowRef<{ id: number; x: number; y: number }[]>([])
let rippleId = 0
watch(time, (now, before) => {
  for (const c of CLICKS) {
    if (before < c && now >= c) {
      const [x, y] = cursorAt(c)
      ripples.value = [...ripples.value, { id: rippleId++, x, y }]
    }
  }
})
const dropRipple = (id: number) => (ripples.value = ripples.value.filter((r) => r.id !== id))

// Hand-off beam from the success alert to the top of Rayburst's list.
const beam = computed(() => {
  const u = prog(time.value, CUE.fly, CUE.arrive - CUE.fly)
  const fade = 1 - prog(time.value, CUE.arrive, 0.6)
  const done = browser.value?.done()
  const list = win.value?.listEl()
  if (u <= 0 || fade <= 0 || !done || !list || !stage.value) return null
  const a = rectIn(done, stage.value)
  const b = rectIn(list, stage.value)
  const k = win.value?.scale ?? 1
  return {
    from: [a.x + a.w - 12, a.y + a.h / 2] as Point,
    to: [b.x + 30 * k, b.y + 20 * k] as Point,
    u: EASE.inOut(u),
    alpha: fade,
  }
})
const matchEl = computed(() => browser.value?.frame() ?? null)
</script>

<template>
  <section id="connect" class="sec connect">
    <SectionField :ring="mark" :aria="{ y: 110, open: 'end', swell: 0.66 }" />
    <div class="wrap">
      <div class="sec-head">
        <div ref="mark" v-reveal.scale class="connect-mark">
          <MarkRing><img :src="connectLogo" alt="" width="84" height="84" /></MarkRing>
        </div>
        <h2 v-reveal="1" class="title">Rayburst Connect</h2>
        <p v-reveal="2" class="lead">{{ t('connect.tagline') }}</p>
        <div v-reveal="3" class="stores">
          <a
            v-for="s in STORES"
            :key="s.id"
            v-spotlight
            class="store"
            :class="{ 'is-yours': s.id === yours }"
            :href="s.href"
            target="_blank"
            rel="noopener"
          >
            <AppIcon :name="s.icon" />
            <span
              ><b>{{ t('connect.add', { browser: s.name }) }}</b
              ><small>{{ t(s.label) }}</small></span
            >
          </a>
        </div>
      </div>
      <div ref="stage" v-reveal.scale="4" class="connect-stage">
        <ConnectBrowser ref="browser" :time="time" />
        <div class="connect-app" :style="{ opacity: appOpacity }">
          <RbWindow ref="win" :frame="frame" :height="700" :rows="4" :match="matchEl" :focus="focus" />
        </div>
        <svg class="stage-fx" aria-hidden="true">
          <LightRibbon
            v-if="beam"
            :from="beam.from"
            :to="beam.to"
            :progress="beam.u"
            :alpha="beam.alpha"
            color="#a8b8ff"
          />
        </svg>
        <svg
          class="cursor"
          :class="{ 'is-hidden': cursor.hidden }"
          :style="{ translate: cursor.translate }"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <path
            d="M5 3l14 8.2-6.2 1.3 3.6 7-2.7 1.4-3.6-7.1L5 18.6z"
            fill="#fff"
            stroke="#1b1520"
            stroke-width="1.3"
            stroke-linejoin="round"
          />
        </svg>
        <i
          v-for="r in ripples"
          :key="r.id"
          class="ripple"
          :style="{ translate: `${r.x}px ${r.y}px` }"
          @animationend="dropRipple(r.id)"
        />
      </div>
      <div v-reveal class="connect-foot">
        <span>{{ t('connect.preview') }}</span>
        <a class="link" :href="LINKS.connectReleases" target="_blank" rel="noopener">
          <span>{{ t('connect.offline') }}</span
          ><AppIcon name="arrow-forward-outline" />
        </a>
      </div>
    </div>
  </section>
</template>

<style scoped>
.connect {
  --c: var(--connect);
}
.connect-mark {
  width: 84px;
  margin: 24px auto 44px;
}
.stores {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 12px;
  margin-top: 32px;
}
.store {
  display: inline-flex;
  align-items: center;
  gap: 12px;
  min-width: 210px;
  height: 60px;
  padding: 0 20px 0 16px;
  border-radius: 16px;
  border: 1px solid var(--line-2);
  background: color-mix(in srgb, var(--text) 4%, transparent);
  text-align: start;
  transition:
    transform 0.35s var(--ease-enter),
    background-color 0.25s,
    border-color 0.25s,
    box-shadow 0.35s var(--ease-enter);
}
.store > .ic {
  font-size: 28px;
  color: var(--c);
  transition: transform 0.45s var(--ease-spring);
}
.store span {
  display: grid;
  line-height: 1.25;
}
.store b {
  font-size: 16px;
  font-weight: 650;
}
.store small {
  color: var(--text-3);
  font-size: 12px;
}
.store:hover {
  transform: translateY(-2px);
  border-color: color-mix(in srgb, var(--c) 55%, transparent);
  background-color: color-mix(in srgb, var(--c) 9%, transparent);
}
.store:hover > .ic {
  transform: rotate(-12deg) scale(1.08);
}
.store:active {
  transform: scale(0.98);
}
/* The visitor's own browser, filled in. */
.store.is-yours {
  color: #fff;
  border-color: transparent;
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.16), rgba(255, 255, 255, 0) 55%),
    linear-gradient(135deg, #8aa2ff 0%, #7b6cf0 55%, #7b3ed1 100%);
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.25),
    0 12px 32px -10px rgba(110, 110, 240, 0.7);
}
.store.is-yours > .ic,
.store.is-yours small {
  color: rgba(255, 255, 255, 0.85);
}
@media (max-width: 720px) {
  .store {
    flex: 1 1 100%;
    max-width: 360px;
  }
}
.connect-stage {
  position: relative;
  display: grid;
  grid-template-columns: minmax(0, 6fr) minmax(0, 5fr);
  gap: clamp(24px, 4vw, 56px);
  align-items: center;
}
.connect-app {
  position: relative;
}
.stage-fx {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  overflow: visible;
  pointer-events: none;
  z-index: 5;
}
.connect-foot {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-top: 28px;
  padding-top: 24px;
  border-top: 1px solid var(--line);
  color: var(--text-3);
  font-size: 14px;
}
.cursor {
  position: absolute;
  top: 0;
  left: 0;
  width: 22px;
  height: 22px;
  z-index: 8;
  pointer-events: none;
  filter: drop-shadow(0 2px 4px rgba(0, 0, 0, 0.5));
  transition: opacity 0.3s;
}
.cursor.is-hidden {
  opacity: 0;
}
.ripple {
  position: absolute;
  top: 0;
  left: 0;
  width: 44px;
  height: 44px;
  margin: -22px 0 0 -22px;
  border-radius: 50%;
  border: 2px solid rgba(214, 186, 255, 0.8);
  pointer-events: none;
  z-index: 7;
  animation: ripple 0.6s var(--ease-enter) forwards;
}
@keyframes ripple {
  from {
    transform: scale(0.2);
    opacity: 1;
  }
  to {
    transform: scale(1);
    opacity: 0;
  }
}
@media (max-width: 860px) {
  .connect-stage {
    grid-template-columns: 1fr;
  }
}
</style>
