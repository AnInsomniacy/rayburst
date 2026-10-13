<script setup lang="ts">
/** @fileoverview Advanced task options panel (UA, auth, referer, cookie, proxy checkbox). */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  NFormItem,
  NSelect,
  NInput,
  NInputGroup,
  NInputGroupLabel,
  NCheckbox,
  NCollapseTransition,
  NButton,
  NSwitch,
  NIcon,
} from 'naive-ui'
import { hasUnsafeHeaderChars, sanitizeHeaderValue } from '@shared/utils/headerSanitize'
import { useSystemProxyDetect } from '@/composables/useSystemProxyDetect'
import { useAppMessage } from '@/composables/useAppMessage'
import { SearchOutline } from '@vicons/ionicons5'
import type { TaskProxyMode } from '@shared/utils/proxy'
import UserAgentPopover from '@/components/common/UserAgentPopover.vue'
import SpeedLimitInput from '@/components/common/SpeedLimitInput.vue'
import type { UserAgentProfile, UserAgentRule } from '@shared/types'
import { parseChecksum } from '@shared/utils/checksum'

const { t } = useI18n()

const props = defineProps<{
  mediaMode?: 'auto' | 'file' | 'hls' | 'dash' | 'collection'
  show: boolean
  userAgent: string
  authorization: string
  httpAuthUsername: string
  httpAuthPassword: string
  saveHttpAuth: boolean
  referer: string
  cookie: string
  /** Proxy mode for this task. */
  proxyMode: TaskProxyMode
  /** Custom proxy address when proxyMode is 'manual'. */
  customProxy: string
  customProxyUsername?: string
  customProxyPassword?: string
  speedLimit?: string
  checksum?: string
  /** A checksum names one file, so it needs exactly one download. */
  checksumAvailable?: boolean
  sourceUrl?: string
  finalUrl?: string
  userAgentSource?: string
  userAgentProfiles: UserAgentProfile[]
  userAgentRules: UserAgentRule[]
  recentUserAgentProfileIds: string[]
}>()

const emit = defineEmits<{
  'update:mediaMode': [value: 'auto' | 'file' | 'hls' | 'dash' | 'collection']
  'update:show': [value: boolean]
  'update:userAgent': [value: string]
  'update:authorization': [value: string]
  'update:httpAuthUsername': [value: string]
  'update:httpAuthPassword': [value: string]
  'update:saveHttpAuth': [value: boolean]
  'update:referer': [value: string]
  'update:cookie': [value: string]
  'update:proxyMode': [value: TaskProxyMode]
  'update:customProxy': [value: string]
  'update:customProxyUsername': [value: string]
  'update:customProxyPassword': [value: string]
  'update:speedLimit': [value: string]
  'update:checksum': [value: string]
  selectUserAgentProfile: [profile: UserAgentProfile]
}>()

const parsedChecksum = computed(() => (props.checksum?.trim() ? parseChecksum(props.checksum) : null))
const checksumFeedback = computed(() => {
  if (!props.checksum?.trim()) return t('task.checksum-hint')
  return parsedChecksum.value ? parsedChecksum.value.algorithm.toUpperCase() : t('task.checksum-invalid')
})

const uaHasIssue = computed(() => !!props.userAgent && hasUnsafeHeaderChars(props.userAgent))

function cleanUserAgent() {
  emit('update:userAgent', sanitizeHeaderValue(props.userAgent))
}

const message = useAppMessage()
const { detecting: detectingProxy, detect: detectProxy } = useSystemProxyDetect({
  onSuccess(info) {
    emit('update:customProxy', info.server)
    message.success(t('preferences.proxy-detected-success'))
  },
  onNotFound() {
    message.info(t('preferences.proxy-system-not-detected'))
  },
  onError() {
    message.error(t('preferences.proxy-system-detect-failed'))
  },
})
</script>

