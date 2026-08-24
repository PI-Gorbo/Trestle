/**
 * The compiler client: a small pool of workers, plus the fallback to the mock.
 *
 * Pooling buys two things. Compiles for different tabs genuinely overlap rather than queuing
 * behind one another, and — more importantly — a worker that traps or hangs can be discarded
 * without taking the others with it. `crates/trestle` still has reachable `todo!()` holes, so
 * that is a routine event, not a theoretical one.
 */

import CompilerWorker from './worker?worker'
import { mockCheck, mockRun } from './stub'
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

const internalError = (message: string, help: string): CompileResult => ({
  ok: false,
  diagnostics: [
    {
      phase: 'internal',
      severity: 'error',
      code: 'trestle::internal_compiler_error',
      message,
      help,
      // No labels: a trap gives us no source position to point at.
      labels: [],
    } satisfies Diagnostic,
  ],
})

export const createCompilerClient = (): CompilerClient => {
  const slots: Slot[] = []
  const queue: Job[] = []

  let engine: CompilerEngine = { kind: 'mock', reason: 'still starting up' }
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
   * package is not there we shut the pool down entirely and serve the mock from the main
   * thread — it is a lexical scan, so it costs microseconds and a worker would be waste.
   */
  const ready: Promise<CompilerEngine> = new Promise((resolve) => {
    let probe: Slot
    try {
      probe = spawn()
    } catch (error) {
      engine = {
        kind: 'mock',
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
          kind: 'mock',
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

    if (resolved.kind === 'mock') {
      return kind === 'check' ? mockCheck(source) : mockRun(source)
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
