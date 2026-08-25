/**
 * The compiler client: a small pool of workers.
 *
 * Pooling buys two things. Compiles for different tabs genuinely overlap rather than queuing
 * behind one another, and — more importantly — a worker that traps or hangs can be discarded
 * without taking the others with it. A Rust panic under WebAssembly is a trap with no
 * unwinding, so there is nothing to catch inside the worker; throwing the whole thread away is
 * the recovery. `crates/trestle` still reaches `expect`s that encode grammar invariants, and
 * its walkers recurse without a depth limit against a stack smaller than a native one, so this
 * is a real path rather than a theoretical one.
 */

import CompilerWorker from './worker?worker'
import type {
  CompileKind,
  CompileResult,
  CompilerEngine,
  Diagnostic,
  WorkerResponse,
} from './types'

/**
 * Four is plenty: compiles are short, and the ceiling exists to bound memory (each worker
 * holds its own copy of the compiler) rather than to saturate the machine.
 */
const MAX_WORKERS = 4

/**
 * Long enough that no honest compile hits it, short enough that a runaway `evaluate` — the
 * tree-walker has no step limit — does not leave the tab looking frozen.
 */
const COMPILE_TIMEOUT_MS = 5_000

type Job = {
  kind: CompileKind
  source: string
  resolve: (result: CompileResult) => void
}

type Slot = {
  worker: Worker
  job: Job | null
  requestId: number
  timer: ReturnType<typeof setTimeout> | null
}

export type CompilerClient = {
  /** Resolves once we know whether the real compiler is available. */
  ready: Promise<CompilerEngine>
  compile: (kind: CompileKind, source: string) => Promise<CompileResult>
  dispose: () => void
}

/**
 * A diagnostic about the compiler rather than about the program. `render` is null and `labels`
 * empty for the same reason: there is no source position to point at, and no compiler was in a
 * state to draw one.
 */
const internalError = (
  message: string,
  help: string,
  code = 'trestle::internal_compiler_error',
): CompileResult => ({
  ok: false,
  diagnostics: [
    {
      phase: 'internal',
      severity: 'error',
      code,
      message,
      help,
      labels: [],
      render: null,
    } satisfies Diagnostic,
  ],
})

export const createCompilerClient = (): CompilerClient => {
  const slots: Slot[] = []
  const queue: Job[] = []

  let engine: CompilerEngine = { kind: 'unavailable', reason: 'still starting up' }
  let poolSize = 0
  let nextRequestId = 1
  let disposed = false

  const clearTimer = (slot: Slot) => {
    if (slot.timer !== null) clearTimeout(slot.timer)
    slot.timer = null
  }

  /** Tear a worker down and drop it from the pool; `dispatch` will spawn a replacement. */
  const recycle = (slot: Slot) => {
    clearTimer(slot)
    slot.worker.terminate()
    const index = slots.indexOf(slot)
    if (index !== -1) slots.splice(index, 1)
  }

  /** Fail whatever this slot was working on, bin the worker, and keep the queue moving. */
  const fail = (slot: Slot, result: CompileResult) => {
    const job = slot.job
    slot.job = null
    recycle(slot)
    job?.resolve(result)
    dispatch()
  }

  const handle = (slot: Slot, response: WorkerResponse) => {
    // A reply for a request we already gave up on (timed out, then the worker recovered).
    if (!slot.job || response.id !== slot.requestId) return

    if (response.outcome === 'panic') {
      fail(
        slot,
        internalError(
          `The compiler crashed: ${response.message}`,
          'This is a bug in the compiler, not in your program. A fresh worker has been started, so you can keep editing.',
        ),
      )
      return
    }

    if (response.outcome === 'result') {
      const job = slot.job
      clearTimer(slot)
      slot.job = null
      job.resolve(response.result)
      dispatch()
    }
  }

  const spawn = (): Slot => {
    const slot: Slot = { worker: new CompilerWorker(), job: null, requestId: 0, timer: null }

    slot.worker.onmessage = (event: MessageEvent<WorkerResponse>) => handle(slot, event.data)
    slot.worker.onerror = (event) => {
      fail(
        slot,
        internalError(
          `The compiler worker failed to start: ${event.message}`,
          'Try reloading the page.',
        ),
      )
    }

    slots.push(slot)
    return slot
  }

  const assign = (slot: Slot, job: Job) => {
    slot.job = job
    slot.requestId = nextRequestId
    nextRequestId += 1

    slot.timer = setTimeout(() => {
      fail(
        slot,
        internalError(
          `The compiler did not finish within ${COMPILE_TIMEOUT_MS / 1000}s.`,
          'The program may not terminate — evaluation has no step limit yet.',
        ),
      )
    }, COMPILE_TIMEOUT_MS)

    slot.worker.postMessage({ id: slot.requestId, kind: job.kind, source: job.source })
  }

  function dispatch() {
    while (queue.length > 0 && !disposed) {
      const idle = slots.find((slot) => slot.job === null)
      const slot = idle ?? (slots.length < poolSize ? spawn() : null)
      if (!slot) return
      assign(slot, queue.shift()!)
    }
  }

  /**
   * Probe once, on a worker of its own, before any real work is queued. If the WebAssembly
   * package is not there, the pool is shut down: there is nothing else to run, and every
   * later `compile` answers with the reason instead.
   */
  const ready: Promise<CompilerEngine> = new Promise((resolve) => {
    let probe: Slot
    try {
      probe = spawn()
    } catch (error) {
      engine = {
        kind: 'unavailable',
        reason: `web workers are unavailable (${error instanceof Error ? error.message : error})`,
      }
      resolve(engine)
      return
    }

    const id = nextRequestId
    nextRequestId += 1

    const settle = (next: CompilerEngine) => {
      engine = next
      resolve(next)
    }

    probe.worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
      const response = event.data
      if (response.outcome !== 'ready' || response.id !== id) return

      if (response.version === null) {
        recycle(probe)
        settle({
          kind: 'unavailable',
          reason: 'the WebAssembly compiler has not been built — run `pnpm build:wasm`',
        })
        return
      }

      // Hand the probe worker back to the pool rather than discarding it: it has already
      // paid the cost of instantiating the module.
      probe.worker.onmessage = (message: MessageEvent<WorkerResponse>) =>
        handle(probe, message.data)
      poolSize = Math.min(MAX_WORKERS, navigator.hardwareConcurrency || 2)
      settle({ kind: 'wasm', version: response.version })
      dispatch()
    }

    probe.worker.postMessage({ id, kind: 'init' })
  })

  const compile = async (kind: CompileKind, source: string): Promise<CompileResult> => {
    const resolved = await ready
    if (disposed) return internalError('The compiler has been shut down.', 'Reload the page.')

    if (resolved.kind === 'unavailable') {
      return internalError(
        `The compiler is unavailable: ${resolved.reason}.`,
        // Deliberately covers both reasons this branch is reachable: an unbuilt package, and a
        // browser that would not give us a worker. Naming only the first would be wrong advice
        // in the second case.
        'Nothing can be checked or evaluated until this is resolved. If the compiler has not been built, run `pnpm build:wasm` and reload.',
        'trestle::compiler_unavailable',
      )
    }

    return new Promise<CompileResult>((resolve) => {
      queue.push({ kind, source, resolve })
      dispatch()
    })
  }

  const dispose = () => {
    disposed = true
    queue.length = 0
    for (const slot of [...slots]) recycle(slot)
  }

  return { ready, compile, dispose }
}
