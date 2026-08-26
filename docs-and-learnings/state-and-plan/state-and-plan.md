# Trestle — State of the Language & Plan Forward

_Last updated: 2026-08-26_

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

_Section last reconciled against `crates/trestle/tests/corpus.rs`: 2026-08-26._

**Implementation:** Rust, single crate `crates/trestle`. Four-phase pipeline:

```
parse (pest)  →  binding_resolution  →  type_check  →  evaluate
```

- `parse/` — pest grammar (`trestle.pest`) + AST builders. No separate lexer.
- `binding_resolution/` — names → `BindingId`, scopes, shadowing. Values and types live in
  separate namespaces.
- `type_check/` — real Hindley–Milner: union-find `unification` with a genuine occurs check,
  bottom-up `inference`, `substitution`.
- `evaluate/` — tree-walk over the type-checked AST; `Rc`-linked persistent environment.

**The feature matrix lives in the corpus, not here.** `crates/trestle/tests/corpus.rs` is
the authoritative ledger and cannot drift: a program registered there *without* an `ignore =
"…"` parses, analyses **and** evaluates, and a recorded snapshot proves each stage. A prose
table here would go stale the way the one it replaced did. Read `corpus.rs` top to bottom
for what works; read its `ignore` reasons for what does not.

At the time of writing that is 195 passing stage-tests over 85 programs, with 17 ignored.
Shipping, in tiers: all five literal forms plus records; the full operator set; `let` and
typed `let`; blocks, block scoping and shadowing; `if` / `if`-`else` with block branches;
lambdas (typed, untyped, zero-parameter, with return annotations), currying, partial
application and closures; the `|>` pipe with leading-pipe continuation; `type` aliases
(structural, chaining); record types, record literals, field access and `.` chains, and
records nested by naming the inner type; inference of unannotated parameters and unannotated
records.

`apps/demo` — the playground — carries a curated example for every one of those, lifted
verbatim from the corpus, plus a Features dialog that states the shipped/planned split from
the same source. If a feature ships and the playground does not show it, that is now a
visible gap.

**Not yet supported** (each has an ignored corpus program naming its blocker): function type
expressions, both as a `type` right-hand side and in annotations; *inline* record type
expressions in field position; mixed postfix chaining (`a.b().c`); `match` (tier 02); ADTs
(tier 03); generics (tier 04); the effect system (tier 05).

**Known limits that are deliberate, not bugs:**
- **Operators are hardcoded to `Int`.** Arithmetic and *all* comparisons — `==` and `!=`
  included — are `Int` against `Int`; `&&` / `||` are `Bool`. This dissolves when operators
  retrofit onto `Add` / `Eq` / `Ord` instances (see Learning 2); do not hand-expand it
  first.
- **`&&` / `||` do not short-circuit.** `eval_binary` evaluates both operands. (Some corpus
  comments still describe them as short-circuiting; the evaluator is the truth.)
- **Inference is monomorphic** — no let-generalization, so `let id = (x) => x` cannot be
  used at two types. Generalization arrives with generics (Phase 2.3).
- **`let` is not recursive.** A binding's value resolves before its own name binds, so a
  function cannot yet call itself; mutual recursion needs the same pre-registration.
- **Records unify on exactly equal field sets.** Width subtyping waits on row polymorphism.

**Still to clean up:**
- **`main.rs` is parse-only:** the CLI never analyses or evaluates. You cannot yet *run* a
  `.trsl` file end to end outside the test harness and the wasm playground.
- **Stale `// @skip:` markers** litter corpus `.trsl` files for features that now work
  (`if`, `|>`, string/bool/float literals, zero-parameter lambdas). Trust `corpus.rs`, not
  the markers. The playground strips them when it lifts a program.
- **Stale tier READMEs** under `tests/programs/` say the same about `if`, typed `let` and
  zero-parameter lambdas.

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

### Phase 1 — Interpreter core *(essentially done)*
Get real programs running end to end.
1. ✅ `eval` rolled across the corpus, not just the `int` literal.
2. ✅ Dropped `else` branch fixed (`58bbbb7`).
3. ✅ `|>` pipe operator + leading-pipe continuation (tier 01), the dumb `x |> f ≡ f(x)`
   desugar.
4. ✅ Zero-parameter lambdas and typed `let`.
5. ⬜ Wire `main.rs` to the full pipeline so `trestle run file.trsl` executes outside the
   test harness and the playground. The one item still open.

### Phase 2 — Type system *(current — the enabling layer for the whole vision)*
1. ✅ **Real inference:** `unify` is genuine union-find unification with `Type::Var` and an
   occurs check, and unannotated parameters infer. Still monomorphic — generalization comes
   with generics, at step 3.
2. **Records ✅, ADTs + `match` ⬜** (tiers 02–03). Records, field access, nested records and
   `type` aliases ship; what remains is inline record types in field position, mixed postfix
   chaining, and then sum types with pattern matching.
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

Ordered, actionable, for the current push. Everything the previous edition of this list held
is done; what follows is what is actually left.

1. [ ] Make `main.rs` run the full pipeline (`parse → binding_resolution → type_check →
       evaluate`) so `.trsl` files execute from the CLI. The last Phase 1 item.
2. [ ] Sweep the stale `// @skip:` markers out of the corpus `.trsl` files and correct the
       tier `README.md`s — both still describe `if`, `|>`, typed `let` and zero-parameter
       lambdas as unsupported.
3. [ ] Correct the `operators/logical/*` comments that claim `&&` / `||` short-circuit.
       Either make them short-circuit in `eval_binary` or say plainly that they do not.
4. [ ] Inline record type expressions in field position, then mixed postfix chaining
       (`a.b().c`) — the two smallest ignored programs, and both tier-03 blockers.
5. [ ] Function type expressions, which unblock five ignored programs. The syntax is
       decided: `=>` mirroring lambdas, with the parameter name **optional** because it is
       documentation, not part of the type — so `(n: Int) => Int` and `(Int) => Int` are the
       same type, a multi-parameter form curries as a multi-parameter lambda does, and
       `() => Int` is the nullary case. It all maps onto the existing
       `Type::Fn(Option<Box<Type>>, Box<Type>)`, so the blocker is `type_expression` in the
       grammar, not the checker. Two positions, and they are separable: as a `type`
       right-hand side (`00-basics/type-declarations/function-type*`, three programs), and in
       *annotation* position, where `type_declaration = { ":" ~ identifier }` has to widen to
       a full `type_expression` (`function-typed-parameter`, `record-function-field`).
6. [ ] ADTs + constructors + `match` (tiers 02–03).

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
