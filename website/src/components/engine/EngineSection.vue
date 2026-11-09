<script setup lang="ts">
/**
 * @fileoverview Aria2 Next, on the ivory paper of its README banner: the icon
 * in a gold rosette, the name above the gold line that swells into an aria
 * (EnginePaper), and a terminal typing the engine's own quick-start commands
 * (aria2-next README) and a standard aria2 JSON-RPC call; a gold download
 * button for the visitor's system with every build in a menu, and a Docker
 * command that copies itself.
 */
import { useClipboard } from '@vueuse/core'
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from 'reka-ui'
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import AppIcon from '@/components/AppIcon.vue'
import { BUILDS, useEngineBuilds } from '@/composables/useEngineBuilds'
import { DOCKER_PULL, LINKS } from '@/links'
import { vReveal } from '@/motion/directives'
import { useScene } from '@/motion/useScene'
import { fileSize } from '@/sim/format'
import EnginePaper from './EnginePaper.vue'
import { typedScript, TYPED_BY } from './script'
import engineLogo from '@/assets/img/aria2-next.svg'

const { t } = useI18n()
const builds = useEngineBuilds()
const { copy, copied } = useClipboard({ legacy: true, copiedDuring: 1800 })

const section = ref<HTMLElement | null>(null)
const icon = ref<HTMLElement | null>(null)
const name = ref<HTMLElement | null>(null)
const { time } = useScene(section, { still: TYPED_BY })
const lines = computed(() => typedScript(time.value))
</script>

<template>
  <section id="engine" ref="section" class="engine">
    <EnginePaper :time="time" :section="section" :icon="icon" :name="name" />
    <div class="wrap engine-grid">
      <div class="engine-copy">
        <span v-reveal class="eyebrow">{{ t('engine.over') }}</span>
        <div class="engine-head">
          <img ref="icon" v-reveal.scale="1" class="engine-icon" :src="engineLogo" alt="" width="112" height="112" />
          <h2 ref="name" v-reveal="1" class="engine-title">Aria2 <em>Next</em></h2>
          <p v-reveal="2" class="engine-tag">{{ t('engine.tag') }}</p>
        </div>
        <i18n-t v-reveal="3" keypath="engine.description" tag="p" scope="global">
          <template #aria2Next>
            <a :href="LINKS.engine" target="_blank" rel="noopener">Aria2 Next</a>
          </template>
        </i18n-t>
        <p v-reveal="4">{{ t('engine.media') }}</p>
        <div v-reveal="5" class="chips">
          <span
            v-for="c in ['HTTP', 'HTTPS', 'SFTP', 'ED2K', 'BitTorrent', 'HLS', 'DASH', 'JSON-RPC']"
            :key="c"
            class="chip"
          >
            {{ c }}
          </span>
        </div>
        <div v-reveal="6" class="engine-get">
          <div class="split">
            <a class="split-main" :href="builds.url(builds.chosen.value)" target="_blank" rel="noopener">
              <AppIcon name="download-outline" />
              <span class="btn-stack">
                <b>{{ builds.chosen.value ? t('hero.download') : t('engine.getAny') }}</b>
                <small class="mono">{{
                  builds.chosen.value ? `${builds.chosen.value.os} · ${builds.chosen.value.label}` : ''
                }}</small>
              </span>
            </a>
            <DropdownMenuRoot :modal="false">
              <DropdownMenuTrigger class="split-more" :aria-label="t('engine.more')">
                <AppIcon name="chevron-down-outline" />
              </DropdownMenuTrigger>
              <DropdownMenuPortal>
                <DropdownMenuContent class="menu builds" side="top" align="end" :side-offset="10">
                  <DropdownMenuItem v-for="b in BUILDS" :key="`${b.os}-${b.arch}`" as-child>
                    <a
                      class="menu-item build"
                      :class="{ 'is-active': b === builds.chosen.value }"
                      :href="builds.url(b)"
                      target="_blank"
                      rel="noopener"
                    >
                      <AppIcon :name="b.icon" />
                      <span>{{ b.os }}</span>
                      <small>{{ b.label }}</small>
                      <em class="mono">{{ fileSize(builds.size(b)) }}</em>
                    </a>
                  </DropdownMenuItem>
                  <DropdownMenuSeparator class="menu-sep" />
                  <DropdownMenuItem as-child>
                    <a
                      class="menu-item build"
                      :href="builds.checksums.value || LINKS.engineLatest"
                      target="_blank"
                      rel="noopener"
                    >
                      <AppIcon name="shield-checkmark-outline" />
                      <span>{{ t('engine.checksums') }}</span>
                      <small>SHA-256</small>
                    </a>
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenuPortal>
            </DropdownMenuRoot>
          </div>
          <button class="cmd" :class="{ 'is-copied': copied }" type="button" @click="copy(DOCKER_PULL)">
            <span class="cmd-tag"><AppIcon name="logo-docker" />Docker</span>
            <code>docker pull ghcr.io/aninsomniacy/aria2-next</code>
            <span class="cmd-copy" :title="t('engine.copy')">
              <AppIcon name="copy-outline" class="ic-copy" />
              <AppIcon name="checkmark-outline" class="ic-done" />
            </span>
            <span class="cmd-toast" aria-live="polite">{{ copied ? t('engine.copied') : '' }}</span>
          </button>
          <a class="link link-gold" :href="LINKS.engine" target="_blank" rel="noopener">
            <AppIcon name="logo-github" /><span>{{ t('engine.source') }}</span
            ><AppIcon name="arrow-forward-outline" />
          </a>
        </div>
      </div>
      <div v-reveal.scale="2" class="term">
        <div class="term-bar"><i /><i /><i /><span>aria2-next</span></div>
        <pre aria-label="aria2-next"><template v-for="(line, i) in lines" :key="i"><span
          v-for="(part, j) in line.parts" :key="j" :class="part.cls">{{ part.text }}</span><span
          v-if="line.caret" class="caret" />{{ '\n' }}</template></pre>
      </div>
    </div>
  </section>
</template>

<style scoped src="./engine.css"></style>

<style>
/* The builds menu renders in a portal, outside the band. */
.menu.builds {
  --accent: var(--gold);
  min-width: 290px;
}
.build {
  display: grid;
  grid-template-columns: 18px 1fr auto;
  grid-template-rows: auto auto;
  column-gap: 12px;
}
.build .ic {
  grid-row: span 2;
  align-self: center;
}
.build span {
  color: var(--text);
  font-weight: 600;
}
.build small {
  grid-row: 2;
  grid-column: 2;
  color: var(--text-3);
  font-size: 12px;
}
.build em {
  grid-row: span 2;
  grid-column: 3;
  align-self: center;
  color: var(--text-3);
  font-size: 12px;
  font-style: normal;
}
</style>
