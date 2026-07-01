# Tier 02 — Functions

**Status: pending** (all files `@skip`).

## Covers
- Arrow functions: `(x) => expr`
- Multi-parameter arrows as sugar: `(a, b) => body` ≡ `(a) => (b) => body`
- Curried application: `f(a, b)` ≡ `f(a)(b)`
- Partial application ("stop early"): `add(10)` awaits the remaining argument

## Prerequisites
Tier 01.

## To un-skip
Grammar + AST need: a `lambda`/arrow expression, a function-application form
(`callee(args)`), and desugaring of multi-arg arrows/calls into nested
single-arg forms. Delete each file's `// @skip:` line as it starts parsing.
