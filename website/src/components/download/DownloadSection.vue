<script setup lang="ts">
/**
 * @fileoverview Get Rayburst: one button for the visitor's system with an
 * architecture switch, a card per platform with every package, and a slow
 * logo in its ring of light with ripples spreading over the section; then the
 * rest of the family.
 */
import { ToggleGroupItem, ToggleGroupRoot } from 'reka-ui'
import { computed, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '@/components/AppIcon.vue'
import type { Arch, DesktopOS } from '@/composables/github'
import { SYSTEMS, useDesktopRelease } from '@/composables/useDesktopRelease'
import { useDownloadPulse } from '@/composables/useDownloadPulse'
import { LINKS } from '@/links'
import { vReveal, vSpotlight } from '@/motion/directives'
import { EASE, Flip, reducedMotion } from '@/motion/gsap'
import { fileSize } from '@/sim/format'
import logo from '@/assets/img/logo.svg'
import MarkRing from '@/components/art/MarkRing.vue'
import SectionField from '@/components/art/SectionField.vue'
import FamilyCards from './FamilyCards.vue'

const { t } = useI18n()
const release = useDesktopRelease()
const os = release.os

const logoEl = ref<HTMLElement | null>(null)
const primary = ref<HTMLElement | null>(null)

// Object.keys widens to string[]; the keys of SYSTEMS[os].arch are Arch by construction.
const archKeys = computed(() => (os ? (Object.keys(SYSTEMS[os].arch) as Arch[]) : []))
const primaryMeta = computed(() => {
  if (!os) return ''
  const sys = SYSTEMS[os]
  return [os, sys.arch[release.arch.value], sys.rows[0].name, fileSize(release.main.value?.asset?.size)]
    .filter(Boolean)
    .join(' · ')
})
const primaryHref = computed(() => release.main.value?.asset?.browser_download_url || LINKS.releases)

// The button's width glides when its label changes (architecture or package size).
async function setArch(value: unknown) {
  if (value !== 'arm' && value !== 'x86') return
  const el = primary.value
  const state = el && !reducedMotion() ? Flip.getState(el) : null
  release.arch.value = value
  if (!state) return
  await nextTick()
  Flip.from(state, { duration: 0.35, ease: EASE.enter, scale: false })
}

const platforms = computed(() =>
  // Object.entries widens keys to string; SYSTEMS is keyed by DesktopOS.
  (Object.entries(SYSTEMS) as [DesktopOS, (typeof SYSTEMS)[DesktopOS]][]).map(([name, sys]) => ({
    name,
    sys,
    yours: name === os,
    rows: sys.rows.map((row) => ({ ...row, links: release.packagesFor(name, row.kind) })),
  })),
)

// Links to #download glide down to the section, then the main button glows once.
const pulse = useDownloadPulse()
const pulsing = ref(false)
function glow() {
  pulsing.value = false
  requestAnimationFrame(() => (pulsing.value = true))
}
watch(pulse.pulses, () => {
  const r = primary.value?.getBoundingClientRect()
  if (!r || (r.top >= 0 && r.bottom <= innerHeight) || reducedMotion()) return glow()
  if ('onscrollend' in window) window.addEventListener('scrollend', glow, { once: true })
  else setTimeout(glow, 900)
})
</script>

<template>
  <section id="download" class="sec get">
    <SectionField :ring="logoEl" />
    <div class="wrap">
      <div class="get-hero">
        <div ref="logoEl" v-reveal.scale class="get-logo">
          <MarkRing><img :src="logo" alt="" width="104" height="104" /></MarkRing>
        </div>
        <h2 v-reveal="1" class="title">{{ t('dl.title') }}</h2>
        <p v-reveal="2" class="lead">{{ t('dl.sub') }}</p>
        <div v-reveal="3" class="get-main">
          <a
            ref="primary"
            class="btn btn-primary btn-xl"
            :class="{ 'is-pulse': pulsing }"
            :href="primaryHref"
            target="_blank"
            rel="noopener"
            @animationend="pulsing = false"
          >
            <AppIcon name="download-outline" />
            <span class="btn-stack">
              <b>{{ os ? t('hero.download') : t('dl.primary.fallback') }}</b>
              <small class="mono">{{ primaryMeta }}</small>
            </span>
          </a>
          <ToggleGroupRoot
            v-if="os"
            type="single"
            class="seg"
            :model-value="release.arch.value"
            :style="{ '--at': archKeys.indexOf(release.arch.value), '--n': archKeys.length }"
            @update:model-value="setArch"
          >
            <i class="seg-thumb" aria-hidden="true" />
            <ToggleGroupItem v-for="k in archKeys" :key="k" :value="k">{{ SYSTEMS[os].arch[k] }}</ToggleGroupItem>
          </ToggleGroupRoot>
          <p class="get-meta">
            <b v-if="release.version.value" class="mono">{{ release.version.value }}</b>
            <a :href="release.notes.value" target="_blank" rel="noopener">{{ t('dl.notes') }}</a>
            <a :href="LINKS.releases" target="_blank" rel="noopener">{{ t('dl.releases') }}</a>
          </p>
        </div>
      </div>

      <div class="plats">
        <article
          v-for="(p, i) in platforms"
          :key="p.name"
          v-reveal="i"
          v-spotlight
          class="plat"
          :class="{ 'is-yours': p.yours }"
        >
          <header>
            <AppIcon :name="p.sys.icon" />
            <h4>{{ p.name }}</h4>
            <span v-if="p.yours" class="plat-badge">{{ t('dl.yours') }}</span>
          </header>
          <div class="pkg-rows">
            <div v-for="row in p.rows" :key="row.kind" class="pkg-row">
              <div class="pkg-name">
                <b>{{ row.name }}</b
                ><small v-if="row.sub">{{ t(row.sub) }}</small>
              </div>
              <div class="pkg-links">
                <a
                  v-for="l in row.links"
                  :key="l.pkg.arch"
                  class="pkg"
                  :href="l.asset?.browser_download_url || LINKS.releases"
                  :target="l.asset ? undefined : '_blank'"
                  :rel="l.asset ? undefined : 'noopener'"
                  :title="fileSize(l.asset?.size) || undefined"
                >
                  <AppIcon name="download-outline" /><span>{{ p.sys.arch[l.pkg.arch] }}</span>
                </a>
              </div>
            </div>
          </div>
          <p class="plat-help">
            {{ t('dl.help.prompt') }}
            <i18n-t v-if="p.name === 'Linux'" keypath="dl.help.linux" tag="span" scope="global">
              <template #cmd><code>uname -m</code></template>
            </i18n-t>
            <span v-else>{{ t(p.sys.help) }}</span>
          </p>
        </article>
      </div>
      <p v-reveal class="get-note">{{ t('dl.note') }}</p>

      <FamilyCards />
    </div>
  </section>
</template>

<style scoped src="./download.css"></style>
