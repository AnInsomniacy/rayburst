// Starts `pnpm tauri dev` with the WebView2 debugging port the E2E suite attaches to.
import { spawn } from 'node:child_process'

const port = process.env.RAYBURST_E2E_CDP_PORT ?? '9223'
const child = spawn('pnpm', ['tauri', 'dev'], {
  stdio: 'inherit',
  shell: process.platform === 'win32',
  env: {
    ...process.env,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
})
child.on('exit', (code) => process.exit(code ?? 0))
