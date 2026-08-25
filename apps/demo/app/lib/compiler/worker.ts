/**
 * The compiler worker.
 *
 * Compilation runs off the main thread for two reasons. The obvious one is that a synchronous
 * compile would jank the editor. The important one is isolation: a Rust panic under wasm is a
 * trap, and `panic = "abort"` on this target means there is nothing to catch on either side of
 * the boundary. Owning the compiler in a worker means the client can throw the whole thread
 * away and start a clean one. `crates/trestle` still holds `expect`s that encode grammar
 * invariants, and its walkers recurse without a depth limit against a stack smaller than a
 * native one, so this is a real path rather than a theoretical one.
 */

/// <reference lib="webworker" />

import type { CheckResult, RunResult, WorkerRequest, WorkerResponse } from './types'

type WasmModule = {
  default: () => Promise<unknown>
  check: (source: string) => CheckResult
  run: (source: string) => RunResult
  version: () => string
}

/**
 * `import.meta.glob` rather than a plain dynamic import, because `app/lib/trestle-wasm` is a
 * build artifact that is gitignored and often absent. A bare `import()` of a missing path is
 * a hard Vite resolve error at build time; a glob that matches nothing is simply an empty
 * object, which is exactly the graceful degradation we want. Keeping it a real bundled
 * module also lets Vite rewrite the `new URL('…_bg.wasm', import.meta.url)` inside the
 * wasm-bindgen glue, so the `.wasm` is emitted and fingerprinted like any other asset.
 */
const wasmLoaders = Object.values(
  import.meta.glob<WasmModule>('../trestle-wasm/trestle_wasm.js'),
)

let wasm: WasmModule | null = null

const load = async (): Promise<string | null> => {
  const loader = wasmLoaders[0]
  if (!loader) return null

  const module = await loader()
  await module.default()
  wasm = module
  return module.version()
}

const compile = (kind: 'check' | 'run', source: string): CheckResult | RunResult => {
  // The client only routes work here once the handshake reported a version, so this is a
  // guard against a protocol mistake rather than an expected path. Throwing puts it on the
  // `panic` branch, which is the right treatment: something is wrong with this worker.
  if (!wasm) throw new Error('the WebAssembly compiler is not loaded in this worker')
  return kind === 'check' ? wasm.check(source) : wasm.run(source)
}

const post = (response: WorkerResponse) => self.postMessage(response)

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const request = event.data

  if (request.kind === 'init') {
    try {
      post({ id: request.id, outcome: 'ready', version: await load() })
    } catch (error) {
      // The package exists but would not initialise — a corrupt or half-written build.
      // Report it as absent so the client explains itself rather than the tab dying.
      post({ id: request.id, outcome: 'ready', version: null })
      console.error('[trestle] failed to initialise the WebAssembly compiler', error)
    }
    return
  }

  try {
    post({ id: request.id, outcome: 'result', result: compile(request.kind, request.source) })
  } catch (error) {
    // A trap from the Rust side arrives here as a `RuntimeError`. This worker is finished:
    // the client terminates it on seeing this.
    post({
      id: request.id,
      outcome: 'panic',
      message: error instanceof Error ? error.message : String(error),
    })
  }
}
