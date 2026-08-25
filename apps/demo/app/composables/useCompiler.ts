/**
 * Compiler state, one entry per open program.
 *
 * The client (and so the worker pool) is module-scoped: one pool serves every tab, which is
 * the point of pooling. Per-program results live in a map keyed by program id so switching
 * tabs is instant and a background compile lands on the right tab.
 */

import { createCompilerClient, type CompilerClient } from '@/lib/compiler/client'
import type {
  Binding,
  CompileKind,
  CompilerEngine,
  Diagnostic,
} from '@/lib/compiler/types'

export type CompileState = {
  status: 'idle' | 'compiling' | 'done'
  diagnostics: Diagnostic[]
  bindings: Binding[]
  /** The evaluated result, set only by `run` and only when it succeeded. */
  value: string | null
  valueType: string | null
}

const idleState = (): CompileState => ({
  status: 'idle',
  diagnostics: [],
  bindings: [],
  value: null,
  valueType: null,
})

const engine = ref<CompilerEngine>({ kind: 'unavailable', reason: 'still starting up' })
const states = reactive(new Map<string, CompileState>())

/**
 * The compile generation per program. A result whose generation is stale — because the user
 * typed again while it was in flight — is discarded rather than rendered, so markers never
 * describe source that is no longer on screen.
 */
const generations = new Map<string, number>()

let client: CompilerClient | null = null

const ensureClient = (): CompilerClient => {
  if (client) return client

  client = createCompilerClient()
  void client.ready.then((resolved) => {
    engine.value = resolved
  })
  return client
}

export const useCompiler = () => {
  if (import.meta.client) ensureClient()

  const stateFor = (id: string): CompileState => {
    const existing = states.get(id)
    if (existing) return existing

    const fresh = idleState()
    states.set(id, fresh)
    return fresh
  }

  const compile = async (id: string, kind: CompileKind, source: string) => {
    if (import.meta.server) return

    const generation = (generations.get(id) ?? 0) + 1
    generations.set(id, generation)

    const state = stateFor(id)
    state.status = 'compiling'

    const result = await ensureClient().compile(kind, source)

    // Superseded while in flight.
    if (generations.get(id) !== generation) return

    const next = stateFor(id)
    next.status = 'done'

    if (result.ok) {
      next.diagnostics = []
      next.bindings = result.bindings
      // `check` has no value to report; leave whatever the last `run` produced rather than
      // blanking the output panel on every keystroke.
      if ('value' in result) {
        next.value = result.value
        next.valueType = result.valueType
      }
      return
    }

    next.diagnostics = result.diagnostics
    if (kind === 'run') {
      next.value = null
      next.valueType = null
    }
  }

  const forget = (id: string) => {
    states.delete(id)
    generations.delete(id)
  }

  return {
    engine: readonly(engine),
    stateFor,
    compile,
    check: (id: string, source: string) => compile(id, 'check', source),
    run: (id: string, source: string) => compile(id, 'run', source),
    forget,
  }
}
