<script setup lang="ts">
/**
 * @fileoverview Every protocol, in one place. Six protocol families light up
 * one after another: the name fills with colour, its real link types out, and
 * a ribbon of light carries it into the live window, where the task lands at
 * the top of the list. The magnet link becomes a torrent task ("Fetching
 * torrent"); BitTorrent lights up when its metadata arrives and the same task
 * starts downloading pieces. After the sequence, a click on any protocol
 * sends its ribbon again.
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '@/components/AppIcon.vue'
import LightRibbon from '@/components/mock/LightRibbon.vue'
import RbWindow from '@/components/mock/RbWindow.vue'
import { vReveal } from '@/motion/directives'
import { clamp, EASE, prog } from '@/motion/gsap'
import { useScene } from '@/motion/useScene'
import { KINDS, LINKS, scenario, toastsFor, type TaskId } from '@/sim/tasks'
import { isRtlPage, rectIn, type Point } from '@/ui/geometry'
import SectionField from '@/components/art/SectionField.vue'

const { t } = useI18n()
const tr = (key: string, named?: Record<string, unknown>) => t(key, named ?? {})

const START = 0.6
const STEP = 1.7
const FLY = 0.55
const at = (i: number) => START + i * STEP
const land = (i: number) => at(i) + FLY

const LINES: { name: () => string; kind: string; color: string; link: string; task: TaskId }[] = [
  { name: () => 'HTTP(S)', kind: 'proto.k.files', color: '#A98BFF', link: LINKS.http, task: 'http' },
  { name: () => 'SFTP', kind: 'proto.k.files', color: '#7C95FF', link: LINKS.sftp, task: 'sftp' },
  { name: () => t('proto.magnet'), kind: 'proto.k.p2p', color: '#FF7A93', link: LINKS.magnet, task: 'bt' },
  {
    name: () => 'BitTorrent',
    kind: 'proto.k.p2p',
    color: '#5FD39A',
    link: 'btih:9c4e2b7d1a0f53e8… · 642 MB · 2568 pieces',
    task: 'bt',
  },
  { name: () => 'ED2K', kind: 'proto.k.p2p', color: '#F0B64A', link: LINKS.ed2k, task: 'ed2k' },
  { name: () => 'HLS · DASH', kind: 'proto.k.streams', color: '#4FD0E6', link: LINKS.live, task: 'live' },
]
const END = land(LINES.length - 1) + 0.6

const model = scenario(
  [
    KINDS.http(land(0), 0),
    KINDS.sftp(land(1), 0),
    KINDS.bt(land(2), 0, 16, land(3) - land(2)),
    KINDS.ed2k(land(4), 0),
    KINDS.live(land(5), 0),
  ],
  { newestFirst: true },
)

const stage = ref<HTMLElement | null>(null)
const names = ref<HTMLElement[]>([])
const win = ref<InstanceType<typeof RbWindow> | null>(null)
const scene = useScene(stage, { still: END + 1 })
const time = scene.time

// After the sequence, a click re-sends that protocol.
const replayIdx = ref(-1)
const replayAt = ref(-10)
function resend(i: number) {
  if (time.value < END) return
  replayIdx.value = i
  replayAt.value = time.value
}
function replay() {
  replayIdx.value = -1
  scene.replay()
}

const frame = computed(() => model(time.value, tr))
const toasts = computed(() => toastsFor(frame.value, time.value, tr))
// A brief focus ring where a ribbon lands.
const focus = computed(() =>
  LINES.flatMap((ln, i) => {
    const landed = replayIdx.value === i ? replayAt.value + FLY : land(i)
    return frame.value.byId[ln.task] && time.value >= landed && time.value < landed + 0.9 ? [ln.task] : []
  }),
)

const lit = (i: number) => time.value >= at(i)
const current = computed(() => {
  if (time.value >= END && replayIdx.value >= 0) return replayIdx.value
  let c = -1
  LINES.forEach((_, i) => lit(i) && (c = i))
  return c
})
const typed = (i: number) => {
  const chars = [...LINES[i].link]
  return chars.slice(0, Math.ceil(chars.length * clamp((time.value - at(i)) / 0.5))).join('')
}

function source(i: number): Point {
  const el = names.value[i]
  if (!el || !stage.value) return [0, 0]
  const r = rectIn(el, stage.value)
  return [isRtlPage() ? r.x : r.x + r.w, r.y + r.h * 0.55]
}
function target(i: number): Point {
  // A new task lands at the top of the list; BitTorrent (and replays) go to the existing card.
  const el = win.value?.cardEl(LINES[i].task) ?? win.value?.listEl()
  if (!el || !stage.value) return [0, 0]
  const r = rectIn(el, stage.value)
  const k = win.value?.scale ?? 1
  return [r.x + 40 * k, r.y + 24 * k]
}

const ribbons = computed(() =>
  LINES.map((ln, i) => {
    const start = replayIdx.value === i && time.value >= replayAt.value ? replayAt.value : at(i)
    const u = prog(time.value, start + 0.05, FLY)
    const fade = 1 - prog(time.value, start + 0.05 + FLY, 0.5)
    if (u <= 0 || fade <= 0) return { color: ln.color, from: [0, 0] as Point, to: [0, 0] as Point, u: 0, alpha: 0 }
    return { color: ln.color, from: source(i), to: target(i), u: EASE.inOut(u), alpha: 0.9 * fade }
  }),
)
</script>

<template>
  <section id="protocols" class="sec protocols">
    <SectionField :aria="{ y: 110, open: 'start', swell: 0.66 }" />
    <div class="wrap">
      <div class="sec-head">
        <h2 v-reveal class="title">{{ t('proto.title') }}</h2>
        <p v-reveal="1" class="lead">{{ t('proto.sub') }}</p>
      </div>
      <div ref="stage" class="proto-stage">
        <div>
          <ol v-reveal="2" class="proto-list">
            <li v-for="(ln, i) in LINES" :key="i">
              <button
                class="proto"
                type="button"
                :class="{ 'is-lit': lit(i), 'is-current': current === i }"
                :style="{ '--c0': ln.color }"
                @click="resend(i)"
              >
                <span ref="names" class="proto-name" :data-name="ln.name()">{{ ln.name() }}</span>
                <span class="proto-kind">{{ t(ln.kind) }}</span>
                <code class="proto-link">{{ typed(i) }}</code>
              </button>
            </li>
          </ol>
          <button class="replay" type="button" @click="replay">
            <AppIcon name="refresh-outline" /><span>{{ t('ctl.replay') }}</span>
          </button>
        </div>
        <div v-reveal.scale="3" class="proto-app">
          <RbWindow ref="win" :frame="frame" :toasts="toasts" :height="700" :rows="5" :focus="focus" />
        </div>
        <svg class="stage-fx" aria-hidden="true">
          <LightRibbon
            v-for="(r, i) in ribbons"
            :key="i"
            :from="r.from"
            :to="r.to"
            :progress="r.u"
            :alpha="r.alpha"
            :color="r.color"
          />
        </svg>
      </div>
    </div>
  </section>
</template>

<style scoped>
.proto-stage {
  position: relative;
  display: grid;
  grid-template-columns: minmax(0, 5fr) minmax(0, 7fr);
  gap: clamp(28px, 4vw, 64px);
  align-items: center;
}
.proto-list {
  margin: 0;
  padding: 0;
  list-style: none;
}
.proto {
  --c: var(--c0);
  position: relative;
  display: grid;
  grid-template-columns: auto 1fr;
  align-items: baseline;
  column-gap: 14px;
  width: 100%;
  padding-block: 12px;
  padding-inline: 22px 0;
  text-align: start;
}
:root[data-theme='light'] .proto {
  --c: color-mix(in srgb, var(--c0) 72%, #000);
}
.proto::before {
  content: '';
  position: absolute;
  inset-inline-start: 0;
  top: 18px;
  bottom: 18px;
  width: 3px;
  border-radius: 3px;
  background: var(--c);
  box-shadow: 0 0 14px var(--c);
  transform: scaleY(0);
  transition: transform 0.5s var(--ease-enter);
}
.proto-name {
  position: relative;
  font-size: clamp(30px, 3.4vw, 46px);
  line-height: 1.1;
  font-weight: 780;
  letter-spacing: -0.03em;
  color: color-mix(in srgb, var(--text) 16%, transparent);
  transition: color 0.4s;
}
.proto-name::after {
  content: attr(data-name);
  position: absolute;
  inset: 0;
  background: linear-gradient(90deg, var(--text) 0%, color-mix(in srgb, var(--c) 70%, var(--text)) 100%);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
  clip-path: inset(0 100% 0 0);
  transition: clip-path 0.55s var(--ease-enter);
}
[dir='rtl'] .proto-name::after {
  clip-path: inset(0 0 0 100%);
}
.proto-kind {
  color: var(--text-3);
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  transition: color 0.4s;
}
.proto-link {
  grid-column: 1 / -1;
  display: block;
  height: 0;
  overflow: hidden;
  color: var(--text-2);
  font-size: 13px;
  white-space: nowrap;
  text-overflow: ellipsis;
  direction: ltr;
  text-align: left;
  opacity: 0;
  transition:
    height 0.4s var(--ease-enter),
    opacity 0.3s,
    margin 0.4s var(--ease-enter);
}
.proto.is-lit::before {
  transform: scaleY(1);
}
.proto.is-lit .proto-name::after {
  clip-path: inset(0 0 0 0);
}
.proto.is-lit .proto-kind {
  color: var(--c);
}
.proto.is-current .proto-link {
  height: 20px;
  margin-top: 6px;
  opacity: 1;
}
.proto:hover .proto-name {
  color: color-mix(in srgb, var(--text) 30%, transparent);
}
.proto-app {
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
.replay {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  margin-top: 18px;
  padding: 6px 12px 6px 10px;
  border-radius: 999px;
  color: var(--text-3);
  font-size: 13px;
  font-weight: 500;
  transition:
    color 0.2s,
    background-color 0.2s;
}
.replay:hover {
  color: var(--text);
  background: color-mix(in srgb, var(--text) 7%, transparent);
}
.replay .ic {
  font-size: 15px;
  transition: transform 0.5s var(--ease-enter);
}
.replay:hover .ic {
  transform: rotate(-180deg);
}
@media (max-width: 860px) {
  .proto-stage {
    grid-template-columns: 1fr;
  }
  .proto-list {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    column-gap: 16px;
  }
  .proto-name {
    font-size: 26px;
  }
  .proto-link {
    display: none;
  }
}
</style>
