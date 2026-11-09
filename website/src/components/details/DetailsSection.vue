<script setup lang="ts">
/**
 * @fileoverview Details, live. Three tabs advance on their own clock; picking
 * one jumps to it and the cycle continues from there. Only the tab on screen
 * follows the clock; the others hold their last frame.
 */
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from 'reka-ui'
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { vReveal } from '@/motion/directives'
import { useScene } from '@/motion/useScene'
import ConnectionsPanel from './ConnectionsPanel.vue'
import LivePanel from './LivePanel.vue'
import PiecesPanel from './PiecesPanel.vue'
import SectionField from '@/components/art/SectionField.vue'

const { t } = useI18n()

const DUR = 6
const STILL = 4.6
const TABS = [
  { key: 'd1', color: '#b98cf0', panel: ConnectionsPanel },
  { key: 'd2', color: '#5fcf8a', panel: PiecesPanel },
  { key: 'd3', color: '#8fb0ff', panel: LivePanel },
]

const view = ref<HTMLElement | null>(null)
const scene = useScene(view, { still: 0 })
const offset = ref(0)

const cycle = computed(() => {
  const span = DUR * TABS.length
  return (((scene.time.value - offset.value) % span) + span) % span
})
const index = computed(() => Math.floor(cycle.value / DUR))
const local = computed(() => (scene.reduced.value ? STILL : cycle.value - index.value * DUR))
const selected = computed({
  get: () => String(index.value),
  set: (value: string | number) => {
    offset.value = scene.time.value - Number(value) * DUR
  },
})
</script>

<template>
  <section id="details" class="sec details">
    <SectionField />
    <div class="wrap">
      <div class="sec-head">
        <h2 v-reveal class="title">{{ t('details.title') }}</h2>
        <p v-reveal="1" class="lead">{{ t('details.sub') }}</p>
      </div>
      <TabsRoot v-model="selected" class="details-stage" activation-mode="manual">
        <TabsList v-reveal="2" class="dtabs" :aria-label="t('details.title')">
          <TabsTrigger
            v-for="(tab, i) in TABS"
            :key="tab.key"
            class="dtab"
            :value="String(i)"
            :style="{ '--c': tab.color, '--p': index === i ? (local / DUR).toFixed(4) : 0 }"
          >
            <span class="dtab-n mono">0{{ i + 1 }}</span>
            <span class="dtab-t">{{ t(`${tab.key}.title`) }}</span>
          </TabsTrigger>
        </TabsList>
        <div ref="view" v-reveal.scale="3" class="dview" :style="{ '--c': TABS[index].color }">
          <TabsContent v-for="(tab, i) in TABS" :key="tab.key" :value="String(i)" force-mount class="dpanel">
            <component :is="tab.panel" :time="index === i ? local : STILL" :active="index === i" />
          </TabsContent>
        </div>
      </TabsRoot>
    </div>
  </section>
</template>

<style scoped>
.details-stage {
  display: grid;
  justify-items: center;
  gap: 28px;
}

/* Tabs: one segmented bar; the selected tab fills with its colour as its time runs. */
.dtabs {
  display: inline-flex;
  max-width: 100%;
  padding: 5px;
  gap: 4px;
  border-radius: 16px;
  border: 1px solid var(--line);
  background: color-mix(in srgb, var(--text) 3%, transparent);
  overflow-x: auto;
  scrollbar-width: none;
}
.dtab {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 10px 18px;
  border-radius: 12px;
  color: var(--text-2);
  font-size: 15px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  transition:
    color 0.3s,
    background-color 0.3s,
    box-shadow 0.3s;
}
.dtab:hover {
  color: var(--text);
}
.dtab[data-state='active'] {
  color: var(--text);
  background: var(--card);
  box-shadow:
    0 1px 2px rgba(0, 0, 0, 0.12),
    0 0 0 1px var(--line);
}
.dtab-n {
  color: var(--text-3);
  font-size: 12px;
  transition: color 0.3s;
}
.dtab[data-state='active'] .dtab-n {
  color: var(--c);
}
.dtab::after {
  content: '';
  position: absolute;
  inset: auto 14px 5px;
  height: 2px;
  border-radius: 2px;
  background: var(--c);
  transform-origin: 0 50%;
  transform: scaleX(var(--p, 0));
  opacity: 0;
  transition: opacity 0.3s;
}
[dir='rtl'] .dtab::after {
  transform-origin: 100% 50%;
}
.dtab[data-state='active']::after {
  opacity: 0.9;
}

/* Stage: every panel shares one grid cell, so switching never changes the height. */
.dview {
  position: relative;
  display: grid;
  width: 100%;
  border-radius: 28px;
  border: 1px solid var(--line);
  background:
    radial-gradient(70% 90% at 100% 0%, color-mix(in srgb, var(--c) 14%, transparent), transparent 60%),
    radial-gradient(50% 70% at 0% 100%, color-mix(in srgb, var(--c) 7%, transparent), transparent 60%), var(--card);
  box-shadow: 0 40px 100px -60px color-mix(in srgb, var(--c) 60%, transparent);
  overflow: hidden;
  transition:
    background 0.8s var(--ease-enter),
    box-shadow 0.8s var(--ease-enter);
}
.dpanel {
  grid-area: 1 / 1;
  display: grid;
  grid-template-columns: minmax(0, 5fr) minmax(0, 7fr);
  gap: clamp(28px, 4vw, 56px);
  padding: clamp(24px, 3.4vw, 44px);
  outline: none;
  opacity: 0;
  visibility: hidden;
  transform: translateY(10px);
  transition:
    opacity 0.3s var(--ease-exit),
    transform 0.3s var(--ease-exit),
    visibility 0s 0.3s;
}
.dpanel[data-state='active'] {
  opacity: 1;
  visibility: visible;
  transform: none;
  transition:
    opacity 0.6s 0.1s var(--ease-text),
    transform 0.8s 0.1s var(--ease-text);
}
.dpanel :deep(.dp-viz) {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 20px;
  min-width: 0;
}
.dpanel :deep(.dcard) {
  --w-main: transparent;
}
@media (max-width: 860px) {
  .dpanel {
    grid-template-columns: 1fr;
  }
  .dtab {
    padding: 9px 14px;
    font-size: 14px;
  }
}
</style>
