<script setup lang="ts">
/** @fileoverview File and URL defaults, verified by the operating system. */
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useI18n } from 'vue-i18n'
import { NButton, NDivider, NFormItem, NTooltip, NSkeleton, NInput, NInputGroup, NIcon } from 'naive-ui'
import { useProtocolHandlers, type ProtocolKey } from '@/composables/useProtocolHandlers'
import { usePlatform } from '@/composables/usePlatform'
import { useAppMessage } from '@/composables/useAppMessage'
import { CopyOutline, FolderOpenOutline } from '@vicons/ionicons5'
import { invoke } from '@tauri-apps/api/core'
import { writeAppClipboardText } from '@shared/utils'
import { logger } from '@shared/logger'

const { t } = useI18n()
const { isWindows } = usePlatform()
const message = useAppMessage()
const { status, pending, busy, refreshAll, setDefault } = useProtocolHandlers()
const options = computed(() => [
  { key: 'rayburst' as const, label: 'Rayburst [rayburst://]' },
  { key: '.torrent' as const, label: 'BitTorrent [.torrent]' },
  { key: 'magnet' as const, label: t('preferences.protocol-magnet') },
  { key: 'ed2k' as const, label: t('preferences.protocol-ed2k') },
  { key: 'thunder' as const, label: t('preferences.protocol-thunder') },
])
const labels = computed(() => ({
  current: t('preferences.association-current'),
  other: t('preferences.association-other'),
  unassigned: t('preferences.association-unassigned'),
  unavailable: t('preferences.association-unavailable'),
  error: t('preferences.association-error'),
}))

const developmentMode = computed(() => Object.values(status.value).some((value) => value?.canChange === false))
function isPath(value: string | null | undefined) {
  return !!value && (value.startsWith('/') || /^[a-z]:[\\/]/i.test(value))
}
async function copyHandler(protocol: ProtocolKey, label: string) {
  const value = status.value[protocol]?.handler
  if (!value) return
  try {
    await writeAppClipboardText(value)
    message.success(t('preferences.copied-to-clipboard', { label }))
  } catch (error) {
    logger.warn('Protocol.clipboard', String(error))
  }
}
async function revealHandler(protocol: ProtocolKey) {
  const path = status.value[protocol]?.handler
  if (!isPath(path)) return
  try {
    await invoke('show_item_in_dir', { path })
  } catch (error) {
    logger.warn('Protocol.reveal', String(error))
    message.warning(t('task.file-not-exist'))
  }
}

const needsSettings = ref(new Set<ProtocolKey>())
watch(
  status,
  (snapshot) => {
    for (const protocol of needsSettings.value) {
      if (snapshot[protocol]?.state === 'current') needsSettings.value.delete(protocol)
    }
  },
  { deep: true },
)
async function openSettings() {
  try {
    await invoke('open_default_apps_settings')
  } catch (error) {
    logger.warn('Protocol.settings', String(error))
    message.error(String(error))
  }
}
async function change(protocol: ProtocolKey) {
  const result = await setDefault(protocol)
  if (result.kind === 'success') needsSettings.value.delete(protocol)
  else if (result.kind !== 'cancelled' && result.kind !== 'ignored') needsSettings.value.add(protocol)
  switch (result.kind) {
    case 'success':
      message.success(t('preferences.protocol-registered', { protocol }))
      break
    case 'manual':
      message.info(t('preferences.protocol-manual-required'))
      break
    case 'unchanged':
      message.warning(t('preferences.protocol-unchanged', { protocol }))
      break
    case 'query-failed':
      message.error(t('preferences.protocol-query-failed', { protocol }))
      break
    case 'failed':
      message.error(t('preferences.protocol-register-failed', { protocol, reason: result.reason }))
      break
  }
}
let disposed = false
let unlistenFocus: (() => void) | undefined
onMounted(async () => {
  try {
    const unlisten = await getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) void refreshAll()
    })
    if (disposed) unlisten()
    else unlistenFocus = unlisten
  } catch (error) {
    logger.warn('Protocol.focus', String(error))
  }
  if (!disposed) await refreshAll()
})
onUnmounted(() => {
  disposed = true
  unlistenFocus?.()
})
</script>

