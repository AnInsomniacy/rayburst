<script setup lang="ts">
/**
 * @fileoverview Says when the system will not show Rayburst's notifications,
 * and sends a test one. A blocked notification is otherwise accepted without
 * a word, and downloads finish silently while the window is closed (#742).
 */
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { useEventListener } from '@vueuse/core'
import { NAlert, NButton, NFormItem } from 'naive-ui'
import { useAppMessage } from '@/composables/useAppMessage'
import { usePlatform } from '@/composables/usePlatform'
import { getErrorMessage } from '@shared/utils/errorMessage'
import { logger } from '@shared/logger'

/** `NotificationAccess` in services/notification.rs. */
type NotificationAccess =
  | 'allowed'
  | 'blocked-for-app'
  | 'blocked-for-user'
  | 'blocked-by-policy'
  | 'unavailable'
  | 'unknown'

const { t } = useI18n()
const message = useAppMessage()
const { isLinux } = usePlatform()
const access = ref<NotificationAccess>('unknown')
const testing = ref(false)

const blocked = computed(() => access.value.startsWith('blocked-'))

async function refresh() {
  access.value = await invoke<NotificationAccess>('notification_access_state').catch((error: unknown) => {
    logger.debug('NotificationAccess.refresh', error)
    return 'unknown' as const
  })
}

async function sendTest() {
  testing.value = true
  try {
    await invoke('send_app_system_notification', { title: 'Rayburst', body: t('preferences.notification-test-body') })
  } catch (error) {
    message.error(getErrorMessage(error))
  } finally {
    testing.value = false
    void refresh()
  }
}

function openSettings() {
  invoke('open_notification_settings').catch((error: unknown) => message.error(getErrorMessage(error)))
}

onMounted(refresh)
// Back from the system's settings, the answer may have changed.
useEventListener(window, 'focus', () => void refresh())
</script>

<template>
  <NFormItem label=" ">
    <div class="notification-access">
      <NAlert v-if="blocked || access === 'unavailable'" type="warning" :show-icon="false">
        <div class="notification-access__message">
          {{ blocked ? t('preferences.notification-blocked') : t('preferences.notification-unavailable') }}
          <NButton v-if="blocked && !isLinux" size="small" @click="openSettings">
            {{ t('preferences.notification-open-settings') }}
          </NButton>
        </div>
      </NAlert>
      <NButton size="small" :loading="testing" @click="sendTest">{{ t('preferences.notification-test') }}</NButton>
    </div>
  </NFormItem>
</template>

<style scoped>
.notification-access,
.notification-access__message {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;
}
</style>
