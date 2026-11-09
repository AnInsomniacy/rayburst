<script setup lang="ts">
/**
 * @fileoverview Make it yours: the studio around one live window, three
 * native moments (tray, file selection, open source).
 */
import { useI18n } from 'vue-i18n'
import { vReveal, vSpotlight } from '@/motion/directives'
import CodeMoment from './CodeMoment.vue'
import PickMoment from './PickMoment.vue'
import StudioPanel from './StudioPanel.vue'
import TrayMoment from './TrayMoment.vue'
import SectionField from '@/components/art/SectionField.vue'

const { t } = useI18n()

const MOMENTS = [
  { art: TrayMoment, title: 'craft.native', sub: 'craft.nativeSub' },
  { art: PickMoment, title: 'craft.pick', sub: 'craft.pickSub' },
  { art: CodeMoment, title: 'craft.open', sub: 'craft.openSub' },
]
</script>

<template>
  <section id="craft" class="sec craft">
    <SectionField :aria="{ y: 110, open: 'start', swell: 0.66 }" />
    <div class="wrap">
      <div class="sec-head">
        <h2 v-reveal class="title">{{ t('craft.title') }}</h2>
        <p v-reveal="1" class="lead">{{ t('craft.sub') }}</p>
      </div>
      <StudioPanel />
      <div class="moments">
        <article v-for="(m, i) in MOMENTS" :key="m.title" v-reveal="i" v-spotlight class="moment">
          <div class="moment-art"><component :is="m.art" /></div>
          <h3>{{ t(m.title) }}</h3>
          <p>{{ t(m.sub) }}</p>
        </article>
      </div>
    </div>
  </section>
</template>

<style scoped>
.moments {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
  margin-top: 16px;
}
.moment {
  display: flex;
  flex-direction: column;
  padding: 16px 16px 24px;
  border-radius: 26px;
  border: 1px solid var(--line);
  background: var(--card);
}
.moment-art {
  height: 236px;
  margin-bottom: 22px;
}
.moment h3 {
  margin: 0 8px;
  font-size: 19px;
  line-height: 1.25;
  font-weight: 700;
  letter-spacing: -0.015em;
}
.moment p {
  margin: 6px 8px 0;
  color: var(--text-2);
  font-size: 14px;
  line-height: 1.5;
}
@media (max-width: 960px) {
  .moments {
    grid-template-columns: 1fr;
    max-width: 520px;
    margin-inline: auto;
  }
}
</style>
