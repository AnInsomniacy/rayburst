/** Advanced settings own RPC, extension access, the engine, logging, and rendering. */
import { RANDOM_PORT_RANGE_END, RANDOM_PORT_RANGE_START } from '@shared/constants'
import { generateRandomInt } from '@shared/utils'
import type { AppConfig } from '@shared/types'

export interface AdvancedForm {
  [key: string]: unknown
  rpcListenPort: number
  rpcSecret: string
  extensionDownloadBehavior: AppConfig['extensionDownloadBehavior']
  extensionTorrentAction: AppConfig['extensionTorrentAction']
  extensionApiPort: number
  extensionApiSecret: string
  allowRemoteAccess: boolean
  logLevel: AppConfig['logLevel']
  aria2LogLevel: AppConfig['aria2LogLevel']
  tempFilesDir: string
  hardwareRendering: boolean
}

export function buildAdvancedForm(config: AppConfig): AdvancedForm {
  return {
    rpcListenPort: config.rpcListenPort,
    rpcSecret: config.rpcSecret,
    extensionDownloadBehavior: config.extensionDownloadBehavior,
    extensionTorrentAction: config.extensionTorrentAction,
    extensionApiPort: config.extensionApiPort,
    extensionApiSecret: config.extensionApiSecret,
    allowRemoteAccess: config.allowRemoteAccess,
    logLevel: config.logLevel,
    aria2LogLevel: config.aria2LogLevel,
    tempFilesDir: config.tempFilesDir,
    hardwareRendering: config.hardwareRendering,
  }
}

export function buildAdvancedSystemConfig(form: AdvancedForm): Record<string, string> {
  return {
    'rpc-listen-port': String(form.rpcListenPort),
    'allow-remote-access': String(form.allowRemoteAccess),
    'rpc-secret': form.rpcSecret,
  }
}

export function randomRpcPort(): number {
  return generateRandomInt(RANDOM_PORT_RANGE_START, RANDOM_PORT_RANGE_END + 1)
}
