<script setup lang="ts">
/** @fileoverview The 404 page, in the visitor's language and theme. */
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTheme } from '@/composables/useTheme'
import { EASE, gsap, reducedMotion } from '@/motion/gsap'
import logo from '@/assets/img/logo.svg'
import MarkRing from '@/components/art/MarkRing.vue'

const { t } = useI18n()
useTheme()
const root = ref<HTMLElement | null>(null)

onMounted(() => {
  document.title = t('error.title')
  if (!root.value || reducedMotion()) return
  gsap.from(root.value.children, { autoAlpha: 0, y: 16, duration: 0.9, stagger: 0.1, ease: EASE.text })
})
</script>

<template>
  <main ref="root" class="nf">
    <MarkRing class="nf-mark"><img :src="logo" alt="" width="88" height="88" /></MarkRing>
    <h1>404</h1>
    <p>{{ t('error.description') }}</p>
    <a class="btn btn-primary btn-lg" href="/">Rayburst</a>
  </main>
</template>

<style scoped>
.nf {
  display: grid;
  place-content: center;
  justify-items: center;
  min-height: 100svh;
  padding: 24px;
  background: radial-gradient(50% 45% at 50% 30%, var(--glow-a), transparent 70%), var(--bg);
  text-align: center;
}
.nf-mark {
  width: 88px;
  margin: 24px 0 52px;
}
h1 {
  margin: 0;
  font-family: var(--display);
  font-size: clamp(56px, 12vw, 120px);
  line-height: 1;
  font-weight: 720;
  font-stretch: 112%;
  letter-spacing: -0.03em;
  background: var(--sheen);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
p {
  margin: 14px 0 32px;
  color: var(--text-2);
}
</style>
