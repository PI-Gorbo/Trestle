# Trestle program corpus

A tiered suite of `.trsl` programs that test the compiler as the language grows.
The harness lives in [`../corpus.rs`](../corpus.rs); run it with:

```sh
cargo test -p trestle                             # every program
cargo test -p trestle basics_operators_addition   # a single program
cargo test -p trestle -- --ignored                # the not-yet-supported programs
```

## How it works

Programs are **not** auto-discovered — each is registered with one `trsl_test!`
line in [`../corpus.rs`](../corpus.rs). Every program lives in its own directory
so its snapshots sit beside it:

```text
00-basics/operators/addition/
  addition.trsl        the source
  addition.ast.snap    parse() -> ast::Program   (recorded by insta)
```

Each registered program parses its source and snapshots the resulting AST via
`insta`, so every program reports pass/fail individually. A program whose feature
isn't implemented *yet* is still registered but marked ignored, with a reason:

```rust
trsl_test!(basics_operators_subtraction,
    "00-basics/operators/subtraction/subtraction.trsl",
    ignore = "needs the subtraction operator (-)");
```

When you implement the feature, delete the `ignore = "…"` argument and the program
joins the must-parse set (its AST snapshot is recorded on the next run). The
shrinking ignore list *is* the remaining roadmap.

## Organisation — complexity & dependencies

Programs are tiered so each tier only depends on earlier ones. **One concern per
program**; related concerns are co-located rather than merged into one file.

| Tier | Focus |
|------|-------|
| `00-basics` | the foundation — literals, operators, bindings, functions, conditionals |
| `01-pipelines` | the `\|>` operator and leading-pipe continuation |
| `02-control-flow` | `match` / pattern matching *(proposed syntax)* |
| `03-records-and-adts` | records, `.` field access, algebraic data types *(proposed)* |
| `04-generics` | type parameters, generic functions and data types *(proposed)* |
| `05-effects` | `effect { }`, railway errors, `main` as an effect *(proposed)* |

`00-basics` is split into **houses**, each a folder of closely-related programs
(see [`00-basics/README.md`](00-basics/README.md)). For example `lambda` and
`typed-lambda` are two separate programs living together under `functions/`.

> Where the language hasn't pinned syntax yet (tiers 02–05), programs use a
> **proposed** syntax — treat them as design prompts, not settled decisions.
