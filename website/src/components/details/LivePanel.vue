<script setup lang="ts">
/**
 * @fileoverview 03 · Live recording: a running timecode while HLS segments
 * stream into the recording card.
 */
import { useElementSize } from '@vueuse/core'
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '@/components/AppIcon.vue'
import TaskCard from '@/components/mock/TaskCard.vue'
import DetailCopy from './DetailCopy.vue'
import { clock } from '@/sim/format'
import { KINDS } from '@/sim/tasks'

// `active` is part of the panels' shared interface; this panel draws nothing off screen.
const props = defineProps<{ time: number; active?: boolean }>()
const { t } = useI18n()
const tr = (key: string, named?: Record<string, unknown>) => t(key, named ?? {})

const SEG_EVERY = 0.55
const SEG_SPEED = 170
const SEG_LIFE = 5
const SLOTS = Math.ceil(SEG_LIFE / SEG_EVERY) + 2

const task = KINDS.live(-1.2, 2472)
const view = computed(() => task(props.time, tr))

const lane = ref<HTMLElement | null>(null)
const { width } = useElementSize(lane)
const segments = computed(() => {
  const lt = props.time
  const newest = Math.floor(lt / SEG_EVERY)
  const lw = width.value || 600
  return Array.from({ length: SLOTS }, (_, j) => {
    const k = newest - j
    const age = lt - k * SEG_EVERY
    if (k < 0 || age > SEG_LIFE) return null
    const x = -150 + age * SEG_SPEED
    return {
      key: k,
      name: `seg-${1400 + k}.m4s`,
      transform: `translate(${x.toFixed(1)}px, ${(k % 3) * 40 + 4}px)`,
      visible: x <= lw - 60,
    }
  }).filter((s) => s !== null)
})
</script>

<template>
  <DetailCopy :n="3" :title="t('d3.title')" :sub="t('d3.sub')">
    <div class="metric">
      <b>{{ clock(view?.rec ?? 0) }}</b
      ><span class="is-live">{{ t('ui.recording') }} · {{ t('ui.live') }}</span>
    </div>
    <div class="metric">
      <b>{{ view?.right?.down ?? '0 KB/s' }}</b
      ><span>{{ t('ui.d.down') }}</span>
    </div>
  </DetailCopy>
  <div class="dp-viz">
    <div ref="lane" class="segs">
      <span
        v-for="s in segments"
        :key="s.key"
        class="stream-segment"
        :style="{ transform: s.transform, opacity: s.visible ? 1 : 0 }"
      >
        <AppIcon name="film-outline" /><em>{{ s.name }}</em>
      </span>
    </div>
    <div class="dcard rbw rbw-solo"><TaskCard v-if="view" :task="view" /></div>
  </div>
</template>

<style scoped>
.segs {
  position: relative;
  height: 124px;
  overflow: hidden;
  -webkit-mask-image: linear-gradient(90deg, transparent, #000 12%, #000 80%, transparent);
  mask-image: linear-gradient(90deg, transparent, #000 12%, #000 80%, transparent);
}
.stream-segment {
  position: absolute;
  top: 0;
  left: 0;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 12px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--c) 12%, var(--card-2));
  border: 1px solid color-mix(in srgb, var(--c) 30%, transparent);
  color: var(--text-2);
  font: 500 12px/1 var(--mono);
  white-space: nowrap;
  will-change: transform;
}
.stream-segment em {
  font-style: normal;
}
</style>
