<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { NButton, NIcon, NPopover } from 'naive-ui'
import { ChevronDownOutline, OpenOutline } from '@vicons/ionicons5'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useAppMessage } from '@/composables/useAppMessage'
import { getErrorMessage } from '@shared/utils/errorMessage'

const { t } = useI18n()
const message = useAppMessage()
const show = ref(false)
const stores = [
  { name: 'Chrome', url: 'https://chromewebstore.google.com/detail/ofeajdebdjajhkmcmamagokecnbephhl' },
  { name: 'Microsoft Edge', url: 'https://microsoftedge.microsoft.com/addons/detail/loojjolhejmakcdlbidigoniobfanjlb' },
  { name: 'Firefox', url: 'https://addons.mozilla.org/firefox/addon/rayburst-connect/' },
]
async function install(url: string) {
  try {
    await openUrl(url)
    show.value = false
  } catch (error) {
    message.error(getErrorMessage(error))
  }
}
</script>

<template>
  <NPopover v-model:show="show" trigger="click" placement="bottom-start">
    <template #trigger>
      <NButton size="small" :aria-expanded="show">
        {{ t('preferences.install-extension') }}
        <template #icon
          ><NIcon><ChevronDownOutline /></NIcon
        ></template>
      </NButton>
    </template>
    <div class="extension-stores">
      <NButton v-for="store in stores" :key="store.name" quaternary @click="install(store.url)">
        {{ store.name }}
        <template #icon
          ><NIcon><OpenOutline /></NIcon
        ></template>
      </NButton>
    </div>
  </NPopover>
</template>

<style scoped>
.extension-stores {
  display: grid;
  gap: 4px;
  min-width: 160px;
}
.extension-stores :deep(.n-button__content) {
  flex: 1;
  justify-content: flex-start;
}
</style>
