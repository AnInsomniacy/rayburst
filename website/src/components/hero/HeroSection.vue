<script setup lang="ts">
/**
 * @fileoverview The first screen, after the README banner and social preview:
 * the mark large in front of its ring of light, the name beside it. The logo
 * traces itself and fills in while the ring draws out from its point, the name
 * rises letter by letter, then the slogan, actions, film card and statistics
 * follow. One GSAP timeline plays the intro once the fonts are in; the halo
 * and the aria behind it run on their own scene clock.
 */
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '@/components/AppIcon.vue'
import { useDownloadPulse } from '@/composables/useDownloadPulse'
import { useStats } from '@/composables/useStats'
import { LINKS } from '@/links'
import { EASE, gsap, reducedMotion, SplitText } from '@/motion/gsap'
import { useScene } from '@/motion/useScene'
import filmCover from '@/assets/img/film-cover.webp'
import MarkRing from '@/components/art/MarkRing.vue'
import HeroLogo from './HeroLogo.vue'
import HeroSky from './HeroSky.vue'
import MigrateNote from './MigrateNote.vue'

const emit = defineEmits<{ film: [] }>()
const { t } = useI18n()
const stats = useStats()
const pulse = useDownloadPulse()

const root = ref<HTMLElement | null>(null)
const logo = ref<InstanceType<typeof HeroLogo> | null>(null)
const ring = ref<InstanceType<typeof MarkRing> | null>(null)
const markEl = ref<HTMLElement | null>(null)
const copyEl = ref<HTMLElement | null>(null)
const title = ref<HTMLElement | null>(null)
const sky = useScene(root, { still: 3, autostart: false })

function openFilm(e: MouseEvent) {
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return
  e.preventDefault()
  emit('film')
}

// ── Intro ────────────────────────────────────────────────────────────────
let intro: gsap.core.Timeline | null = null
let split: SplitText | null = null

onMounted(async () => {
  const el = root.value
  if (!el || !title.value) return
  const rise = el.querySelectorAll('[data-intro]')
  if (reducedMotion()) {
    sky.play()
    return
  }
  split = SplitText.create(title.value, { type: 'chars', charsClass: 'ch' })
  title.value.classList.add('is-split')
  gsap.set(rise, { autoAlpha: 0, y: 18 })
  gsap.set(split.chars, { autoAlpha: 0, yPercent: 35, filter: 'blur(10px)' })
  const logoIntro = logo.value?.intro()
  const ringIntro = ring.value?.intro()
  await Promise.race([document.fonts.ready, new Promise((resolve) => setTimeout(resolve, 700))])
  intro = gsap.timeline({ defaults: { ease: EASE.text, duration: 0.9 } })
  if (logoIntro) intro.add(logoIntro.paused(false), 0)
  if (ringIntro) intro.add(ringIntro.paused(false), 0.45)
  intro
    .to(split.chars, { autoAlpha: 1, yPercent: 0, filter: 'blur(0px)', stagger: 0.045, clearProps: 'filter' }, 0.55)
    .to(rise, { autoAlpha: 1, y: 0, stagger: 0.07, clearProps: 'transform' }, 0.95)
  sky.play()
})

onBeforeUnmount(() => {
  intro?.kill()
  split?.revert()
})
</script>

<template>
  <section id="hero" ref="root" class="hero">
    <div class="hero-sky" aria-hidden="true" />
    <div class="hero-grid" aria-hidden="true" />
    <HeroSky :time="sky.time.value" :mark="markEl" :copy="copyEl" />
    <div class="hero-body wrap">
      <div ref="markEl" class="hero-mark">
        <MarkRing ref="ring"><HeroLogo ref="logo" /></MarkRing>
      </div>
      <div ref="copyEl" class="hero-copy">
        <MigrateNote class="hero-migrate" data-intro />
        <h1 ref="title" class="hero-title">Rayburst</h1>
        <p class="hero-tagline" data-intro>{{ t('hero.tagline') }}</p>
        <p class="hero-desc" data-intro>{{ t('hero.description') }}</p>
        <div class="hero-actions" data-intro>
          <a class="btn btn-sponsor btn-lg" :href="LINKS.sponsor" target="_blank" rel="noopener">
            <AppIcon name="heart" class="heart" />
            <span>{{ t('hero.sponsor') }}</span>
          </a>
          <a class="btn btn-primary btn-lg" href="#download" @click="pulse.request()">
            <AppIcon name="download-outline" />
            <span>{{ t('hero.download') }}</span>
          </a>
          <a class="btn btn-glass btn-lg" :href="LINKS.github" target="_blank" rel="noopener">
            <AppIcon name="logo-github" />
            <span>{{ t('hero.github') }}</span>
          </a>
        </div>
        <div class="hero-foot" data-intro>
          <a class="film-card" :href="LINKS.film" @click="openFilm">
            <span class="film-thumb" aria-hidden="true">
              <img :src="filmCover" alt="" width="384" height="216" />
              <i class="film-thumb-play"><AppIcon name="play" /></i>
            </span>
            <span class="film-text">
              <b>{{ t('film.watch') }}</b>
              <small class="mono film-meta"><bdi>1:06</bdi> · {{ t('film.language') }}</small>
            </span>
          </a>
          <div class="hero-meta">
            <a class="stat" :href="LINKS.stars" target="_blank" rel="noopener">
              <AppIcon name="star-outline" /><b>{{ stats.stars.value }}</b
              ><span>{{ t('hero.stars') }}</span>
            </a>
            <i class="stat-sep" />
            <a class="stat" :href="LINKS.releases" target="_blank" rel="noopener">
              <AppIcon name="download-outline" /><b>{{ stats.downloads.value }}</b
              ><span>{{ t('hero.downloads') }}</span>
            </a>
            <i class="stat-sep" />
            <a class="stat" :href="LINKS.latest" target="_blank" rel="noopener">
              <AppIcon name="pricetag-outline" /><b>{{ stats.version.value }}</b
              ><span>{{ t('hero.version') }}</span>
            </a>
          </div>
        </div>
      </div>
    </div>
    <a class="hero-scroll" href="#overview" :aria-label="t('nav.features')" data-intro>
      <AppIcon name="chevron-down-outline" />
    </a>
  </section>
