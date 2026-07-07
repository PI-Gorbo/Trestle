# Tier 00 — Basics

The foundation every later tier builds on. Organised into **houses** of
closely-related programs, ordered roughly by dependency. One concern per program:
e.g. `lambda` and `typed-lambda` are separate files living side by side, not one
omnibus program.

- **literals** — the atoms: `int` (parses today), plus `string`, `bool`, `float`.
- **operators** — `addition`, `multiplication`, and `precedence-and-grouping`
  parse today; `subtraction`, `division`, `negation`, `comparison`, `logical`
  are placeholders awaiting their operators (comparison/logical also need
  booleans).
- **bindings** — `let-declaration` and chained `arithmetic` (with variable
  references) parse today; `typed-let-declaration` waits on the AST carrying a
  type annotation on `let`.
- **functions** — lambdas and application: `lambda`, `typed-lambda`,
  `nested-lambda`, `function-invocation`, `currying`, `partial-application` all
  parse today. `zero-param-lambda` is blocked — the grammar's `lambda` rule
  requires at least one parameter.
- **conditionals** — `if-expression` / `if-else-expression`, awaiting the `if`
  grammar (its `ExpressionKind::If` already exists in `checked.rs`).
