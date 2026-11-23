<script setup lang="ts">
/** Browser-to-desktop campaign with direct store and package downloads. */
import { ref, useId } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { usePanelEntrance } from '@/composables/usePanelEntrance'
import { NButton, NIcon, NModal } from 'naive-ui'
import {
  CloseOutline,
  DownloadOutline,
  LogoChrome,
  LogoEdge,
  LogoFirefox,
  OpenOutline,
  PlayCircleOutline,
  SettingsOutline,
  SpeedometerOutline,
} from '@vicons/ionicons5'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useAppMessage } from '@/composables/useAppMessage'
import { getErrorMessage } from '@shared/utils/errorMessage'
// Optimized from rayburst-connect/docs/brand/banner.png, without cropping the artwork.
import banner from '@/assets/extension-banner.webp'

const props = defineProps<{ show: boolean }>()
const emit = defineEmits<{ close: [] }>()
const brandLogo = '/logo.svg'
const { t } = useI18n()
const router = useRouter()
const message = useAppMessage()
const panelId = useId()
const entering = usePanelEntrance(() => props.show)
const imageLoaded = ref(false)
const imageFailed = ref(false)
const packagesUrl = 'https://github.com/AnInsomniacy/rayburst-connect/releases/latest'

const stores = [
  {
    name: 'Chrome',
    store: 'Chrome Web Store',
    icon: LogoChrome,
    url: 'https://chromewebstore.google.com/detail/ofeajdebdjajhkmcmamagokecnbephhl',
  },
  {
    name: 'Edge',
    store: 'Microsoft Edge Add-ons',
    icon: LogoEdge,
    url: 'https://microsoftedge.microsoft.com/addons/detail/loojjolhejmakcdlbidigoniobfanjlb',
  },
  {
    name: 'Firefox',
    store: 'Firefox Add-ons',
    icon: LogoFirefox,
    url: 'https://addons.mozilla.org/firefox/addon/rayburst-connect/',
  },
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
      :class="{ 'panel-entering': entering }"
      role="dialog"
      aria-modal="true"
      :aria-labelledby="`${panelId}-title`"
      :aria-describedby="`${panelId}-description`"
    >
      <button type="button" class="extension-close" tabindex="0" :aria-label="t('app.close')" @click="emit('close')">
        <NIcon :size="18" :component="CloseOutline" aria-hidden="true" />
      </button>
      <h2 :id="`${panelId}-title`" class="extension-accessible-title">Rayburst Connect</h2>
      <!-- The artwork is the panel's backdrop and arrives with it: fading it would show
           the panel's own two-tone background through it. -->
      <div class="extension-artwork" aria-hidden="true">
        <img
          v-if="!imageFailed"
          :src="banner"
          width="1600"
          height="400"
          alt=""
          decoding="async"
          :class="{ loaded: imageLoaded }"
          @load="imageLoaded = true"
          @error="imageFailed = true"
        />
        <div v-else class="extension-artwork-fallback">
          <img :src="brandLogo" width="48" height="48" alt="" />
          <span>Rayburst Connect</span>
        </div>
      </div>

      <div class="extension-body">
        <div class="extension-copy">
          <h3 class="entrance" style="--entrance: 1">{{ t('extension.install') }}</h3>
          <p :id="`${panelId}-description`" class="entrance" style="--entrance: 2">
            {{ t('extension.description') }}
          </p>
          <ul class="extension-features">
            <li class="entrance" style="--entrance: 3">
              <NIcon :size="18" :component="PlayCircleOutline" aria-hidden="true" />
              <span>{{ t('extension.media-preview') }}</span>
            </li>
            <li class="entrance" style="--entrance: 4">
              <NIcon :size="18" :component="SpeedometerOutline" aria-hidden="true" />
              <span>{{ t('extension.browser-controls') }}</span>
            </li>
          </ul>
        </div>
        <nav class="extension-stores" :aria-label="t('extension.install')">
          <a
            v-for="(store, index) in stores"
            :key="store.name"
            class="extension-store entrance"
            :style="{ '--entrance': index + 1 }"
            :href="store.url"
            @click.prevent="install(store.url)"
          >
            <span class="extension-store-icon"><NIcon :size="26" :component="store.icon" aria-hidden="true" /></span>
            <span class="extension-store-label">
              <strong>{{ t('extension.add-to-browser', { browser: store.name }) }}</strong>
              <span>{{ store.store }}</span>
            </span>
            <NIcon class="extension-store-arrow" :size="16" :component="OpenOutline" aria-hidden="true" />
          </a>
        </nav>
      </div>
      <footer class="extension-footer entrance entrance--fade" style="--entrance: 5">
        <a class="extension-packages" :href="packagesUrl" @click.prevent="install(packagesUrl)">
          <NIcon :size="16" :component="DownloadOutline" aria-hidden="true" />
          {{ t('extension.download-packages') }}
        </a>
        <NButton class="extension-settings" text @click="openSettings">
          <template #icon><NIcon :component="SettingsOutline" aria-hidden="true" /></template>
          {{ t('extension.connection-settings') }}
        </NButton>
      </footer>
    </div>
  </NModal>
</template>

<style scoped>
.extension-panel {
  position: relative;
  box-sizing: border-box;
  width: min(760px, calc(100vw - 32px));
  max-height: calc(100dvh - 48px);
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
  text-align: start;
  color: var(--m3-on-surface);
  /* The banner's deep violet; its edge continues into the panel's border. */
  --extension-stage: #1d0d3b;
  --extension-art-height: calc(min(760px, 100vw - 32px) / 4);
  border-radius: var(--radius-2xl);
  border: 1px solid transparent;
  /* Solid: a backdrop blur makes WebKitGTK flicker on some GPUs (#705).
     Border and background stay dark behind the banner's opaque top, so no
     light rim or corner seam frames it; the fade below still meets the panel. */
  background:
    linear-gradient(
        to bottom,
        var(--extension-stage) calc(var(--extension-art-height) * 0.5),
        var(--m3-surface-container-high) 0
      )
      padding-box,
    linear-gradient(
        to bottom,
        var(--extension-stage) calc(var(--extension-art-height) * 0.7),
        var(--m3-outline-variant) var(--extension-art-height)
      )
      border-box;
  box-shadow:
    0 8px 32px var(--m3-shadow),
    0 0 0 1px color-mix(in srgb, var(--m3-on-surface) 8%, transparent);
}
.extension-accessible-title {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
.extension-close {
  position: absolute;
  top: 14px;
  right: 14px;
  z-index: 1;
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  border: 0;
  border-radius: 50%;
  /* A light disc stands out on the dark banner it always sits on. */
  background: rgb(255 255 255 / 0.8);
  color: var(--extension-stage);
  box-shadow: 0 2px 10px rgb(0 0 0 / 0.4);
  cursor: pointer;
  transition:
    background-color var(--task-motion-enter) var(--task-motion-ease),
    color var(--task-motion-enter) var(--task-motion-ease);
}
.extension-close:hover {
  background: #fff;
  color: var(--extension-stage);
}
.extension-artwork {
  aspect-ratio: 4 / 1;
  overflow: hidden;
  border-radius: calc(var(--radius-2xl) - 1px) calc(var(--radius-2xl) - 1px) 0 0;
  background: var(--extension-stage);
  /* The dark stage dissolves into the panel instead of ending on a hard edge. */
  -webkit-mask-image: linear-gradient(to bottom, #000 74%, rgb(0 0 0 / 0.55) 88%, transparent);
  -webkit-mask-image: linear-gradient(to bottom, #000 74%, rgb(0 0 0 / 0.55) 88%, transparent);
  mask-image: linear-gradient(to bottom, #000 74%, rgb(0 0 0 / 0.55) 88%, transparent);
}
.extension-artwork > img {
  display: block;
  width: 100%;
  height: auto;
  opacity: 0;
  transition: opacity var(--task-motion-enter) var(--task-motion-ease);
}
.extension-artwork > img.loaded {
  opacity: 1;
}
.extension-artwork-fallback {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  height: 100%;
  font-size: var(--font-size-3xl);
  font-weight: 700;
  color: #f4ecff;
}
.extension-body {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.1fr);
  align-items: center;
  gap: 28px;
  padding: 24px 28px;
}
.extension-copy h3 {
  margin: 0;
  font-size: var(--font-size-3xl);
  font-weight: 650;
  line-height: 1.25;
  letter-spacing: -0.4px;
  text-wrap: balance;
}
.extension-copy p {
  margin: 10px 0 18px;
  color: var(--m3-on-surface-variant);
  line-height: 1.65;
}
.extension-features {
  display: grid;
  gap: 10px;
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: var(--font-size-sm);
}
.extension-features li {
  display: flex;
  align-items: center;
  gap: 8px;
  line-height: 1.5;
}
.extension-features :deep(.n-icon) {
  flex-shrink: 0;
  color: var(--m3-primary);
}
.extension-stores {
  display: grid;
  gap: 8px;
}
.extension-store {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 60px;
  padding: 10px 14px;
  border: 1px solid var(--m3-outline-variant);
  border-radius: var(--radius-lg);
  background: var(--m3-surface-container-lowest);
  color: var(--m3-on-surface);
  text-decoration: none;
  transition:
    background-color var(--task-motion-enter) var(--task-motion-ease),
    color var(--task-motion-enter) var(--task-motion-ease),
    border-color var(--task-motion-enter) var(--task-motion-ease);
}
.extension-store:hover {
  border-color: var(--m3-primary);
  background: var(--m3-primary-container);
  color: var(--m3-on-primary-container);
}
.extension-store-icon {
  display: grid;
  place-items: center;
  flex-shrink: 0;
  width: 36px;
  height: 36px;
  color: var(--m3-primary);
}
.extension-store-label {
  display: grid;
  flex: 1;
  min-width: 0;
  gap: 2px;
}
.extension-store-label strong {
  font-weight: 600;
  line-height: 1.4;
}
.extension-store-label > span {
  font-size: var(--font-size-xs);
  color: var(--m3-on-surface-variant);
}
.extension-store-arrow {
  flex-shrink: 0;
  color: var(--m3-primary);
  transition: transform var(--task-motion-enter) var(--task-motion-ease);
}
.extension-store:hover .extension-store-arrow {
  transform: translate(1px, -1px);
}
.extension-store:focus-visible,
.extension-close:focus-visible,
.extension-packages:focus-visible {
  outline: 2px solid var(--m3-primary);
  outline-offset: 2px;
}
.extension-footer {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 14px 28px;
  border-top: 1px solid var(--m3-outline-variant);
  background: var(--m3-surface-container);
}
.extension-packages {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  color: var(--m3-primary);
  font-size: var(--font-size-sm);
  text-decoration: none;
}
.extension-packages:hover {
  text-decoration: underline;
  text-underline-offset: 3px;
}
.extension-settings {
  font-size: var(--font-size-sm);
}
@media (max-width: 599px) {
  .extension-body {
    grid-template-columns: 1fr;
    gap: 20px;
    padding: 20px;
  }
  .extension-copy h3 {
    font-size: var(--font-size-2xl);
  }
  .extension-copy p {
    margin-bottom: 12px;
  }
  .extension-footer {
    padding: 14px 20px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .extension-panel,
  .extension-panel * {
    transition-duration: 1ms !important;
    animation: none !important;
  }
}
</style>
