<script setup lang="ts">
/** Persistent footer controls bound to the active preference form. */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { NButton, NIcon, NDropdown, type DropdownOption } from 'naive-ui'
import { RefreshOutline, EllipsisHorizontalOutline } from '@vicons/ionicons5'
import { useEngineRestart } from '@/composables/useEngineRestart'
import { reducedMotion } from '@/composables/useWindowReveal'

const props = withDefaults(defineProps<{ isDirty: boolean; isValid?: boolean }>(), { isValid: true })
const emit = defineEmits<{ save: []; discard: [] }>()
const { t } = useI18n()
const { confirmManualRestart } = useEngineRestart()
const secondaryActions = computed<DropdownOption[]>(() => [
  { key: 'discard', label: t('preferences.discard'), disabled: !props.isDirty },
  { key: 'restart', label: t('preferences.engine-restart-btn') },
])
function selectAction(key: string | number) {
  if (key === 'discard' && props.isDirty) emit('discard')
  if (key === 'restart') void confirmManualRestart()
}

/** Unhurried enough to be followed; the standard curve, which eases out evenly. */
const ENTER_MS = 380
const LEAVE_MS = 220
const EASE = 'cubic-bezier(0.2, 0, 0, 1)'

// The buttons after save and discard glide to their new place instead of
// jumping when those appear or go.
const trailing = ref<HTMLElement | null>(null)
let from = 0

function remember() {
  from = trailing.value?.getBoundingClientRect().left ?? 0
}

function glide(duration: number) {
  const element = trailing.value
  if (!element || reducedMotion()) return
  const offset = from - element.getBoundingClientRect().left
  if (!offset) return
  element.animate([{ transform: `translateX(${offset}px)` }, { transform: 'none' }], { duration, easing: EASE })
}
</script>

<template>
  <div class="form-actions">
    <!-- Shown only with unsaved changes: greyed-out buttons read as broken text (#723). -->
    <Transition
      name="dirty-actions"
      :duration="{ enter: ENTER_MS, leave: LEAVE_MS }"
      @before-enter="remember"
      @enter="glide(ENTER_MS)"
      @before-leave="remember"
      @after-leave="glide(ENTER_MS - 80)"
    >
      <div v-if="isDirty" class="dirty-actions">
        <NButton :type="isValid ? 'primary' : 'default'" :disabled="!isValid" @click="emit('save')">{{
          t('preferences.save')
        }}</NButton>
        <NButton class="secondary-action" type="error" ghost @click="emit('discard')">{{
          t('preferences.discard')
        }}</NButton>
      </div>
    </Transition>
    <div ref="trailing" class="trailing-actions">
      <NButton class="secondary-action" type="info" ghost @click="confirmManualRestart">
        <template #icon
          ><NIcon :size="16"><RefreshOutline /></NIcon
        ></template>
        {{ t('preferences.engine-restart-btn') }}
      </NButton>
      <NDropdown trigger="click" placement="top-start" :options="secondaryActions" @select="selectAction">
        <NButton class="compact-actions" quaternary :aria-label="t('task.more-actions')">
          <template #icon
            ><NIcon><EllipsisHorizontalOutline /></NIcon
          ></template>
        </NButton>
      </NDropdown>
    </div>
  </div>
</template>

<style scoped>
.form-actions,
.dirty-actions,
.trailing-actions {
  display: flex;
  align-items: center;
  gap: var(--actions-gap);
  min-width: 0;
}
.form-actions {
  --actions-gap: 12px;
  min-height: 46px;
}
.dirty-actions {
  flex-shrink: 1;
}
.dirty-actions > :first-child {
  min-width: 0;
  flex-shrink: 1;
}
/* Clipped sideways only: the button's line height is 1, and CJK glyphs drawn in
   a fallback font taller than the Latin one would lose their top row (#723). */
.form-actions :deep(.n-button__content) {
  display: block;
  overflow: clip visible;
  text-overflow: ellipsis;
}
.dirty-actions-enter-active {
  transition:
    opacity 380ms cubic-bezier(0.2, 0, 0, 1),
    transform 380ms cubic-bezier(0.2, 0, 0, 1);
}
.dirty-actions-leave-active {
  transition:
    opacity 220ms var(--dialog-ease-leave),
    transform 220ms var(--dialog-ease-leave);
}
.dirty-actions-enter-from,
.dirty-actions-leave-to {
  opacity: 0;
  transform: translateX(-16px);
}
.compact-actions {
  display: none;
}
@container footer (max-width: 640px) {
  .secondary-action {
    display: none;
  }
  .compact-actions {
    display: inline-flex;
  }
  .form-actions {
    --actions-gap: 8px;
  }
}
</style>
