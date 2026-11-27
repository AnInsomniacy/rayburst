/** @fileoverview Verify engine integrity, target selection and failed-download cleanup. */
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { chmod, mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { basename, join } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'
import { prepareEngine } from './prepare-engine.mjs'

const manifest = JSON.parse(await readFile(new URL('./engine-manifest.json', import.meta.url), 'utf8'))
const body = Buffer.from('Pinned engine fixture\n')
const sha256 = createHash('sha256').update(body).digest('hex')
const lock = {
  ...manifest,
  targets: Object.fromEntries(
    Object.entries(manifest.targets).map(([target, entry]) => [target, { ...entry, sha256 }]),
  ),
}

async function fixture(t) {
  const directory = await mkdtemp(join(tmpdir(), 'rayburst-engine-test-'))
  t.after(() => rm(directory, { recursive: true, force: true }))
  const download = t.mock.method(globalThis, 'fetch', async (url, options) => {
    assert.ok(options.signal instanceof AbortSignal)
    assert.ok(url.startsWith(`https://github.com/${lock.repository}/releases/download/v${lock.version}/`))
    return new Response(body)
  })
  return { directory, download }
}

for (const [target, entry] of Object.entries(lock.targets)) {
  test(`prepares only ${target}, then reuses its verified file offline`, async (t) => {
    const { directory, download } = await fixture(t)
    const destination = await prepareEngine(target, directory, lock)
    const name = `aria2-next-${target}${target.includes('windows') ? '.exe' : ''}`
    assert.equal(basename(destination), name)
    assert.deepEqual(await readFile(destination), body)
    assert.deepEqual(await readdir(directory), [name])
    assert.equal(
      download.mock.calls[0].arguments[0],
      `https://github.com/${lock.repository}/releases/download/v${lock.version}/${entry.asset}`,
    )
    await chmod(destination, 0o600)
    download.mock.mockImplementation(() => {
      throw new Error('Offline')
    })
    assert.equal(await prepareEngine(target, directory, lock), destination)
    assert.equal(download.mock.calls.length, 1)
    if (process.platform !== 'win32' && !target.includes('windows')) {
      assert.equal((await stat(destination)).mode & 0o777, 0o755)
    }
  })
}

test('replaces a stale engine only after verifying the new bytes', async (t) => {
  const { directory, download } = await fixture(t)
  const target = 'aarch64-apple-darwin'
  const destination = join(directory, `aria2-next-${target}`)
  await writeFile(destination, 'Old engine')
  await prepareEngine(target, directory, lock)
  assert.deepEqual(await readFile(destination), body)
  assert.equal(download.mock.calls.length, 1)
  assert.deepEqual(await readdir(directory), [basename(destination)])
})

const failures = [
  ['HTTP error', () => new Response('Unavailable', { status: 503 }), /HTTP 503/],
  ['checksum mismatch', () => new Response('Untrusted engine'), /SHA-256 mismatch/],
  [
    'network failure',
    () => {
      throw new Error('Network unavailable')
    },
    /Network unavailable/,
  ],
  [
    'interrupted body',
    () =>
      new Response(
        new ReadableStream({
          start(controller) {
            controller.error(new Error('Interrupted download'))
          },
        }),
      ),
    /Interrupted download/,
  ],
]

for (const [name, response, expected] of failures) {
  for (const existing of [false, true]) {
    test(`${name} leaves ${existing ? 'the old engine intact' : 'no engine installed'} and removes temporary files`, async (t) => {
      const { directory, download } = await fixture(t)
      const target = 'x86_64-pc-windows-msvc'
      const name = `aria2-next-${target}.exe`
      const destination = join(directory, name)
      if (existing) await writeFile(destination, 'Old engine')
      download.mock.mockImplementation(response)
      await assert.rejects(prepareEngine(target, directory, lock), expected)
      assert.deepEqual(await readdir(directory), existing ? [name] : [])
      if (existing) assert.equal(await readFile(destination, 'utf8'), 'Old engine')
    })
  }
}

test('rejects unsupported targets before downloading or creating files', async (t) => {
  const { directory, download } = await fixture(t)
  for (const target of ['unsupported', '__proto__', 'toString', '../../escape']) {
    await assert.rejects(prepareEngine(target, directory, lock), /Unsupported engine target/)
  }
  assert.equal(download.mock.calls.length, 0)
  assert.deepEqual(await readdir(directory), [])
})

test('CLI reports unsupported explicit and environment targets with a failing exit status', () => {
  const script = fileURLToPath(new URL('./prepare-engine.mjs', import.meta.url))
  for (const args of [['unsupported'], []]) {
    const result = spawnSync(process.execPath, [script, ...args], {
      encoding: 'utf8',
      env: { ...process.env, TAURI_ENV_TARGET_TRIPLE: 'unsupported' },
    })
    assert.equal(result.status, 1)
    assert.match(result.stderr, /Cannot prepare engine: Unsupported engine target/)
  }
})
