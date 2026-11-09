<script setup lang="ts">
/**
 * @fileoverview The rest of the family: Aria2 Next's build for the visitor's
 * system, Rayburst Connect in each browser's store, and Sponsor.
 */
import { useI18n } from 'vue-i18n'
import AppIcon from '@/components/AppIcon.vue'
import { detectBrowser } from '@/composables/github'
import { useEngineBuilds } from '@/composables/useEngineBuilds'
import { LINKS, STORES } from '@/links'
import { vReveal, vSpotlight } from '@/motion/directives'
import connectLogo from '@/assets/img/connect.svg'
import engineLogo from '@/assets/img/aria2-next.svg'

const { t } = useI18n()
const builds = useEngineBuilds()
const yours = detectBrowser()
</script>

<template>
  <div class="family">
    <h3 v-reveal class="family-title">{{ t('dl.family') }}</h3>
    <div class="family-grid">
      <article v-reveal v-spotlight class="fam fam-engine">
        <img class="fam-mark" :src="engineLogo" alt="" width="44" height="44" />
        <h4>Aria2 Next</h4>
        <p>{{ t('dl.engineSub') }}</p>
        <div class="fam-row">
          <a class="btn btn-gold btn-sm" :href="builds.url(builds.chosen.value)" target="_blank" rel="noopener">
            <AppIcon name="download-outline" />
            <span>{{ builds.chosen.value ? t('hero.download') : t('engine.getAny') }}</span>
          </a>
          <a class="link link-gold" href="#engine"><span>Docker</span><AppIcon name="arrow-forward-outline" /></a>
        </div>
      </article>
      <article v-reveal="1" v-spotlight class="fam fam-connect">
        <img class="fam-mark" :src="connectLogo" alt="" width="44" height="44" />
        <h4>Rayburst Connect</h4>
        <p>{{ t('dl.connectSub') }}</p>
        <div class="fam-stores">
          <a
            v-for="s in STORES"
            :key="s.id"
            class="fam-store"
            :class="{ 'is-yours': s.id === yours }"
            :href="s.href"
            target="_blank"
            rel="noopener"
            :aria-label="t(s.label)"
            :title="t(s.label)"
          >
            <AppIcon :name="s.icon" />
          </a>
        </div>
      </article>
      <article v-reveal="2" v-spotlight class="fam fam-sponsor beat-on-hover">
        <span class="fam-mark fam-heart" aria-hidden="true"><AppIcon name="heart" class="heart" /></span>
        <h4>{{ t('dl.sponsor') }}</h4>
        <p>{{ t('dl.sponsorSub') }}</p>
        <div class="fam-row">
          <a class="btn btn-sponsor btn-sm" :href="LINKS.sponsor" target="_blank" rel="noopener">
            <AppIcon name="heart" class="heart" /><span>{{ t('hero.sponsor') }}</span>
          </a>
        </div>
      </article>
    </div>
  </div>
</template>

<style scoped>
.family {
  margin-top: clamp(64px, 9vw, 112px);
}
.family-title {
  margin: 0 0 24px;
  color: var(--text-2);
  font-size: 14px;
  font-weight: 650;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  text-align: center;
}
.family-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
}
.fam {
  --c: var(--accent-2);
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  padding: 26px;
  border-radius: 24px;
  border: 1px solid var(--line);
  background:
    radial-gradient(90% 70% at 0% 0%, color-mix(in srgb, var(--c) 12%, transparent), transparent 70%), var(--card);
  overflow: hidden;
  transition:
    border-color 0.3s,
    transform 0.45s var(--ease-enter);
}
.fam:hover {
  border-color: color-mix(in srgb, var(--c) 40%, transparent);
  transform: translateY(-3px);
}
.fam-connect {
  --c: #8aa2ff;
}
.fam-engine {
  --c: #e3c07a;
}
:root[data-theme='light'] .fam-engine .link-gold {
  color: #8a6420;
}
.fam-sponsor {
  --c: var(--rose-ink);
}
.fam-mark {
  width: 44px;
  height: 44px;
  margin-bottom: 18px;
}
.fam-heart {
  display: grid;
  place-items: center;
  border-radius: 50%;
  background: color-mix(in srgb, var(--rose-ink) 14%, transparent);
  color: var(--rose);
  font-size: 22px;
}
.fam h4 {
  margin: 0;
  font-size: 19px;
  letter-spacing: -0.015em;
}
.fam p {
  flex: 1;
  margin: 8px 0 20px;
  color: var(--text-2);
  font-size: 14px;
}
.fam-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px 16px;
}
.fam-stores {
  display: flex;
  gap: 10px;
}
.fam-store {
  display: grid;
  place-items: center;
  width: 44px;
  height: 44px;
  border-radius: 14px;
  border: 1px solid var(--line-2);
  color: var(--text-2);
  font-size: 21px;
  transition:
    color 0.2s,
    background-color 0.2s,
    border-color 0.2s,
    transform 0.4s var(--ease-spring);
}
.fam-store:hover {
  color: #8aa2ff;
  border-color: color-mix(in srgb, #8aa2ff 55%, transparent);
  background: color-mix(in srgb, #8aa2ff 10%, transparent);
  transform: translateY(-2px) rotate(-8deg);
}
.fam-store.is-yours {
  color: #fff;
  border-color: transparent;
  background: linear-gradient(135deg, #8aa2ff, #7b3ed1);
}
@media (max-width: 900px) {
  .family-grid {
    grid-template-columns: 1fr;
    max-width: 520px;
    margin-inline: auto;
  }
}
</style>
