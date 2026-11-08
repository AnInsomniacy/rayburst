/**
 * @fileoverview Names the desktop surface that shows download progress.
 *
 * The system progress bar is not portable: KDE Plasma tracks a D-Bus job
 * view, Windows shows a progress toast, macOS paints the Dock tile, and other
 * desktops have none. Settings use the name to offer only surfaces that
 * exist, and to hide the start and completion notifications that a progress
 * surface replaces.
 */
import { computed, ref } from 'vue'
import type { ComputedRef, Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { logger } from '@shared/logger'

export type JobProgressBackend = 'none' | 'kde-job-view' | 'windows-toast' | 'macos-dock'

const backend = ref<JobProgressBackend>('none')
let probed: Promise<void> | null = null

/** Whether the surface itself announces job start and completion. */
const showsProgressNotification = computed(() => backend.value === 'kde-job-view' || backend.value === 'windows-toast')

/**
 * Probes the backend once per session; repeated calls share the result.
 */
export function useJobProgressBackend(): {
  backend: Ref<JobProgressBackend>
  showsProgressNotification: ComputedRef<boolean>
} {
  probed ??= invoke<string>('job_progress_backend')
    .then((name) => {
      backend.value = name as JobProgressBackend
    })
    .catch((error) => {
      logger.debug('JobProgress', `backend probe failed: ${error}`)
    })
  return { backend, showsProgressNotification }
}
