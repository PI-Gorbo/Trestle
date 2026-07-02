# Tier 03 — Pipelines

**Status: pending** (all files `@skip`).

## Covers
- The pipe operator `|>` — "dumb": `x |> f` ≡ `f(x)`
- Leading-pipe continuation: a line beginning with `|>` continues the previous
  expression (Kotlin-style leading `.`), so chains need no semicolons
- Composing curried, data-last functions into a pipeline

## Prerequisites
Tier 02 (currying/partial application) — the pipe gets its expressiveness from
currying, not from operator magic.

## To un-skip
Grammar needs a `|>` binary operator and the newline-continuation rule
(continue when the next non-blank line begins with `|>`). AST needs a pipe node
(or desugaring `x |> f` directly into application `f(x)`).
