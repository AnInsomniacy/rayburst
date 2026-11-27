#!/usr/bin/env node
/** @fileoverview Prepare the pinned Aria2 Next sidecar for one Rust target. */
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { createReadStream, createWriteStream } from 'node:fs'
import { chmod, mkdir, mkdtemp, readFile, rename, rm } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { Readable } from 'node:stream'
import { pipeline } from 'node:stream/promises'
import { fileURLToPath } from 'node:url'

const scriptPath = fileURLToPath(import.meta.url)
const root = join(dirname(scriptPath), '..')
const manifest = JSON.parse(await readFile(join(dirname(scriptPath), 'engine-manifest.json'), 'utf8'))

async function sha256(path) {
  const hash = createHash('sha256')
  for await (const chunk of createReadStream(path)) hash.update(chunk)
  return hash.digest('hex')
}

export async function prepareEngine(target, directory = join(root, 'src-tauri', 'binaries'), lock = manifest) {
  const entry = Object.hasOwn(lock.targets, target) && lock.targets[target]
  if (!entry) throw new Error(`Unsupported engine target: ${target}`)
  const extension = target.includes('windows') ? '.exe' : ''
  const destination = join(directory, `aria2-next-${target}${extension}`)
  const mode = extension ? 0o644 : 0o755
  let existingHash
  try {
    existingHash = await sha256(destination)
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
  }
  if (existingHash === entry.sha256) {
    await chmod(destination, mode)
    console.log(`Aria2 Next ${lock.version} ready: ${target} (cached)`)
    return destination
  }

  const url = `https://github.com/${lock.repository}/releases/download/v${lock.version}/${entry.asset}`
  console.log(`Downloading Aria2 Next ${lock.version} for ${target}`)
  await mkdir(directory, { recursive: true })
  const temporaryDirectory = await mkdtemp(join(directory, 'aria2-next-'))
  const temporaryFile = join(temporaryDirectory, 'engine')
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(120_000) })
    if (!response.ok || !response.body) throw new Error(`Engine download failed: HTTP ${response.status} (${url})`)
    await pipeline(Readable.fromWeb(response.body), createWriteStream(temporaryFile, { flags: 'wx' }))
    const actual = await sha256(temporaryFile)
    if (actual !== entry.sha256) {
      throw new Error(`Engine SHA-256 mismatch for ${target}: expected ${entry.sha256}, received ${actual}`)
    }
    await chmod(temporaryFile, mode)
    await rename(temporaryFile, destination)
  } finally {
    await rm(temporaryDirectory, { recursive: true, force: true })
  }
  return destination
}

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  try {
    const target =
      process.argv[2] ||
      process.env.TAURI_ENV_TARGET_TRIPLE ||
      execFileSync('rustc', ['--print', 'host-tuple'], { encoding: 'utf8' }).trim()
    await prepareEngine(target)
  } catch (error) {
    console.error(`Cannot prepare engine: ${error.message}`)
    process.exitCode = 1
  }
}
