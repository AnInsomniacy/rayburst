<script setup lang="ts">
/** @fileoverview Edits per-site rules: a host pattern, its connections per server and its proxy choice. */
import { useI18n } from 'vue-i18n'
import { computed } from 'vue'
import { NButton, NIcon, NInput, NInputNumber, NSelect, NSwitch, NText } from 'naive-ui'
import { AddOutline, TrashOutline } from '@vicons/ionicons5'
import type { SiteRule } from '@shared/types'
import {
  isValidSiteHostPattern,
  MAX_SITE_RULES,
  SITE_RULE_CONNECTION_RANGE,
  SITE_RULE_PROXY_MODES,
} from '@shared/utils/siteRules'

const props = defineProps<{ value: SiteRule[] }>()
const emit = defineEmits<{ 'update:value': [value: SiteRule[]] }>()
const { t } = useI18n()
const proxyLabels: Record<SiteRule['proxy'], () => string> = {
  default: () => t('preferences.site-rule-proxy-default'),
  proxy: () => t('preferences.site-rule-proxy-proxy'),
  direct: () => t('preferences.site-rule-proxy-direct'),
}
const proxyOptions = computed(() => SITE_RULE_PROXY_MODES.map((mode) => ({ value: mode, label: proxyLabels[mode]() })))

function update(index: number, patch: Partial<SiteRule>) {
  emit(
    'update:value',
    props.value.map((rule, position) => (position === index ? { ...rule, ...patch } : rule)),
  )
}

function add() {
  emit('update:value', [
    ...props.value,
    { id: crypto.randomUUID(), enabled: true, hostPattern: '', maxConnectionPerServer: 0, proxy: 'default' },
  ])
}

function remove(index: number) {
  emit(
    'update:value',
    props.value.filter((_, position) => position !== index),
  )
}
</script>

<template>
  <div class="site-rules">
    <div v-for="(rule, index) in value" :key="rule.id" class="site-rule">
      <NSwitch
        :value="rule.enabled"
        size="small"
        :aria-label="t('preferences.site-rule-enabled')"
        @update:value="(enabled: boolean) => update(index, { enabled })"
      />
      <NInput
        :value="rule.hostPattern"
        size="small"
        class="site-rule-host"
        placeholder="example.com"
        :status="rule.hostPattern && !isValidSiteHostPattern(rule.hostPattern) ? 'error' : undefined"
        @update:value="(hostPattern: string) => update(index, { hostPattern: hostPattern.trim().toLowerCase() })"
      />
      <NInputNumber
        :value="rule.maxConnectionPerServer || null"
        size="small"
        clearable
        class="site-rule-connections"
        :min="SITE_RULE_CONNECTION_RANGE.min"
        :max="SITE_RULE_CONNECTION_RANGE.max"
        :placeholder="t('preferences.site-rule-connections-default')"
        :title="t('preferences.site-rule-connections')"
        :aria-label="t('preferences.site-rule-connections')"
        @update:value="(connections: number | null) => update(index, { maxConnectionPerServer: connections ?? 0 })"
      />
      <NSelect
        :value="rule.proxy"
        size="small"
        class="site-rule-proxy"
        :options="proxyOptions"
        :aria-label="t('preferences.site-rule-proxy')"
        @update:value="(proxy: SiteRule['proxy']) => update(index, { proxy })"
      />
      <NButton quaternary circle size="small" :aria-label="t('app.delete')" @click="remove(index)">
        <template #icon>
          <NIcon><TrashOutline /></NIcon>
        </template>
      </NButton>
    </div>
    <div class="site-rules-footer">
      <NButton size="small" dashed :disabled="value.length >= MAX_SITE_RULES" @click="add">
        <template #icon>
          <NIcon><AddOutline /></NIcon>
        </template>
        {{ t('preferences.site-rule-add') }}
      </NButton>
      <NText depth="3" class="site-rules-hint">{{ t('preferences.site-rules-hint') }}</NText>
    </div>
  </div>
</template>

<style scoped>
.site-rules {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}
.site-rule {
  display: flex;
  align-items: center;
  gap: 8px;
}
.site-rule-host {
  flex: 1;
  min-width: 0;
}
.site-rule-connections {
  width: 110px;
  flex-shrink: 0;
}
.site-rule-proxy {
  width: 150px;
  flex-shrink: 0;
}
.site-rules-footer {
  display: flex;
  align-items: center;
  gap: 10px;
}
.site-rules-hint {
  font-size: var(--font-size-sm);
}
</style>
