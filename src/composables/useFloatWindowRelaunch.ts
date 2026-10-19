/**
 * @fileoverview Offers a restart when the floating window turns on under Wayland.
 *
 * Wayland lets no window place itself, so the orb runs the app under XWayland,
 * chosen when the process starts. Turned on in a process that started on
 * Wayland, its panel cannot follow it until Rayburst restarts (#731).
 */
import { invoke } from '@tauri-apps/api/core'
import { relaunch } from '@tauri-apps/plugin-process'
import { useDialog } from 'naive-ui'
import { useI18n } from 'vue-i18n'
import { useEngineStore } from '@/stores/engine'
import { logger } from '@shared/logger'
import { getErrorMessage } from '@shared/utils/errorMessage'
import type { FloatWindowMode } from '@shared/types'

export function useFloatWindowRelaunch(): (mode: FloatWindowMode) => Promise<void> {
  const dialog = useDialog()
  const { t } = useI18n()
  const engineStore = useEngineStore()
  let offered = false

  return async (mode) => {
    if (offered) return
    const needed = await invoke<boolean>('float_window_needs_relaunch', { mode }).catch((error: unknown) => {
      logger.debug('FloatWindowRelaunch.check', getErrorMessage(error))
      return false
    })
    if (!needed || offered) return
    offered = true
    dialog.info({
      title: t('preferences.restart-required'),
      content: t('preferences.float-window-restart-confirm'),
      positiveText: t('preferences.restart-now'),
      negativeText: t('preferences.engine-restart-later'),
      maskClosable: false,
      onPositiveClick: async () => {
        await engineStore.stop('appRelaunch')
        await relaunch()
      },
      onAfterLeave: () => {
        offered = false
      },
    })
  }
}
