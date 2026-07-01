# Trestle program corpus

A tiered suite of `.trsl` programs used to test the parser (and later the
interpreter) as the language grows. The harness lives in
[`../tests/corpus.rs`](../tests/corpus.rs); run it with:

```sh
cargo test -p trestle-tests                  # pass/fail
cargo test -p trestle-tests -- --nocapture   # + the parsed/skipped checklist
```

## How it works

Every `.trsl` file here is **expected to parse**. A file that the parser can't
handle *yet* opts out with a directive on any comment line:

```trestle
// @skip: needs arrow functions (tier 02)
```

`//` is grammar trivia, so the directive never affects parsing — the harness
reads it from the raw text. **When you implement the feature, delete the
`@skip` line** and the file joins the must-parse set. The shrinking skip list
*is* the remaining roadmap.

## Tiers (easy → hard, working backwards from the effect system)

| Tier | Folder | Focus |
|------|--------|-------|
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
