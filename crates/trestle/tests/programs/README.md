# Trestle program corpus

A tiered suite of `.trsl` programs used to test the parser (and later the
interpreter) as the language grows. The harness lives in
[`../corpus.rs`](../corpus.rs); run it with:

```sh
cargo test -p trestle                     # every program, one test each
cargo test -p trestle basics_arithmetic   # a single program
cargo test -p trestle -- --ignored        # the not-yet-supported programs
```

## How it works

Each `.trsl` file gets its own `#[test]`, wired up by a single `trsl_test!` line
in [`../corpus.rs`](../corpus.rs). Each test parses the program and snapshots its
AST (via `insta`), so every program reports pass/fail individually.

Every file here is **expected to parse**. A file that the parser can't handle
*yet* is registered but marked ignored, by giving its macro line a reason:

```rust
trsl_test!(functions_arrow_functions, "02-functions/arrow-functions.trsl",
    ignore = "arrow-function walker not implemented yet (tier 02)");
```

An ignored test shows as `... ignored` in the report (not silently passed).
**When you implement the feature, delete the `ignore = "…"` argument** and the
program joins the must-parse set (run `cargo insta accept` to record its AST
snapshot). The shrinking ignore list *is* the remaining roadmap.

## Tiers (easy → hard, working backwards from the effect system)

| Tier | Folder | Focus |
|------|--------|-------|
| 00 | `00-primitives` | one construct per file — int, let, add, mul, lambda, call, typed lambda — the evaluator's atoms |
| 01 | `01-basics` | let bindings, integer arithmetic, precedence, comments — **parses today** |
| 02 | `02-functions` | arrow functions, currying, partial application, curried calls |
| 03 | `03-pipelines` | the `\|>` operator, leading-pipe continuation, chaining |
| 04 | `04-values-and-types` | strings, booleans, floats, negatives, comparison/boolean ops |
| 05 | `05-control-flow` | `if`/`else` expressions, `match` / pattern matching *(syntax proposed)* |
| 06 | `06-records-and-adts` | records, `.` field access, algebraic data types *(syntax proposed)* |
| 07 | `07-generics` | type parameters, generic functions, higher-order data types |
| 08 | `08-effects` | `Effect<V,E,R>`, `effect { }`, railway errors, DI, `main` as an effect |

Each tier folder has a `README.md` describing the goal, its prerequisites, and
what to build to un-skip it.

> Where the language spec hasn't pinned syntax yet (tiers 05–08), the stubs use
> a **proposed** syntax marked as such — treat those files as design prompts,
> not settled decisions.
