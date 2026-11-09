<script setup lang="ts">
/**
 * @fileoverview A first look at the whole window (sidebar, task cards, toolbar,
 * speedometer). A short tour on the left lights each part up in turn, or on
 * hover and focus.
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '@/components/AppIcon.vue'
import RbWindow from '@/components/mock/RbWindow.vue'
import type { WindowSpot } from '@/components/mock/types'
import { vReveal, vSpotlight } from '@/motion/directives'
import { useScene } from '@/motion/useScene'
import { KINDS, scenario, toastsFor } from '@/sim/tasks'
import type { IconName } from '@/ui/icons'
import SectionField from '@/components/art/SectionField.vue'

const { t } = useI18n()
const tr = (key: string, named?: Record<string, unknown>) => t(key, named ?? {})

const POINTS: { spot: WindowSpot | 'card'; icon: IconName; key: string }[] = [
  { spot: 'side', icon: 'list-outline', key: 'ov.p1' },
  { spot: 'card', icon: 'albums-outline', key: 'ov.p2' },
  { spot: 'tools', icon: 'options-outline', key: 'ov.p3' },
  { spot: 'speed', icon: 'speedometer-outline', key: 'ov.p4' },
]
const TOUR = 3.2

const model = scenario(
  [
    KINDS.live(-40, 2472),
    KINDS.ed2k(-60, 0.18),
    KINDS.sftp(-30, 0.34),
    KINDS.bt(-3, 0.58, 14, 1),
    KINDS.http(0, 0.52),
    KINDS.media(0, 0.36),
  ],
  { newestFirst: true },
)

const stage = ref<HTMLElement | null>(null)
const { time } = useScene(stage, { still: 30 })
const pinned = ref(-1)

const frame = computed(() => model(time.value, tr))
const toasts = computed(() => toastsFor(frame.value, time.value, tr))
const on = computed(() => (pinned.value >= 0 ? pinned.value : Math.floor(time.value / TOUR) % POINTS.length))
const spot = computed(() => POINTS[on.value]?.spot ?? null)
// The task-cards point makes the newest card glow.
const focus = computed(() => (spot.value === 'card' && frame.value.tasks[0] ? [frame.value.tasks[0].id] : []))
</script>

<template>
  <section id="overview" class="sec overview">
    <SectionField />
    <div class="wrap">
      <div class="sec-head">
        <h2 v-reveal class="title">{{ t('ov.title') }}</h2>
        <p v-reveal="1" class="lead">{{ t('ov.sub') }}</p>
      </div>
      <div class="ov-grid">
        <ul class="ov-points">
          <li v-for="(p, i) in POINTS" :key="p.key" v-reveal="i + 2">
            <div
              v-spotlight
              class="ov-point"
              :class="{ 'is-on': i === on }"
              tabindex="0"
              @pointerenter="pinned = i"
              @pointerleave="pinned = -1"
              @focus="pinned = i"
              @blur="pinned = -1"
            >
              <i><AppIcon :name="p.icon" /></i>
              <span>
                <b>{{ t(`${p.key}.t`) }}</b>
                <small>{{ t(`${p.key}.d`) }}</small>
              </span>
            </div>
          </li>
        </ul>
        <div ref="stage" v-reveal.scale="1" class="ov-stage">
          <RbWindow
            :frame="frame"
            :toasts="toasts"
            :height="640"
            :rows="4"
            :wide-from="0"
            :spot="spot === 'card' ? null : spot"
            :focus="focus"
          />
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.ov-grid {
  display: grid;
  grid-template-columns: minmax(0, 4fr) minmax(0, 8fr);
  gap: clamp(28px, 4vw, 64px);
  align-items: center;
}
.ov-points {
  display: grid;
  gap: 12px;
  margin: 0;
  padding: 0;
  list-style: none;
}
.ov-point {
  display: grid;
  grid-template-columns: 44px minmax(0, 1fr);
  align-items: center;
  gap: 16px;
  padding: 18px 20px;
  border-radius: 20px;
  border: 1px solid var(--line);
  background: var(--card);
  cursor: default;
  transition:
    border-color 0.3s,
    background-color 0.3s,
    transform 0.45s var(--ease-enter);
}
.ov-point > i {
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  border-radius: 13px;
  background: color-mix(in srgb, var(--accent-2) 14%, transparent);
  color: var(--accent);
  font-size: 20px;
  transition:
    transform 0.4s var(--ease-spring),
    background 0.3s,
    color 0.3s;
}
.ov-point b {
  display: block;
  font-size: 16px;
  font-weight: 650;
  letter-spacing: -0.01em;
}
.ov-point small {
  display: block;
  margin-top: 2px;
  color: var(--text-2);
  font-size: 14px;
  line-height: 1.45;
}
.ov-point.is-on {
  border-color: color-mix(in srgb, var(--accent-2) 50%, transparent);
  background:
    linear-gradient(90deg, color-mix(in srgb, var(--accent-2) 10%, transparent), transparent 70%), var(--card);
  transform: translateX(4px);
}
[dir='rtl'] .ov-point.is-on {
  background:
    linear-gradient(270deg, color-mix(in srgb, var(--accent-2) 10%, transparent), transparent 70%), var(--card);
  transform: translateX(-4px);
}
.ov-point.is-on > i {
  background: var(--brand);
  color: #fff;
  transform: scale(1.06);
}
.ov-stage {
  position: relative;
  min-width: 0;
}
.ov-stage::before {
  content: '';
  position: absolute;
  inset: 6% 4% -4%;
  z-index: -1;
  background: radial-gradient(50% 50% at 50% 50%, var(--glow-a), transparent 72%);
  filter: blur(30px);
}
@media (max-width: 960px) {
  .ov-grid {
    grid-template-columns: 1fr;
  }
}
</style>
