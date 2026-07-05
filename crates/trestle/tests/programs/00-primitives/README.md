# Tier 00 — Primitives

**Status: partly implemented.** `let-declaration.trsl` parses today. The rest are
single **bare expressions** and stay ignored until the grammar accepts a top-level
expression as a statement (`statement = let_binding | expr`).

One construct per file, deliberately minimal — the atoms the evaluator is built on,
in a sensible order to tackle them.

## Covers (one file each)
- `int.trsl` — an integer literal → `Value::Int`
- `let-declaration.trsl` — a `let` binding; extends the environment
- `addition.trsl` — `Expr::Add`
- `multiplication.trsl` — `Expr::Mul`
- `lambda.trsl` — the simplest lambda → a closure capturing its environment
- `function-invocation.trsl` — define a function, then call it by name
- `typed-lambda.trsl` — typed params + return type (parsed, not yet checked)

## Prerequisites
Bare top-level expressions. Today `program = statement*` and `statement =
let_binding`, so only `let-declaration.trsl` parses. Add `expr` as a statement form
(and an AST `Statement` that is either a `Let` or an `Expr`) to un-ignore the rest —
their inner walkers already exist in `src/ast/build_expression.rs`.