<template>
  <!-- Closed, nothing follows the toggle, so it keeps no space for a message. -->
  <NFormItem :show-label="false" :show-feedback="show">
    <NCheckbox :checked="show" @update:checked="$emit('update:show', $event)">
      {{ t('task.show-advanced-options') }}
    </NCheckbox>
  </NFormItem>
  <NCollapseTransition :show="show">
    <NFormItem v-if="mediaMode !== undefined" :label="t('media.mode')">
      <NSelect
        :value="mediaMode"
        :options="[
          { value: 'auto', label: t('media.auto') },
          { value: 'file', label: t('media.original') },
          { value: 'hls', label: 'HLS' },
          { value: 'dash', label: 'DASH' },
        ]"
        @update:value="(value) => $emit('update:mediaMode', value)"
      />
    </NFormItem>
    <div>
      <NFormItem :label="t('task.task-user-agent') + ':'">
        <div class="ua-field-wrapper">
          <NInputGroup class="ua-input-row">
            <NInput
              :value="userAgent"
              type="textarea"
              :autosize="{ minRows: 1, maxRows: 3 }"
              @update:value="$emit('update:userAgent', $event)"
            />
            <UserAgentPopover
              :url="sourceUrl"
              :final-url="finalUrl"
              :referer="referer"
              :profiles="userAgentProfiles"
              :rules="userAgentRules"
              :recent-profile-ids="recentUserAgentProfileIds"
              @select="$emit('selectUserAgentProfile', $event)"
            />
          </NInputGroup>
          <div class="ua-source-collapse" :class="{ 'ua-source-collapse--open': userAgentSource }">
            <div class="ua-source-collapse__inner">
              <div class="ua-source">{{ userAgentSource }}</div>
            </div>
          </div>
          <!-- UA sanitization hint — slides in via CSS Grid 0fr→1fr -->
          <div class="ua-warn-collapse" :class="{ 'ua-warn-collapse--open': uaHasIssue }">
            <div class="ua-warn-collapse__inner">
              <div class="ua-warn-bar">
                <span class="ua-warn-text">⚠ {{ t('preferences.ua-unsafe-chars-detected') }}</span>
                <NButton size="tiny" type="primary" ghost @click="cleanUserAgent">
                  {{ t('preferences.ua-sanitize') }}
                </NButton>
              </div>
            </div>
          </div>
        </div>
      </NFormItem>
      <NFormItem :label="t('task.speed-limit') + ':'">
        <SpeedLimitInput
          :value="speedLimit ?? ''"
          :placeholder="t('task.speed-limit-unlimited')"
          @update:value="$emit('update:speedLimit', $event)"
        />
      </NFormItem>
      <NFormItem
        class="checksum-item"
        :label="t('task.checksum') + ':'"
        :validation-status="checksum?.trim() && !parsedChecksum ? 'error' : undefined"
        :feedback="checksumAvailable ? checksumFeedback : t('task.checksum-single-only')"
      >
        <NInput
          :value="checksum"
          :disabled="!checksumAvailable"
          :placeholder="t('task.checksum-placeholder')"
          clearable
          @update:value="$emit('update:checksum', $event)"
        />
      </NFormItem>
      <!-- One place for credentials: HTTP authentication, or a raw Authorization header. -->
      <NFormItem :label="t('task.task-authorization') + ':'">
        <div class="http-auth-fields">
          <div class="http-auth-credentials">
            <NInput
              :value="httpAuthUsername"
              :placeholder="t('task.task-http-auth-username-placeholder')"
              @update:value="$emit('update:httpAuthUsername', $event)"
            />
            <NInput
              :value="httpAuthPassword"
              type="password"
              show-password-on="click"
              :placeholder="t('task.task-http-auth-password-placeholder')"
              @update:value="$emit('update:httpAuthPassword', $event)"
            />
          </div>
          <NCheckbox :checked="saveHttpAuth" @update:checked="$emit('update:saveHttpAuth', $event)">
            {{ t('task.task-http-auth-save') }}
          </NCheckbox>
          <NInputGroup>
            <NInputGroupLabel>Authorization</NInputGroupLabel>
            <NInput
              :value="authorization"
              :placeholder="t('task.task-authorization-placeholder')"
              @update:value="$emit('update:authorization', $event)"
            />
          </NInputGroup>
        </div>
      </NFormItem>
      <NFormItem :label="t('task.task-referer') + ':'">
        <NInput
          :value="referer"
          type="textarea"
          :autosize="{ minRows: 1, maxRows: 3 }"
          @update:value="$emit('update:referer', $event)"
        />
      </NFormItem>
      <NFormItem :label="t('task.task-cookie') + ':'">
        <NInput
          :value="cookie"
          type="textarea"
          :autosize="{ minRows: 1, maxRows: 3 }"
          @update:value="$emit('update:cookie', $event)"
        />
      </NFormItem>
      <NFormItem :label="t('task.use-proxy') + ':'">
        <NSwitch
          :value="proxyMode === 'manual'"
          @update:value="$emit('update:proxyMode', $event ? 'manual' : 'direct')"
        />
      </NFormItem>
      <NFormItem label=" " :show-feedback="false" class="proxy-options-item">
        <NCollapseTransition :show="proxyMode === 'manual'">
          <div class="proxy-radio-group">
            <div class="custom-proxy-input">
              <NInput
                :value="customProxy"
                placeholder="http://host:port"
                @update:value="$emit('update:customProxy', $event)"
              />
              <NInput
                :value="customProxyUsername"
                :placeholder="t('preferences.proxy-username')"
                @update:value="$emit('update:customProxyUsername', $event)"
              />
              <NInput
                :value="customProxyPassword"
                type="password"
                show-password-on="click"
                :placeholder="t('preferences.proxy-password')"
                @update:value="$emit('update:customProxyPassword', $event)"
              />
              <NButton :loading="detectingProxy" size="small" @click="detectProxy">
                <template #icon>
                  <NIcon><SearchOutline /></NIcon>
                </template>
                {{ t('preferences.detect-system-proxy') }}
              </NButton>
            </div>
          </div>
        </NCollapseTransition>
      </NFormItem>
    </div>
  </NCollapseTransition>
