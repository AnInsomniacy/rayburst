import type { AppConfig } from '@shared/types'

// ── Types ───────────────────────────────────────────────────────────

export interface GeneralForm {
  [key: string]: unknown
  locale: AppConfig['locale']
  theme: AppConfig['theme']
  colorScheme: string
  customColorScheme: string
  taskCardMode: AppConfig['taskCardMode']
  showLogoWhenEmpty: boolean
  reduceMotion: boolean
  sidebarTaskCounts: boolean
  iecSizeUnits: boolean
  systemTitleBar: boolean
  autoCheckUpdate: boolean
  autoCheckUpdateInterval: number
  updateChannel: AppConfig['updateChannel']
  showProgressBar: boolean
  dockBadgeSpeed: boolean
  openAtLogin: boolean
  autoHideWindow: boolean
  keepWindowState: boolean
  resumeAllWhenAppLaunched: boolean
  minimizeToTrayOnClose: boolean
  showTrayIcon: boolean
  hideDockOnMinimize: boolean
  traySpeedometer: boolean
  lightweightMode: boolean
  // Links picked up from the clipboard.
  clipboardEnable: boolean
  clipboardHttp: boolean
  clipboardSftp: boolean
  clipboardMagnet: boolean
  clipboardEd2k: boolean
  clipboardThunder: boolean
  clipboardBtHash: boolean
  clipboardHttpFilesOnly: boolean
  // Notifications
  taskNotification: boolean
  notifyOnStart: boolean
  notifyOnComplete: boolean
  notificationClickAction: AppConfig['notificationClickAction']
}

// ── Pure Functions ──────────────────────────────────────────────────

/**
 * Builds the general form state from the preference store config.
 */
export function buildGeneralForm(config: AppConfig): GeneralForm {
  return {
    locale: config.locale,
    theme: config.theme,
    colorScheme: config.colorScheme,
    customColorScheme: config.customColorScheme,
    taskCardMode: config.taskCardMode,
    showLogoWhenEmpty: config.showLogoWhenEmpty,
    reduceMotion: config.reduceMotion,
    sidebarTaskCounts: config.sidebarTaskCounts,
    iecSizeUnits: config.iecSizeUnits,
    systemTitleBar: config.systemTitleBar,
    autoCheckUpdate: config.autoCheckUpdate,
    autoCheckUpdateInterval: config.autoCheckUpdateInterval,
    updateChannel: config.updateChannel,
    showProgressBar: config.showProgressBar,
    dockBadgeSpeed: config.dockBadgeSpeed,
    openAtLogin: config.openAtLogin,
    autoHideWindow: config.autoHideWindow,
    keepWindowState: config.keepWindowState,
    resumeAllWhenAppLaunched: config.resumeAllWhenAppLaunched,
    minimizeToTrayOnClose: config.minimizeToTrayOnClose,
    showTrayIcon: config.showTrayIcon,
    hideDockOnMinimize: config.hideDockOnMinimize,
    traySpeedometer: config.traySpeedometer,
    lightweightMode: config.lightweightMode,
    clipboardEnable: config.clipboard.enable,
    clipboardHttp: config.clipboard.http,
    clipboardSftp: config.clipboard.sftp,
    clipboardMagnet: config.clipboard.magnet,
    clipboardEd2k: config.clipboard.ed2k,
    clipboardThunder: config.clipboard.thunder,
    clipboardBtHash: config.clipboard.btHash,
    clipboardHttpFilesOnly: config.clipboard.httpFilesOnly,
    taskNotification: config.taskNotification,
    notifyOnStart: config.notifyOnStart,
    notifyOnComplete: config.notifyOnComplete,
    notificationClickAction: config.notificationClickAction,
  }
}

/** Folds the flat clipboard fields back into the stored `clipboard` object. */
export function transformGeneralForStore(form: GeneralForm): Partial<AppConfig> {
  const {
    clipboardEnable,
    clipboardHttp,
    clipboardSftp,
    clipboardMagnet,
    clipboardEd2k,
    clipboardThunder,
    clipboardBtHash,
    clipboardHttpFilesOnly,
    ...rest
  } = form
  return {
    ...rest,
    clipboard: {
      enable: clipboardEnable,
      http: clipboardHttp,
      sftp: clipboardSftp,
      magnet: clipboardMagnet,
      ed2k: clipboardEd2k,
      thunder: clipboardThunder,
      btHash: clipboardBtHash,
      httpFilesOnly: clipboardHttpFilesOnly,
    },
  }
}
