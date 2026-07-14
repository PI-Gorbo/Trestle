# Trestle — State of the Language & Plan Forward

_Last updated: 2026-07-12_

This is the living companion to the [origin spec](../inital-spec/inital-spec.md). The
spec captures the *design intent*; this doc captures *where the implementation actually
is* and *the plan to get from here to the effect system*. Where the two disagree, trust
this doc for status and the spec for the long-term vision.

> **Spec drift to keep in mind:** the origin spec says the implementation language is
> TypeScript and shows a TS token enum. The project was rewritten in **Rust** (pest
> grammar, no separate lexer). The spec also *defers* the dispatch/polymorphism decision;
> this doc takes a position on it (see "Learning 2").

> **Design value — accessibility of vocabulary:** Trestle deliberately prefers plain,
> familiar words over functional-programming jargon. The surface language uses `trait`,
> `impl`/`extend`, `import`, `record`, `core`, and `sat` (satisfies) — **not** `prelude`,
> `functor`, `monad`, or `<:`. The powerful machinery (higher-kinded traits, row
> polymorphism, effect rows) stays under the hood; what the user reads and writes stays
> approachable. This is a design constraint that shapes naming and syntax throughout — it
> is why the standard library is called `core`, not "prelude." See the
> [design-decisions doc](./design-decisions.md#accessibility-principle).

## 1. Where we are today

**Implementation:** Rust, single crate `crates/trestle`. Three-phase pipeline:

```
parse (pest)  →  analyse (resolve names + type check)  →  evaluate (tree-walk)
```

- `parse/` — pest grammar (`trestle.pest`) + AST builders. No separate lexer.
- `analyse/` — pass 1 `resolve_names` (names → `BindingId`), pass 2 `type_check`
  (nominal, annotation-driven; `unify` is currently just type equality).
- `evaluate/` — tree-walk over the analysed AST; `Rc`-linked persistent environment.

**Feature matrix (end-to-end):**

| Feature | Parse | Analyse | Evaluate |
|---|---|---|---|
| Int / Float / Bool / String literals | ✅ | ✅ | ✅ (code) |
| Arithmetic `+ - * /` | ✅ | ✅ (Int only) | ✅ |
| Comparison `< > <= >= == !=` | ✅ | ✅ (**Int only**) | ✅ |
| Logical `&& \|\|`, unary `!`, negation `-` | ✅ | ✅ | ✅ |
| Precedence / grouping | ✅ | ✅ | ✅ |
| `let` bindings, blocks `{…}` | ✅ | ✅ | ✅ |
| `if` / `if-else` | ✅ | ✅ | ✅ (code, see bug below) |
| Lambdas (typed params), closures | ✅ | ✅ | ✅ |
| Function invocation, currying, partial application | ✅ | ✅ | ✅ (code) |

**Important nuance — "eval" is barely rolled out.** The evaluator *code* covers almost
all of tier-00, but the conformance corpus (`tests/corpus.rs`) currently opts only the
**`int` literal** program into the `eval` snapshot stage. Everything else is proven only
through `parse + analyse`. Rolling eval across the corpus is the very next task (§4).

**Not yet supported:** `|>` pipe (tier 01), `match` (tier 02), records / field access /
ADTs (tier 03), generics (tier 04), the effect system (tier 05). Also missing: typed
`let` (`let x: Int = …` — grammar accepts it, AST drops it), zero-param lambdas,
inference for untyped params, comparisons on non-Int types.

**Known issues to clean up:**
- **Dropped `else` branch:** `type_check.rs` type-checks the `else` expression but builds
  the analysed `If` with `else_branch: None`, so a false condition yields `Unit` even when
  an `else` was written. High-priority correctness bug — surfaces the moment `if-else` is
  opted into eval.
- **Stale docs:** comments in `analyse/resolved.rs` and `tests/corpus.rs` claim `if` is
  unsupported / rejected; it is actually resolved, type-checked, and evaluated.
- **`main.rs` is parse-only:** the CLI never analyses or evaluates. You can't yet *run* a
  `.trsl` file end-to-end outside the test harness.

## 2. The three learnings that reshape the plan

### Learning 1 — Build the standard library in Trestle itself (intrinsics + `core`)

`Effect`, `Option`, `Result`, and their combinators (`map`, `flatMap`, `recover`, `ok`,
`fail`) should be **ordinary Trestle definitions**, not Rust built-ins. The interpreter
stays small and provides only what the language genuinely cannot express itself:

- **Intrinsics** — native Rust functions surfaced to Trestle for leaf capabilities the
  language can't implement in pure source (`print`, `readLine`, and eventually the effect
  runtime's actual I/O).
- **The `core` library** — a `.trsl` standard library loaded *before* user code, where the
  rich types live as normal ADTs + functions. (Called `core`, not "prelude" — see the
  accessibility design value above.)

This splits the world into **primitives (Rust)** and **library (Trestle)**:

| Primitive (interpreter, Rust) | Library (`core`, `.trsl`) |
|---|---|
| Literals, closures, control flow | `Option`, `Result` |
| Operators (until traits land — see Learning 2) | `Effect` type + combinators |
| Pattern-match execution | `map` / `flatMap` / `recover` / `ok` / `fail` |
| Intrinsics (`print`, `readLine`) | list / general utilities |
| The effect runtime executor | — |

**Consequence:** a working intrinsics mechanism + `core` loader is a *prerequisite* for
the effect system. It's the gate into Phase 3.

### Learning 2 — Traits for behaviour; operators via traits (explored, not yet committed)

Today operators are hardcoded: `type_check` fixes operands to `Int`, and `eval_binary`
matches on `Value::Int`. That's why `+` and `==` only work on `Int`. The vision is for
`+` to dispatch through a trait (an `Add`-like abstraction) so it works for any type with
an instance.

**Why not now:** trait-based operators require, roughly, the *entire* Phase 2 type system:
1. syntax + AST for declaring traits and instances,
2. instance resolution (and a coherence story) in the checker,
3. type variables / generics so trait methods are polymorphic,
4. a runtime dispatch strategy (dictionary-passing or runtime-type lookup).

Retrofitting operators is the *natural payoff* of that work, not a detour to take first.

**Recommendation — defer, but design for it:**
- Keep operator logic **centralized** (it already is: one `eval_binary`, one operator arm
  in `type_check`) so there is a single seam to swap later.
- **Don't** take the tempting middle path of hand-expanding operators to a few concrete
  types (Int + Float) — that's throwaway work the trait system replaces wholesale. Live
  with Int-only comparisons/arithmetic until traits exist.
- In Phase 2, build **type classes / traits as the capstone** of the type system, then
  retrofit `+ - * /` onto `Add`/`Sub`/…, and `< == …` onto `Ord`/`Eq`. The current
  "comparisons are Int-only" limitation dissolves for free at that point.

**Design exercise — now decided.** The trait model is settled: **nominal, Rust-shaped
traits** (`impl … for T`, `self`, dot-call) with global coherence, but with **higher-kinded
(type-constructor) trait parameters** so `Functor`/`Monad` are one shared abstraction — a
deliberate step past Rust's trait kinds. Operators retrofit onto `core` instances of
`Add`/`Eq`/`Ord`, so the Int-only limit dissolves. Structural typing is used for *records*
(via row polymorphism), **not** for behaviour/dispatch. Full rationale and the type-classes
vs traits vs structural comparison live in the
[design-decisions doc](./design-decisions.md#dispatch-model); the recorded summary is in §5.

### Learning 3 — Ordering

The two learnings above pin the order: **traits are a Phase 2 capstone**, and **Effect is
a Phase 3 library** that can't be written until ADTs + generics + traits + the
intrinsics/`core` mechanism all exist. So the sequence is not "jump to effects" — it's
"finish the interpreter, then grow the type system with traits, then Effect falls out as a
library." The roadmap in §3 makes this concrete.

## 3. Revised roadmap

The origin spec's four phases still hold; this refines the interior ordering to serve the
three learnings.

### Phase 1 — Interpreter core *(current — finishing)*
Get real programs running end to end.
1. Roll `eval` across the whole `00-basics` corpus (only `int` is wired today).
2. Fix the known issues (dropped `else` branch; stale docs; wire `main.rs` to
   analyse + evaluate so `trestle run file.trsl` actually executes).
3. `|>` pipe operator + leading-pipe continuation (tier 01). Dumb desugar: `x |> f ≡ f(x)`.
4. Close small gaps: zero-param lambdas, typed `let`.

### Phase 2 — Type system *(the enabling layer for the whole vision)*
1. **Real inference:** turn `unify` into actual unification, add a `Type::Var` variant,
   infer unannotated params (removes today's `MissingAnnotation` friction).
2. **ADTs + records + `match`** (tiers 02–03).
3. **Generics / type parameters** (tier 04).
4. **Traits / type classes** — resolve the dispatch model, then **retrofit operators**
   (Learning 2). This is the capstone that makes operators polymorphic.

### Phase 2.5 — Bootstrapping infrastructure *(prerequisite for Phase 3)*
1. An **intrinsics** mechanism (native functions callable from Trestle).
2. A **`core` library** loaded before user code (the `core` loader is the module system's
   implicit-import special case — see §5).
3. Move `Option` / `Result` into `core` as the first library types.

### Phase 3 — Effects *(the heart of Trestle)*
1. Define **`Effect` in-language** as an ADT + combinators in `core` (Learning 1).
2. `effect { }` block desugars to `flatMap` chains over that type.
3. **Full E/R tracking** in the type system: errors (`E`) and requirements (`R`)
   propagate upward, accumulating until the runtime satisfies them (the spec's DI /
   railway model). This is the ambition — advanced, and gated on the real inference +
   generics from Phase 2.
4. Runtime **executor** + `main` as an effect; the runtime is the single boundary where
   all requirements are met and all errors handled.

### Phase 4 — TypeScript transpiler *(optional, unchanged)*

## 4. Near-term task list (concrete)

Ordered, actionable, for the current push:

1. [ ] Opt each `00-basics` program into the `eval` stage in `tests/corpus.rs`, record
       snapshots, and confirm they match hand-computed values.
2. [ ] Fix the **dropped `else` branch** in `analyse/type_check.rs` (carry the analysed
       else expression into `ExpressionKind::If`); add an `if-else` eval snapshot that
       would have caught it.
3. [ ] Correct the stale "`if` is unsupported/rejected" comments in `analyse/resolved.rs`
       and `tests/corpus.rs`.
4. [ ] Make `main.rs` run the full pipeline (`parse → analyse → evaluate`) so `.trsl`
       files execute from the CLI.
5. [ ] Implement the `|>` operator + leading-pipe continuation; un-ignore tier-01
       programs.
6. [ ] (Small) zero-param lambdas and typed `let` bindings.

## 5. Design directions (recorded)

The four questions that were open here are now **decided in direction** (not yet in full
spec) through a design pass. Short form below; the companion
[design-decisions doc](./design-decisions.md) holds the rationale, tradeoffs, and reading
lists. Everything here honours the accessibility design value (plain words, jargon hidden).

- **Dispatch model — nominal traits, Rust-shaped.** `trait` + `impl … for T` (multiple
  impls per type; co-located with the type or separate), generic traits and methods,
  `self`, dot-call; an impl *proves conformance* (more than C# extension methods). Global
  coherence; implicit, type-directed resolution. The trait system allows **higher-kinded
  (type-constructor) parameters** so `Functor`/`Monad` are one shared abstraction — a
  deliberate step past Rust. Associated types deferred (generic parameters first).
  Operators become `core` instances of `Add`/`Eq`/`Ord`, retiring the Int-only limit
  (§Learning 2). → [dispatch](./design-decisions.md#dispatch-model).
- **Data & types — structural records, nominal variants, row-powered.** Records are
  structural on a **row-polymorphism** engine: `{ name: T, ... }` = "at least these fields"
  (three-dot `...`, mirroring value-spread `{ ...rec, name: v }`); `{ name: T }` = exactly
  these. Bounds use **`sat`** (satisfies), never `<:`, covering both trait bounds
  (`T sat Show`) and structural bounds (`T sat { name: T, ... }`); a *named* bound binds the
  row variable once, so a function can return the same open record it received
  (`rename<T, R sat { name: T, ... }>(r: R, n: T): R`). No implicit coercion between two
  distinct bounded variables. Variants are nominal ADTs. Inference is real Hindley–Milner
  unification with `Type::Var` **and** row variables — one engine for records, open records,
  and effect `E`/`R`. → [records & rows](./design-decisions.md#records-rows-and-sat-bounds).
- **Effects — row-based, staged.** `E`/`R` are rows that accumulate up the call graph and
  are discharged at the runtime/platform boundary, surfaced via `effect { }` (raw row
  syntax stays hidden). Staged: value `A` first, then `E`, then `R`, as real inference
  lands. Set-theoretic / semantic-subtyping unions are the north star, not the first cut.
  → [effects](./design-decisions.md#effects).
- **Modules & `core` — Roc-style platform + hybrid imports.** Opinionated workspace of
  packages; **runnable = the package provides a `main`** (intrinsic to the package, not a
  config flag). The platform/config selects which `core` is in scope. A small opinionated
  `core` is auto-imported; everything else is explicit per-file `import`. The `core` loader
  is the module system's implicit-import special case (one workstream, see Phase 2.5).
  → [modules & core](./design-decisions.md#modules-and-core).

**Still open (sub-questions):**
- Row-polymorphism details: record-update semantics, scoped vs unscoped labels, exact `sat`
  grammar for combined bounds (e.g. `Show + { … }`).
- HKT surface: how kinds are written/inferred when a trait is parameterized by a
  constructor, kept approachable.
- Effect `E`/`R` staging: the precise first-cut scope.
- Coherence enforcement mechanics for a runtime-dispatched interpreter.