</template>

<style scoped>
/* ── UA field wrapper — stacks textarea + warning ────────────────── */
.ua-field-wrapper {
  display: flex;
  flex-direction: column;
  width: 100%;
}
.ua-input-row {
  display: flex;
  align-items: stretch;
  width: 100%;
}
.ua-input-row :deep(.n-input) {
  flex: 1;
}
.ua-source {
  margin-top: 5px;
  color: var(--m3-on-surface-variant);
  font-size: var(--font-size-sm);
  opacity: 0;
  transform: translateY(-4px);
  transition:
    opacity 0.25s cubic-bezier(0.2, 0, 0, 1),
    transform 0.25s cubic-bezier(0.2, 0, 0, 1);
}
.ua-source-collapse {
  display: grid;
  grid-template-rows: 0fr;
  transition: grid-template-rows 0.35s cubic-bezier(0.2, 0, 0, 1);
}
.ua-source-collapse--open {
  grid-template-rows: 1fr;
}
.ua-source-collapse__inner {
  overflow: hidden;
}
.ua-source-collapse--open .ua-source {
  opacity: 1;
  transform: translateY(0);
}

/* ── UA warning — CSS Grid 0fr→1fr slide-in ──────────────────────── */
.ua-warn-collapse {
  display: grid;
  grid-template-rows: 0fr;
  transition: grid-template-rows 0.35s cubic-bezier(0.2, 0, 0, 1);
}
.ua-warn-collapse--open {
  grid-template-rows: 1fr;
}
.ua-warn-collapse__inner {
  overflow: hidden;
}
.ua-warn-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  margin-top: 6px;
  border-radius: var(--radius-sm);
  background: var(--m3-error-container);
  opacity: 0;
  transition: opacity 0.25s cubic-bezier(0.2, 0, 0, 1);
}
.ua-warn-collapse--open .ua-warn-bar {
  opacity: 1;
}
.ua-warn-text {
  font-size: var(--font-size-sm);
  color: var(--m3-on-error-container);
  flex: 1;
}

/* ── Proxy radio group ──────────────────────────────────────────── */
.proxy-radio-group {
  display: flex;
  flex-direction: column;
  width: 100%;
}
.custom-proxy-input {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.custom-proxy-input .n-button {
  align-self: flex-start;
}
.http-auth-fields {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}
.http-auth-credentials {
  display: flex;
  gap: 6px;
}

/* The checksum's hint fills the space other items keep empty below them;
 * give it the same gap to the next item. */
.checksum-item :deep(.n-form-item-feedback-wrapper) {
  padding-bottom: 8px;
}
</style>
