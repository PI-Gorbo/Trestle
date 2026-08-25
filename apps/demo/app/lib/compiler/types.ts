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
 * traps or times out, or when the compiler was never built — so a failure of the compiler
 * itself still reaches the user as a diagnostic rather than as silence.
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
  /**
   * `miette`'s own rendering — source excerpt, caret art, every label — as plain text, exactly
   * what the CLI prints for the same program. The `labels` above are what the editor draws;
   * this is what the diagnostics panel prints.
   *
   * `null` only for the diagnostics this app synthesises itself: a trap or a timeout has no
   * source position and no compiler rendering to show. The compiler always sends a string.
   */
  render: string | null
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
 * Whether the compiler is usable. There is no second implementation to fall back to — the
 * playground either runs the real thing or says why it cannot, which is the only honest pair
 * of states for a page whose entire purpose is to show what the compiler does.
 */
export type CompilerEngine =
  | { kind: 'wasm'; version: string }
  | { kind: 'unavailable'; reason: string }

export type WorkerRequest =
  | { id: number; kind: 'init' }
  | { id: number; kind: CompileKind; source: string }

export type WorkerResponse =
  /** `version` is null when the WebAssembly package is absent — nothing has been built. */
  | { id: number; outcome: 'ready'; version: string | null }
  | { id: number; outcome: 'result'; result: CompileResult }
  /**
   * The compiler trapped. A panicked wasm instance is poisoned for good, so the receiving
   * client must terminate this worker rather than reuse it.
   */
  | { id: number; outcome: 'panic'; message: string }
