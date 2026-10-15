import { execFileSync } from 'node:child_process'
import { copyFileSync, readdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const output = mkdtempSync(join(tmpdir(), 'rayburst-icons-'))
try {
  execFileSync(
    process.execPath,
    ['node_modules/@tauri-apps/cli/tauri.js', 'icon', 'public/logo.svg', '--output', output],
    { stdio: 'inherit' },
  )
  for (const entry of readdirSync(output, { withFileTypes: true })) {
    if (entry.isFile() && entry.name !== 'icon.icns') {
      copyFileSync(join(output, entry.name), join('src-tauri/icons', entry.name))
    }
  }
  // The Windows tray shows the bitmap matching each display scale unscaled.
  const tray = join(output, 'tray')
  execFileSync(
    process.execPath,
    ['node_modules/@tauri-apps/cli/tauri.js', 'icon', 'public/logo.svg', '--png', '16,20,24,32,40,48', '--output', tray],
    { stdio: 'inherit' },
  )
  for (const name of readdirSync(tray)) copyFileSync(join(tray, name), join('src-tauri/icons/tray', name))
  copyFileSync('public/logo.svg', 'src-tauri/icons/Rayburst.icon/Assets/logo.svg')
  if (process.platform === 'darwin') {
    // Tauri dev uses an NSImage, not the compiled Icon Composer asset catalog.
    const preview = join(output, 'macos-preview.png')
    execFileSync(
      process.env.ICON_COMPOSER_TOOL ?? '/Applications/Icon Composer.app/Contents/Executables/ictool',
      [
        'src-tauri/icons/Rayburst.icon',
        '--export-image',
        '--output-file',
        preview,
        '--platform',
        'macOS',
        '--rendition',
        'Default',
        '--width',
        '1024',
        '--height',
        '1024',
        '--scale',
        '1',
      ],
      { stdio: 'inherit' },
    )
    const macosOutput = join(output, 'macos')
    execFileSync(
      process.execPath,
      ['node_modules/@tauri-apps/cli/tauri.js', 'icon', preview, '--output', macosOutput],
      { stdio: 'inherit' },
    )
    copyFileSync(join(macosOutput, 'icon.icns'), 'src-tauri/icons/macos-dev.icns')
  }
} finally {
  rmSync(output, { recursive: true, force: true })
}
