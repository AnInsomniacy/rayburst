<script setup lang="ts">
/** @fileoverview Covers its positioned parent while files are dragged over the window. */
import { useI18n } from 'vue-i18n'
import { NIcon } from 'naive-ui'
import { BanOutline, CloudUploadOutline } from '@vicons/ionicons5'
import { torrentDragState } from '@/composables/useTorrentDrop'

const { t } = useI18n()
</script>

<template>
  <Transition name="torrent-drop">
    <div
      v-if="torrentDragState !== 'none'"
      class="torrent-drop-overlay"
      :class="{ reject: torrentDragState === 'reject' }"
      role="status"
    >
      <div class="torrent-drop-zone">
        <NIcon :size="40">
          <BanOutline v-if="torrentDragState === 'reject'" />
          <CloudUploadOutline v-else />
        </NIcon>
        <span>{{ t(torrentDragState === 'reject' ? 'task.drop-torrent-only' : 'task.drop-torrent-release') }}</span>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.torrent-drop-overlay {
  position: absolute;
  inset: 0;
  z-index: 100;
  display: flex;
  padding: 12px;
  pointer-events: none;
  background: color-mix(in srgb, var(--main-bg) 82%, transparent);
  backdrop-filter: blur(2px);
}

.torrent-drop-zone {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  border: 2px dashed var(--m3-primary);
  border-radius: var(--radius-md);
  background: color-mix(in srgb, var(--m3-primary-container) 45%, transparent);
  color: var(--m3-primary);
  font-size: var(--font-size-base);
}

.reject .torrent-drop-zone {
  border-color: var(--m3-outline);
  background: color-mix(in srgb, var(--m3-surface-container-highest) 60%, transparent);
  color: var(--m3-on-surface-variant);
}

.torrent-drop-enter-active,
.torrent-drop-leave-active {
  transition: opacity 0.15s cubic-bezier(0.2, 0, 0, 1);
}

.torrent-drop-enter-from,
.torrent-drop-leave-to {
  opacity: 0;
}
</style>
