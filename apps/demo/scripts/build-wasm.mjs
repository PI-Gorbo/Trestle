#!/usr/bin/env node
/**
 * Build `crates/trestle-wasm` into `app/lib/trestle-wasm/`.
 *
 * Kept as a script rather than an inline npm command because it has real failure modes worth
 * explaining: a missing wasm32 target and a missing `wasm-pack` are both one command away
 * from fixed, and a raw cargo error does not say so.
 *
 * The output directory is gitignored. `nuxt generate` does not depend on this having been run
 * — the app resolves the package through `import.meta.glob`, so an absent one is an empty match
 * rather than a resolve error — but the site it produces cannot compile anything.
 */

import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const here = dirname(fileURLToPath(import.meta.url))
const appRoot = resolve(here, '..')
const crate = resolve(appRoot, '../../crates/trestle-wasm')
const outDir = resolve(appRoot, 'app/lib/trestle-wasm')

const has = (command, args) =>
  spawnSync(command, args, { stdio: 'ignore' }).status === 0

if (!has('cargo', ['--version'])) {
  console.error('cargo not found. Install Rust from https://rustup.rs and try again.')
  process.exit(1)
}

if (!has('wasm-pack', ['--version'])) {
  console.error('wasm-pack not found. Install it with:\n\n  cargo install wasm-pack\n')
  process.exit(1)
}

const targets = spawnSync('rustup', ['target', 'list', '--installed'], { encoding: 'utf8' })
if (targets.status === 0 && !targets.stdout.includes('wasm32-unknown-unknown')) {
  console.error(
    'The wasm32 target is not installed. Add it with:\n\n  rustup target add wasm32-unknown-unknown\n',
  )
  process.exit(1)
}

// `--target web` emits an ES module whose glue resolves the `.wasm` through
// `new URL(..., import.meta.url)`, which is what lets Vite fingerprint and serve it as a
// normal asset. `--no-pack` skips the package.json wasm-pack would otherwise write: nothing
// consumes this as an npm package, it is imported by path.
const build = spawnSync(
  'wasm-pack',
  ['build', crate, '--target', 'web', '--out-dir', outDir, '--out-name', 'trestle_wasm', '--no-pack', '--release'],
  { stdio: 'inherit' },
)

if (build.status !== 0) {
  console.error(
    '\nwasm build failed.\n' +
      'If the errors are in `crates/trestle` rather than `crates/trestle-wasm`, the break is in\n' +
      'the compiler itself, not in the shim. Until it is fixed the playground will load but\n' +
      'report the compiler as unavailable.\n',
  )
  process.exit(build.status ?? 1)
}

console.log(`\nWebAssembly compiler written to ${outDir}`)
console.log('Restart `pnpm dev` (or reload) to pick it up.')
