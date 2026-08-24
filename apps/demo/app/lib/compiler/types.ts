/**
 * The wire format between the Trestle compiler and this app.
 *
 * Mirrors `crates/trestle-wasm/src/dto.rs` by hand — there is no codegen step, so a change
 * on either side needs the matching change here. The result types are discriminated unions
 * on `ok` so callers can match exhaustively.
 */

/**
 * Which pipeline stage produced a diagnostic. Trestle fails fast, so a batch is one phase.
 *
 * `internal` is the one value the Rust side never emits: it is synthesised here when a worker
 * traps or times out, so a compiler crash still reaches the user as a diagnostic rather than
 * as silence.
 */
export type Phase = 'parse' | 'resolve' | 'typecheck' | 'evaluate' | 'internal'

export type Severity = 'error' | 'warning' | 'advice'

/**
 * A span of source. Line and column are 1-based, and the column counts UTF-16 code units —
 * which is both what Monaco's `IRange` wants and what a JS string index already is.
 */
export type SourceRange = {
  startLine: number
  startColumn: number
  endLine: number
  endColumn: number
}

/** One highlighted range within a diagnostic. */
export type Label = SourceRange & {
  message: string | null
  /** The original byte offset and length from the compiler, kept for debugging. */
  offset: number
  length: number
}

export type Diagnostic = {
  phase: Phase
  severity: Severity
  /** The compiler's diagnostic code, e.g. `trestle::unbound_name`. */
  code: string | null
  message: string
  help: string | null
  labels: Label[]
}

/** A top-level binding and the type inference settled on for it. */
export type Binding = SourceRange & {
  name: string
  type: string
}

export type CheckResult =
  | { ok: true; bindings: Binding[] }
  | { ok: false; diagnostics: Diagnostic[] }

export type RunResult =
  | { ok: true; value: string; valueType: string; bindings: Binding[] }
  | { ok: false; diagnostics: Diagnostic[] }

export type CompileKind = 'check' | 'run'

export type CompileResult = CheckResult | RunResult

/**
 * Which compiler actually answered. Surfaced in the header so mock output is never mistaken
 * for the real thing.
 */
export type CompilerEngine =
  | { kind: 'wasm'; version: string }
  | { kind: 'mock'; reason: string }

export type WorkerRequest =
  | { id: number; kind: 'init' }
  | { id: number; kind: CompileKind; source: string }

export type WorkerResponse =
  /** `version` is null when the WebAssembly package is absent — the app then uses the mock. */
  | { id: number; outcome: 'ready'; version: string | null }
  | { id: number; outcome: 'result'; result: CompileResult }
  /**
   * The compiler trapped. A panicked wasm instance is poisoned for good, so the receiving
   * client must terminate this worker rather than reuse it.
   */
  | { id: number; outcome: 'panic'; message: string }
