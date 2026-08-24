/**
 * The mock compiler.
 *
 * Used only when the WebAssembly package is missing — before `pnpm build:wasm` has been run,
 * or while `crates/trestle` itself does not compile. It exists so the whole app is usable
 * and testable without a Rust toolchain in the loop.
 *
 * The rule it follows: **never invent compiler output**. Everything below is a real lexical
 * property of the source that can be decided without a parser, so a diagnostic it reports is
 * genuinely a diagnostic. It simply sees far less than the real compiler — it knows nothing
 * about names, scopes or types. `run` refuses outright rather than fabricating a value, and
 * every code is namespaced `mock::` so it is obvious in the UI where it came from.
 */

import type { Diagnostic, Label, RunResult, CheckResult } from './types'

/**
 * Every character the pest grammar can consume, outside a string literal or comment.
 * Anything else is unambiguously a syntax error — most usefully the `;` that people reach
 * for by reflex, since Trestle delimits statements structurally.
 */
const LEGAL_SOURCE_CHARACTER = /[\w"'.,:(){}+\-*/&|=!<>\s]/

const OPENING_BRACKETS: Record<string, string> = { '(': ')', '{': '}' }
const CLOSING_BRACKETS = new Set([')', '}'])

type Position = { line: number; column: number }

/**
 * A cursor over the source that tracks its own line and column. JS string indices are
 * already UTF-16 code units, which is exactly what Monaco counts, so the column needs no
 * conversion — unlike the Rust side, where offsets are bytes.
 */
const positionsOf = (source: string): Position[] => {
  const positions: Position[] = []
  let line = 1
  let column = 1

  for (const character of source) {
    // `for..of` iterates code points, so an astral character advances the column by its two
    // UTF-16 units and occupies two entries.
    for (let unit = 0; unit < character.length; unit += 1) {
      positions.push({ line, column: column + unit })
    }
    if (character === '\n') {
      line += 1
      column = 1
    } else {
      column += character.length
    }
  }

  positions.push({ line, column })
  return positions
}

const labelAt = (
  positions: Position[],
  message: string | null,
  offset: number,
  length: number,
): Label => {
  const start = positions[Math.min(offset, positions.length - 1)]!
  const end = positions[Math.min(offset + length, positions.length - 1)]!

  return {
    message,
    startLine: start.line,
    startColumn: start.column,
    endLine: end.line,
    endColumn: end.column,
    offset,
    length,
  }
}

const syntaxError = (code: string, message: string, label: Label): Diagnostic => ({
  phase: 'parse',
  severity: 'error',
  code,
  message,
  help: 'Reported by the mock compiler, which only checks lexical structure.',
  labels: [label],
})

/**
 * A single left-to-right scan. Comments and string literals are skipped as units, so a brace
 * inside `"{"` or after `//` never disturbs the bracket stack.
 */
const scan = (source: string): Diagnostic[] => {
  const positions = positionsOf(source)
  const diagnostics: Diagnostic[] = []
  const brackets: { character: string; offset: number }[] = []

  let index = 0
  while (index < source.length) {
    const character = source[index]!

    if (character === '/' && source[index + 1] === '/') {
      const newline = source.indexOf('\n', index)
      index = newline === -1 ? source.length : newline
      continue
    }

    if (character === '"') {
      const start = index
      index += 1
      let terminated = false
      while (index < source.length) {
        if (source[index] === '\\') {
          index += 2
          continue
        }
        if (source[index] === '"') {
          terminated = true
          index += 1
          break
        }
        // The grammar's `string_character` accepts any non-quote character including a
        // newline, so an unterminated string swallows the rest of the file. Reporting it at
        // the opening quote is far more useful than at EOF.
        index += 1
      }
      if (!terminated) {
        diagnostics.push(
          syntaxError(
            'mock::unterminated_string',
            'this string literal is never closed',
            labelAt(positions, 'opened here', start, 1),
          ),
        )
      }
      continue
    }

    if (character in OPENING_BRACKETS) {
      brackets.push({ character, offset: index })
    } else if (CLOSING_BRACKETS.has(character)) {
      const open = brackets.pop()
      if (!open) {
        diagnostics.push(
          syntaxError(
            'mock::unmatched_bracket',
            `\`${character}\` closes nothing`,
            labelAt(positions, 'no matching opener', index, 1),
          ),
        )
      } else if (OPENING_BRACKETS[open.character] !== character) {
        diagnostics.push(
          syntaxError(
            'mock::mismatched_bracket',
            `expected \`${OPENING_BRACKETS[open.character]}\` to close \`${open.character}\`, found \`${character}\``,
            labelAt(positions, 'mismatched here', index, 1),
          ),
        )
      }
    } else if (!LEGAL_SOURCE_CHARACTER.test(character)) {
      diagnostics.push(
        syntaxError(
          'mock::unexpected_character',
          `\`${character}\` is not valid Trestle${character === ';' ? ' — statements are delimited structurally, not by semicolons' : ''}`,
          labelAt(positions, 'unexpected', index, 1),
        ),
      )
    }

    index += 1
  }

  for (const unclosed of brackets) {
    diagnostics.push(
      syntaxError(
        'mock::unclosed_bracket',
        `\`${unclosed.character}\` is never closed`,
        labelAt(positions, 'opened here', unclosed.offset, 1),
      ),
    )
  }

  return diagnostics
}

export const mockCheck = (source: string): CheckResult => {
  const diagnostics = scan(source)
  // No bindings: the mock does not resolve names, and reporting an empty list is honest —
  // it never claims a program has no bindings, only that it found none to report.
  return diagnostics.length > 0 ? { ok: false, diagnostics } : { ok: true, bindings: [] }
}

export const mockRun = (source: string): RunResult => {
  const diagnostics = scan(source)
  if (diagnostics.length > 0) return { ok: false, diagnostics }

  return {
    ok: false,
    diagnostics: [
      {
        phase: 'evaluate',
        severity: 'advice',
        code: 'mock::cannot_evaluate',
        message: 'The mock compiler cannot evaluate programs.',
        help: 'Build the real compiler with `pnpm build:wasm`, then reload.',
        labels: [],
      },
    ],
  }
}