<template>
  <NDivider title-placement="left">{{ t('preferences.default-programs') }}</NDivider>
  <p v-if="developmentMode" class="association-development pref-section-note">
    {{ t('preferences.association-development') }}
  </p>
  <NFormItem v-for="option in options" :key="option.key" :label="option.label">
    <div class="association-row" :aria-busy="!status[option.key] || pending === option.key">
      <div class="association-actions">
        <NTooltip v-if="status[option.key]?.error" trigger="click">
          <template #trigger>
            <NButton size="small" quaternary :disabled="busy" @click="refreshAll">{{ t('app.retry') }}</NButton>
          </template>
          {{ status[option.key]?.error }}
        </NTooltip>
        <NButton
          size="small"
          secondary
          :type="status[option.key]?.state === 'current' ? 'default' : 'primary'"
          :loading="pending === option.key"
          :disabled="
            busy ||
            !status[option.key] ||
            status[option.key]?.canChange === false ||
            status[option.key]?.state === 'current'
          "
          :aria-label="`${option.label}: ${option.key === 'rayburst' ? t('preferences.association-repair') : t('preferences.association-set')}`"
          @click="change(option.key)"
        >
          {{ option.key === 'rayburst' ? t('preferences.association-repair') : t('preferences.association-set') }}
        </NButton>
        <NButton
          v-if="
            isWindows && !developmentMode && (needsSettings.has(option.key) || status[option.key]?.state === 'current')
          "
          size="small"
          secondary
          :disabled="busy"
          @click="openSettings"
          >{{ t('preferences.association-settings') }}</NButton
        >
      </div>
      <NInputGroup class="association-path">
        <NInput
          :value="status[option.key]?.handler ?? ''"
          readonly
          placeholder="—"
          class="pref-control-full"
          :aria-label="option.label"
        >
          <template #prefix>
            <span class="association-state" role="status" aria-live="polite">
              <Transition name="association" mode="out-in">
                <span
                  v-if="status[option.key]"
                  :key="status[option.key]?.state"
                  :class="['association-label', status[option.key]?.state]"
                >
                  {{ labels[status[option.key]!.state] }}
                </span>
                <NSkeleton v-else key="loading" text :width="60" />
              </Transition>
            </span>
          </template>
        </NInput>
        <NButton
          class="pref-icon-button"
          :disabled="!status[option.key]?.handler"
          :title="t('about.click-to-copy')"
          :aria-label="`${option.label}: ${t('about.click-to-copy')}`"
          @click="copyHandler(option.key, option.label)"
        >
          <template #icon
            ><NIcon :size="14"><CopyOutline /></NIcon
          ></template>
        </NButton>
        <NButton
          class="pref-icon-button"
          :disabled="!isPath(status[option.key]?.handler)"
          :title="t('task.show-in-folder')"
          :aria-label="`${option.label}: ${t('task.show-in-folder')}`"
          @click="revealHandler(option.key)"
        >
          <template #icon
            ><NIcon :size="14"><FolderOpenOutline /></NIcon
          ></template>
        </NButton>
      </NInputGroup>
    </div>
  </NFormItem>
</template>

<style scoped>
.association-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 12px;
  width: 100%;
  min-width: 0;
}
.association-path {
  flex: 1 1 260px;
  min-width: 0;
}
.association-state {
  font-size: var(--font-size-xs);
  padding-inline-end: 8px;
  color: var(--m3-on-surface-variant);
}
.association-development {
  margin: 0 0 16px;
  text-align: start;
  font-size: var(--font-size-xs);
  color: var(--m3-on-surface-variant);
}
.association-label.current {
  color: var(--m3-primary);
}
.association-label.error,
.association-label.unavailable {
  color: var(--m3-error);
}
.association-actions {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}
.association-enter-active,
.association-leave-active {
  transition: opacity 120ms ease;
}
.association-enter-from,
.association-leave-to {
  opacity: 0;
}
@media (prefers-reduced-motion: reduce) {
  .association-enter-active,
  .association-leave-active {
    transition: none;
  }
}
</style>
