# Tier 04 — Values & types

**Status: pending** (all files `@skip`).

Fills out the primitive literals the Phase-1 token set deliberately left out
(strings, floats, booleans, negatives) plus the operators over them.

## Covers
- String literals
- Boolean literals `true` / `false`
- Float literals
- Negative numbers
- Comparison operators (`==`, `<`, `>`, …) and boolean operators (`&&`, `||`, `!`)

## Prerequisites
Tier 01. Independent of functions/pipelines, but listed here because these are
still "just values."

## To un-skip
Lexer/grammar need new literal forms and operator tokens; AST needs `Str`,
`Bool`, `Float` variants and comparison/boolean operator nodes. Decide operator
precedence relative to `+`/`*`.

> Note: the spec's Phase-1 token enum intentionally excludes these — treat their
> exact syntax as still open where it isn't obvious.
