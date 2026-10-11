<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { NPopover, NButton, NIcon, NEllipsis, NEmpty } from 'naive-ui'
import { TimeOutline } from '@vicons/ionicons5'
import { buildUserAgentSelectionItems } from '@shared/utils/userAgentPolicy'
import type { UserAgentProfile, UserAgentRule } from '@shared/types'

const props = withDefaults(
  defineProps<{
    url?: string
    finalUrl?: string
    referer?: string
    profiles: UserAgentProfile[]
    rules: UserAgentRule[]
    recentProfileIds: string[]
    disabled?: boolean
  }>(),
  {
    url: '',
    finalUrl: '',
    referer: '',
    disabled: false,
  },
)

const emit = defineEmits<{
  select: [profile: UserAgentProfile]
}>()

const { t } = useI18n()
const visible = ref(false)
const items = computed(() =>
  buildUserAgentSelectionItems({
    url: props.url,
    finalUrl: props.finalUrl,
    referer: props.referer,
    profiles: props.profiles,
    rules: props.rules,
    recentProfileIds: props.recentProfileIds,
  }),
)
const matchedItems = computed(() => items.value.filter((item) => item.section === 'matched'))
const recentItems = computed(() => items.value.filter((item) => item.section === 'recent'))
const savedItems = computed(() => items.value.filter((item) => item.section === 'saved'))
const hasItems = computed(() => items.value.length > 0)

function selectProfile(profile: UserAgentProfile) {
  emit('select', profile)
  visible.value = false
}
</script>

<template>
  <NPopover
    v-model:show="visible"
    trigger="click"
    placement="bottom-end"
    :width="320"
    content-class="ua-popover-content"
  >
    <template #trigger>
      <NButton :title="t('task.ua-history')" :disabled="disabled" class="ua-popover-trigger">
        <template #icon>
          <NIcon><TimeOutline /></NIcon>
        </template>
      </NButton>
    </template>

    <div v-if="hasItems" class="ua-popover">
      <template v-if="matchedItems.length > 0">
        <div class="ua-popover-heading">{{ t('task.ua-matched') }}</div>
        <div
          v-for="item in matchedItems"
          :key="'matched-' + item.profile.id"
          class="ua-popover-item"
          @click="selectProfile(item.profile)"
        >
          <NEllipsis class="ua-popover-label" :tooltip="false">{{ item.profile.name }}</NEllipsis>
        </div>
      </template>

      <template v-if="recentItems.length > 0">
        <div class="ua-popover-heading" :class="{ 'ua-popover-heading--spaced': matchedItems.length > 0 }">
          {{ t('task.ua-recent') }}
        </div>
        <div
          v-for="item in recentItems"
          :key="'recent-' + item.profile.id"
          class="ua-popover-item"
          @click="selectProfile(item.profile)"
        >
          <NEllipsis class="ua-popover-label" :tooltip="false">{{ item.profile.name }}</NEllipsis>
        </div>
      </template>

      <template v-if="savedItems.length > 0">
        <div
          class="ua-popover-heading"
          :class="{ 'ua-popover-heading--spaced': matchedItems.length > 0 || recentItems.length > 0 }"
        >
          {{ t('preferences.ua-saved') }}
        </div>
        <div
          v-for="item in savedItems"
          :key="'saved-' + item.profile.id"
          class="ua-popover-item"
          @click="selectProfile(item.profile)"
        >
          <NEllipsis class="ua-popover-label" :tooltip="false">{{ item.profile.name }}</NEllipsis>
        </div>
      </template>
    </div>
    <NEmpty v-else class="ua-popover-empty" size="small" :description="t('task.ua-no-saved')" />
  </NPopover>
</template>

<style scoped>
.ua-popover-trigger {
  height: auto;
  min-height: 34px;
  align-self: stretch;
}

.ua-popover {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.ua-popover-heading {
  padding: 4px 8px 2px;
  color: var(--n-text-color-3);
  font-size: var(--font-size-sm);
  font-weight: 600;
}
.ua-popover-heading--spaced {
  margin-top: 6px;
  padding-top: 8px;
  border-top: 1px solid var(--m3-outline-variant);
}
.ua-popover-item {
  display: flex;
  align-items: center;
  min-height: 32px;
  padding: 5px 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background-color 0.15s;
}
.ua-popover-item:hover {
  background: var(--m3-surface-container-high);
}
.ua-popover-label {
  min-width: 0;
}
</style>
