import { describe, expect, it } from 'vitest'
import { buildAdvancedForm, buildAdvancedSystemConfig } from '../useAdvancedPreference'
import { buildGeneralForm, transformGeneralForStore } from '../useGeneralPreference'
import { createDefaultAppConfig } from '@shared/utils/configHydration'

describe('Advanced preference ownership', () => {
  it('sends only RPC options to the engine', () => {
    const form = buildAdvancedForm(createDefaultAppConfig())
    expect(buildAdvancedSystemConfig(form)).toEqual({
      'rpc-listen-port': String(form.rpcListenPort),
      'allow-remote-access': String(form.allowRemoteAccess),
      'rpc-secret': form.rpcSecret,
    })
    expect(form).not.toHaveProperty('clipboardEnable')
    expect(form).not.toHaveProperty('extensionTorrentAction')
  })
})

describe('General preference ownership', () => {
  it('persists the flattened clipboard switches as the clipboard object', () => {
    const form = buildGeneralForm(createDefaultAppConfig())
    const stored = transformGeneralForStore({ ...form, clipboardSftp: false })
    expect(stored.clipboard).toMatchObject({ sftp: false })
    expect(stored).not.toHaveProperty('clipboardSftp')
    expect(stored.extensionDownloadBehavior).toBe(form.extensionDownloadBehavior)
    expect(stored.taskNotification).toBe(form.taskNotification)
    expect(stored).not.toHaveProperty('proxy')
  })
})
