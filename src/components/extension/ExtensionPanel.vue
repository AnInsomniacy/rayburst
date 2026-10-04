<script setup lang="ts">
/** Extension promotion using the same centered modal presentation as About. */
import { ref, useId } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { NButton, NIcon, NModal } from 'naive-ui'
import { CloseOutline, LogoChrome, LogoEdge, LogoFirefox } from '@vicons/ionicons5'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useAppMessage } from '@/composables/useAppMessage'
import { getErrorMessage } from '@shared/utils/errorMessage'
import preview from '@/assets/extension-preview.webp'

defineProps<{ show: boolean }>()
const emit = defineEmits<{ close: [] }>()
const brandLogo = '/logo.svg'
const { t } = useI18n()
const router = useRouter()
const message = useAppMessage()
const panelId = useId()
const imageLoaded = ref(false)
const imageFailed = ref(false)

const stores = [
  {
    name: 'Chrome',
    icon: LogoChrome,
    url: 'https://chromewebstore.google.com/detail/ofeajdebdjajhkmcmamagokecnbephhl',
  },
  {
    name: 'Edge',
    icon: LogoEdge,
    url: 'https://microsoftedge.microsoft.com/addons/detail/loojjolhejmakcdlbidigoniobfanjlb',
  },
  { name: 'Firefox', icon: LogoFirefox, url: 'https://addons.mozilla.org/firefox/addon/rayburst-connect/' },
]

async function install(url: string) {
  try {
    await openUrl(url)
  } catch (error) {
    message.error(getErrorMessage(error))
  }
}

async function openSettings() {
  emit('close')
  try {
    await router.push({ name: 'preference-advanced' })
  } catch (error) {
    message.error(getErrorMessage(error))
  }
}
</script>

<template>
  <NModal :show="show" transform-origin="center" @update:show="(visible: boolean) => !visible && emit('close')">
    <div
      class="extension-panel"
      role="dialog"
      aria-modal="true"
      :aria-labelledby="`${panelId}-title`"
      :aria-describedby="`${panelId}-description`"
    >
      <button type="button" class="extension-close" tabindex="0" :aria-label="t('app.close')" @click="emit('close')">
        <NIcon :size="18" :component="CloseOutline" aria-hidden="true" />
      </button>
      <header class="extension-heading">
        <img :src="brandLogo" width="40" height="40" alt="" />
        <h2 :id="`${panelId}-title`">Rayburst Connect</h2>
      </header>
      <p :id="`${panelId}-description`" class="extension-description">{{ t('extension.description') }}</p>

      <div v-if="!imageFailed" class="extension-artwork" aria-hidden="true">
        <img
          :src="preview"
          width="1280"
          height="720"
          alt=""
          decoding="async"
          :class="{ loaded: imageLoaded }"
          @load="imageLoaded = true"
          @error="imageFailed = true"
        />
      </div>

      <div class="extension-install">
        <p>{{ t('extension.install') }}</p>
        <div class="extension-stores">
          <a v-for="store in stores" :key="store.name" :href="store.url" @click.prevent="install(store.url)">
            <NIcon :size="22" :component="store.icon" aria-hidden="true" />
            {{ store.name }}
          </a>
        </div>
      </div>
      <NButton class="extension-settings" text @click="openSettings">
        {{ t('extension.connection-settings') }}
      </NButton>
    </div>
  </NModal>
</template>

<style scoped>
.extension-panel {
  position: relative;
  box-sizing: border-box;
  width: min(640px, calc(100vw - 32px));
  max-height: calc(100dvh - 48px);
  overflow-y: auto;
  padding: 32px 28px 24px;
  text-align: center;
  color: var(--m3-on-surface);
  border-radius: 16px;
  border: 1px solid var(--m3-outline-variant);
  background: color-mix(in srgb, var(--m3-surface-container-high) 96%, transparent);
  backdrop-filter: blur(24px) saturate(1.4);
  -webkit-backdrop-filter: blur(24px) saturate(1.4);
  box-shadow:
    0 8px 32px var(--m3-shadow),
    0 0 0 1px color-mix(in srgb, var(--m3-on-surface) 8%, transparent);
}
.extension-close {
  position: absolute;
  top: 12px;
  right: 12px;
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--m3-on-surface-variant);
  cursor: pointer;
  transition:
    background-color var(--task-motion-enter) var(--task-motion-ease),
    color var(--task-motion-enter) var(--task-motion-ease);
}
.extension-close:hover {
  background: var(--m3-surface-container-highest);
  color: var(--m3-on-surface);
}
.extension-heading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding-inline: 12px;
}
.extension-heading img {
  flex-shrink: 0;
}
.extension-heading h2 {
  margin: 0;
  font-size: clamp(20px, 3vw, 24px);
  font-weight: 700;
  line-height: 1.4;
}
.extension-description {
  max-width: 480px;
  margin: 12px auto 20px;
  color: var(--m3-on-surface-variant);
  line-height: 1.7;
}
.extension-artwork {
  aspect-ratio: 16 / 9;
  overflow: hidden;
  border-radius: 10px;
  background: var(--m3-surface-container);
}
.extension-artwork img {
  display: block;
  width: 100%;
  height: auto;
  opacity: 0;
  transition: opacity var(--task-motion-enter) var(--task-motion-ease);
}
.extension-artwork img.loaded {
  opacity: 1;
}
.extension-install {
  margin-top: 20px;
}
.extension-install p {
  margin: 0 0 10px;
  font-size: var(--font-size-sm);
  color: var(--m3-on-surface-variant);
}
.extension-stores {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}
.extension-stores a {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-wrap: wrap;
  gap: 8px;
  min-height: 44px;
  padding: 8px;
  border: 1px solid var(--m3-outline-variant);
  border-radius: 10px;
  background: var(--about-card-bg);
  color: var(--m3-on-surface);
  font-weight: 500;
  text-decoration: none;
  transition:
    background-color var(--task-motion-enter) var(--task-motion-ease),
    color var(--task-motion-enter) var(--task-motion-ease),
    border-color var(--task-motion-enter) var(--task-motion-ease);
}
.extension-stores a:hover {
  border-color: var(--m3-primary);
  color: var(--m3-primary);
  background: var(--about-card-hover-bg);
}
.extension-stores a:focus-visible,
.extension-close:focus-visible {
  outline: 2px solid var(--m3-primary);
  outline-offset: 2px;
}
.extension-settings {
  margin-top: 14px;
  font-size: var(--font-size-sm);
}
@media (prefers-reduced-motion: reduce) {
  .extension-panel,
  .extension-panel * {
    transition-duration: 1ms !important;
    animation: none !important;
  }
}
</style>