</template>

<style scoped>
.hero {
  position: relative;
  display: flex;
  flex-direction: column;
  justify-content: center;
  min-height: 100svh;
  padding-top: calc(var(--nav-h) + clamp(32px, 6vh, 64px));
  padding-bottom: clamp(120px, 16vh, 168px);
  isolation: isolate;
  overflow: clip;
}
/* The stage: a violet room, deepest behind the ring, with magenta and indigo
   glows in the far corners (the light theme takes the posters' field). It
   fades into the page below. */
.hero-sky {
  position: absolute;
  inset: 0;
  z-index: -3;
  background:
    radial-gradient(34% 42% at 88% 0%, rgba(162, 28, 175, 0.22), transparent 70%),
    radial-gradient(40% 50% at 96% 100%, rgba(55, 48, 163, 0.34), transparent 70%),
    radial-gradient(40% 60% at 22% 50%, rgba(91, 33, 182, 0.4), transparent 70%),
    linear-gradient(112deg, #150a2c 0%, #0e0720 46%, #09050f 100%);
  -webkit-mask-image: linear-gradient(180deg, #000 calc(100% - 200px), transparent);
  mask-image: linear-gradient(180deg, #000 calc(100% - 200px), transparent);
  animation: sky-in 1.6s 0.1s var(--ease-text) both;
}
:root[data-theme='light'] .hero-sky {
  background:
    radial-gradient(34% 42% at 88% 0%, rgba(255, 150, 196, 0.3), transparent 70%),
    radial-gradient(40% 50% at 96% 100%, rgba(138, 162, 255, 0.36), transparent 70%),
    radial-gradient(40% 60% at 22% 50%, rgba(177, 124, 236, 0.42), transparent 70%),
    linear-gradient(115deg, #fcf9ff 0%, #f3ebfc 48%, #e8dcf8 100%);
}
.hero-grid {
  position: absolute;
  inset: 0;
  z-index: -2;
  background-image: radial-gradient(circle, rgba(201, 184, 255, 0.75) 1px, transparent 1.4px);
  background-size: 26px 26px;
  -webkit-mask-image: radial-gradient(60% 70% at 70% 45%, rgba(0, 0, 0, 0.7), transparent);
  mask-image: radial-gradient(60% 70% at 70% 45%, rgba(0, 0, 0, 0.7), transparent);
}
:root[data-theme='light'] .hero-grid {
  background-image: radial-gradient(circle, rgba(124, 58, 237, 0.5) 0.9px, transparent 1.3px);
  background-size: 22px 22px;
}
.hero :deep(.hero-fx) {
  -webkit-mask-image: linear-gradient(180deg, #000 calc(100% - 96px), transparent);
  mask-image: linear-gradient(180deg, #000 calc(100% - 96px), transparent);
}
@keyframes sky-in {
  from {
    opacity: 0;
  }
}

/* The mark in its ring, the copy beside it. */
.hero-body {
  display: grid;
  grid-template-columns: minmax(0, 5fr) minmax(0, 7fr);
  align-items: center;
  gap: clamp(64px, 8vw, 128px);
}
.hero-mark {
  justify-self: center;
  width: clamp(220px, 27vw, 380px);
}
.hero-copy {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  text-align: start;
}
.hero-migrate {
  margin-bottom: clamp(22px, 4vh, 36px);
}
.hero-title {
  margin: 0;
  margin-inline-start: -0.04em;
  /* The split letters are boxes; keep them in reading order on right-to-left pages. */
  direction: ltr;
  unicode-bidi: isolate;
  font-family: var(--display);
  font-size: clamp(64px, 9.4vw, 136px);
  line-height: 0.95;
  font-weight: 720;
  font-stretch: 112%;
  letter-spacing: -0.03em;
  padding-bottom: 0.08em;
}
.hero-title:not(.is-split) {
  background: var(--sheen);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
/* Each letter carries the gradient itself once split: clipping text to a
   parent's background does not survive transformed children. */
.hero-title :deep(.ch) {
  background: var(--sheen);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
  padding-bottom: 0.08em;
}
.hero-tagline {
  margin: 14px 0 0;
  font-family: var(--display);
  font-size: clamp(21px, 2.3vw, 32px);
  line-height: 1.25;
  font-weight: 520;
  letter-spacing: -0.01em;
  color: var(--slogan);
  text-wrap: balance;
}
.hero-desc {
  margin: 14px 0 0;
  max-width: 540px;
  color: var(--text-2);
  font-size: clamp(16px, 1.4vw, 18px);
  text-wrap: pretty;
}
.hero-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 32px;
}
.hero-foot {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 14px 28px;
  margin-top: 22px;
  margin-inline-start: -6px;
}
.hero-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px 20px;
  color: var(--text-3);
  font-size: 14px;
}
.stat {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  transition: color 0.2s;
}
.stat:hover {
  color: var(--text);
}
.stat .ic {
  font-size: 15px;
}
.stat b {
  color: var(--text);
  font-family: var(--mono);
  font-weight: 600;
  font-size: 14px;
  font-variant-numeric: tabular-nums;
}
.stat-sep {
  width: 3px;
  height: 3px;
  border-radius: 50%;
  background: var(--text-3);
  opacity: 0.6;
}

/* Film link: local cover, official player on demand. */
.film-card {
  display: inline-flex;
  align-items: center;
  gap: 14px;
  padding: 6px 20px 6px 6px;
  border-radius: 16px;
  color: var(--text);
  text-align: start;
  transition: background-color 0.3s;
}
.film-card:hover {
  background: color-mix(in srgb, var(--text) 5%, transparent);
}
.film-thumb {
  position: relative;
  display: grid;
  place-items: center;
  width: 96px;
  aspect-ratio: 16 / 9;
  border-radius: 10px;
  overflow: hidden;
  background: linear-gradient(140deg, #22163a, #0b0914);
  box-shadow:
    0 0 0 1px rgba(255, 255, 255, 0.12),
    0 10px 26px -10px rgba(123, 62, 209, 0.75);
  transition: transform 0.45s var(--ease-spring);
}
.film-thumb > img {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.film-thumb-play {
  position: relative;
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.92);
  color: #5a2da8;
  font-size: 11px;
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35);
  transition: transform 0.45s var(--ease-spring);
}
.film-thumb-play .ic {
  margin-inline-start: 2px;
}
.film-card:hover .film-thumb {
  transform: scale(1.04);
}
.film-card:hover .film-thumb-play {
  transform: scale(1.15);
}
.film-text {
  display: grid;
  gap: 2px;
  line-height: 1.25;
}
.film-text b {
  font-size: 15px;
  font-weight: 600;
}
.film-meta {
  color: var(--text-3);
  font-size: 12px;
}

/* A quiet hint that the page continues. */
.hero-scroll {
  position: absolute;
  left: 50%;
  bottom: 22px;
  display: grid;
  place-items: center;
  width: 36px;
  height: 36px;
  margin-left: -18px;
  border-radius: 50%;
  color: var(--text-3);
  font-size: 20px;
  transition: color 0.2s;
}
.hero-scroll:hover {
  color: var(--text);
}
.hero-scroll .ic {
  animation: nudge 2.4s var(--ease-enter) infinite;
}
@keyframes nudge {
  0%,
  60%,
  100% {
    transform: none;
  }
  30% {
    transform: translateY(5px);
  }
}
@media (max-height: 760px) {
  .hero-scroll {
    display: none;
  }
}
@media (max-width: 960px) {
  .hero-body {
    grid-template-columns: 1fr;
    gap: clamp(56px, 12vw, 80px);
  }
  .hero-mark {
    width: clamp(150px, 36vw, 220px);
    margin-top: clamp(24px, 6vw, 48px);
  }
  .hero-copy,
  .hero-actions,
  .hero-foot,
  .hero-meta {
    align-items: center;
    justify-content: center;
    text-align: center;
  }
  .hero-foot {
    flex-direction: column;
    margin-inline-start: 0;
  }
}
@media (max-width: 560px) {
  .hero-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    width: 100%;
    max-width: 400px;
  }
  .hero-actions .btn-primary {
    grid-column: 1 / -1;
    order: -1;
  }
  .hero-actions .btn {
    padding-inline: 16px;
  }
}
</style>
