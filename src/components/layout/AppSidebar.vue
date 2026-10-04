<script setup lang="ts">
/** Persistent task navigation and application destinations. */
import { computed } from 'vue'
import { RouterLink, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import MTooltip from '@/components/common/MTooltip.vue'
import { NIcon } from 'naive-ui'
import { ExtensionPuzzleOutline, InformationCircleOutline, SettingsOutline } from '@vicons/ionicons5'
import { useTaskStore } from '@/stores/task'
import { usePreferenceStore } from '@/stores/preference'
import SidebarCount from './SidebarCount.vue'
import { useTaskDestinations } from './navigation'

defineProps<{ compact?: boolean; extensionsOpen?: boolean }>()

const emit = defineEmits<{ 'show-about': []; 'show-extensions': [] }>()
const { t } = useI18n()
const route = useRoute()
const taskDestinations = useTaskDestinations()
const tasks = useTaskStore()
const preferences = usePreferenceStore()
const isSettings = computed(() => route.matched.some((record) => record.name === 'preference'))
</script>

<template>
  <nav
    class="sidebar"
    :class="{ compact, counts: preferences.config.sidebarTaskCounts }"
    :aria-label="t('app.task-list')"
  >
    <div class="sidebar-scopes">
      <MTooltip v-for="item in taskDestinations" :key="item.key" placement="right" :disabled="!compact">
        <template #trigger>
          <RouterLink
            :to="{ name: 'task', params: { status: item.key } }"
            class="sidebar-item"
            active-class="active"
            :aria-label="
              preferences.config.sidebarTaskCounts ? `${item.label} ${tasks.taskCounts[item.key]}` : item.label
            "
          >
            <NIcon :size="18" aria-hidden="true"><component :is="item.icon" /></NIcon>
            <span class="sidebar-label">{{ item.label }}</span>
            <Transition name="sidebar-count">
              <SidebarCount v-if="preferences.config.sidebarTaskCounts" :value="tasks.taskCounts[item.key]" />
            </Transition>
          </RouterLink>
        </template>
        {{ item.label }}
      </MTooltip>
    </div>
    <div class="sidebar-bottom">
      <MTooltip placement="right" :disabled="!compact">
        <template #trigger>
          <RouterLink
            :to="{ name: 'preference-general' }"
            class="sidebar-item"
            :class="{ active: isSettings }"
            :aria-label="t('navigation.settings')"
            :aria-current="isSettings ? 'page' : undefined"
          >
            <NIcon :size="18" aria-hidden="true"><SettingsOutline /></NIcon>
            <span class="sidebar-label">{{ t('navigation.settings') }}</span>
          </RouterLink>
        </template>
        {{ t('navigation.settings') }}
      </MTooltip>
      <MTooltip placement="right" :disabled="!compact || extensionsOpen">
        <template #trigger>
          <button
            type="button"
            class="sidebar-item"
            :class="{ active: extensionsOpen }"
            :aria-label="t('navigation.extensions')"
            :aria-expanded="!!extensionsOpen"
            aria-haspopup="dialog"
            @click="emit('show-extensions')"
          >
            <NIcon :size="18" aria-hidden="true"><ExtensionPuzzleOutline /></NIcon>
            <span class="sidebar-label">{{ t('navigation.extensions') }}</span>
          </button>
        </template>
        {{ t('navigation.extensions') }}
      </MTooltip>
      <MTooltip placement="right" :disabled="!compact">
        <template #trigger>
          <button type="button" class="sidebar-item" :aria-label="t('navigation.about')" @click="emit('show-about')">
            <NIcon :size="18" aria-hidden="true"><InformationCircleOutline /></NIcon>
            <span class="sidebar-label">{{ t('navigation.about') }}</span>
          </button>
        </template>
        {{ t('navigation.about') }}
      </MTooltip>
    </div>
  </nav>
</template>

<style scoped>
.sidebar {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 8px;
  gap: 16px;
  background: var(--sidebar-bg);
}
.sidebar-scopes {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}
.sidebar-bottom {
  flex-shrink: 0;
}
.sidebar-item {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 38px;
  position: relative;
  overflow: hidden;
  width: 100%;
  padding: 8px 10px;
  margin-bottom: 4px;
  border-radius: 8px;
  color: var(--m3-on-surface-variant);
  text-align: left;
  transition:
    padding var(--navigation-duration) var(--navigation-easing),
    gap var(--navigation-duration) var(--navigation-easing),
    background-color 180ms ease,
    color 180ms ease;
}
.sidebar-item :deep(.n-icon) {
  flex-shrink: 0;
}
.sidebar-label {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  transition: opacity var(--navigation-duration) var(--navigation-easing);
}
.sidebar-item:hover {
  background: var(--sidebar-hover-bg);
}
.sidebar-item.active {
  background: var(--sidebar-active-bg);
  color: var(--m3-on-surface);
}
.sidebar-item:focus-visible {
  outline: 2px solid var(--m3-primary);
  outline-offset: -2px;
}
.sidebar-count-enter-active,
.sidebar-count-leave-active {
  transition:
    opacity 180ms ease,
    transform 180ms ease;
}
.sidebar-count-enter-from,
.sidebar-count-leave-to {
  opacity: 0;
  scale: 0.92;
}
.compact .sidebar-item {
  padding-inline: calc((var(--sidebar-width) - 34px) / 2);
  gap: 0;
}
.compact .sidebar-label {
  opacity: 0;
}
.counts .sidebar-scopes .sidebar-label {
  padding-inline-end: 30px;
}
.sidebar :deep(.sidebar-count) {
  position: absolute;
  top: 50%;
  right: 10px;
  transform: translateY(-50%);
  transition:
    scale var(--navigation-duration) var(--navigation-easing),
    top var(--navigation-duration) var(--navigation-easing),
    right var(--navigation-duration) var(--navigation-easing),
    transform var(--navigation-duration) var(--navigation-easing),
    opacity var(--navigation-duration) var(--navigation-easing),
    font-size var(--navigation-duration) var(--navigation-easing);
}
.compact :deep(.sidebar-count) {
  transform: translateY(0);
  top: 1px;
  right: 1px;
  min-width: 14px;
  max-width: 30px;
  overflow: hidden;
  padding: 0 3px;
  font-size: 9px;
  line-height: 13px;
}
@media (prefers-reduced-motion: reduce) {
  .sidebar-item,
  .sidebar :deep(.sidebar-count),
  .sidebar-label {
    transition-duration: 1ms;
  }
}
</style>
