# Plan — Trestle Phase 2, Step 1: Real (monomorphic) type inference

## Context

Trestle's interpreter has finished Phase 1 (literals, arithmetic, closures/currying, the
`|>` pipe). The next phase is the type system, and the first move — per
[state-and-plan.md](../../docs-and-learnings/state-and-plan/state-and-plan.md) §Phase 2.1 —
is **real type inference**: turn today's stub `unify` into actual unification and let
unannotated lambda params be inferred.

Today the checker is a facade of inference:
- `unify` (`crates/trestle/src/analyse/type_check.rs:425`) is **pure `PartialEq`** — no
  variables, no substitution threaded anywhere. `TypeEnv` (`type_check.rs:34`) is just
  `Vec<Option<Type>>` keyed by `BindingId` — not a solver.
- `Type::Var()` (`crates/trestle/src/analyse/analysed.rs:25`) is a **no-op stub** — carries
  no id and is never constructed.
- Lambda params are **forced to carry annotations** (`build_param`,
  `crates/trestle/src/parse/build_expression.rs:197`, rejects untyped with
  `MissingParamType`), so **no code path today needs a type variable**.

**The decision (confirmed):**
1. **Rows/records are deferred.** Row polymorphism is an *additive* extension of HM
   unification (a `Type::Row` variant + a row arm in the same `unify`), and nothing needs
   it until records exist. It arrives with the records + `match` milestone, not now. The
   discipline this step must honor: build `unify` as a *real substitution-based unifier* so
   rows drop in later without a rewrite.
2. **First cut is monomorphic** — no let-generalization. A let-bound function is usable at
   one type per scope; `let id = x => x; id(1); id(true)` is intentionally a type error.
   Generalization (implicit polymorphism) becomes a clean fast-follow.

**Intended outcome:** `(x) => x + 1` infers `x: Int` with no annotation; `(a, b) => a + b`
infers both `Int`; the `MissingParamType` friction is gone; and `unify` is a genuine
occurs-check + substitution engine that rows and generalization can build on.

## Out of scope (explicitly)

- Records / rows / `Type::Row` — the next milestone (records + `match`).
- Let-generalization / type schemes (`forall`) — the fast-follow after this.
- Explicit generic surface syntax (`<T>`, `identity<Int>(5)`) and `sat` bounds — later.
- Operator polymorphism — operators stay hardcoded `Int` (logical stays `Bool`) per
  Learning 2; they retrofit onto traits much later. Real `unify` still delivers the win:
  `x + 1` unifies `x`'s fresh var against `Int`.
- `match`, ADTs, traits, wiring `main.rs` to run end-to-end.

## Implementation

### 0. (Optional, adjacent one-liner) Fix the dropped-`else` bug
`type_check.rs:388` — the `Some(false_condition)` arm type-checks the else then builds
`else_branch: None`, discarding it; eval already reads `else_branch`, so `if false 1 else 2`
yields `Unit` instead of `2`. Change it to `else_branch: Some(Box::new(false_condition))`.
Independent of inference; trivial; include only if convenient. (Tracked in
state-and-plan §4.2.)

### 1. Introduce type variables
- Add `TypeVarId(pub usize)` newtype and change `Type::Var()` → `Type::Var(TypeVarId)` in
  `analysed.rs:22-27`.

### 2. A substitution + fresh-var context
- Add a solver struct (e.g. `Subst`) holding a growable `Vec<Option<Type>>` indexed by
  `TypeVarId` plus the fresh counter. Methods: `fresh() -> Type::Var`, `resolve(&Type)`
  (chase a var to its representative / concrete type), `bind(id, ty)`, `occurs(id, ty)`.
- Thread it through `infer_type_of_expression` alongside `env` (both already flow `&mut`
  through the walk, so this is a signature addition, not a restructure). Keep it separate
  from `TypeEnv` — `BindingId` and `TypeVarId` are different index spaces.

### 3. Rewrite `unify` into a real unifier
Replace the equality body at `type_check.rs:425`:
- `resolve` both sides through the substitution first.
- Var vs anything → `occurs`-check, then `bind`.
- `Fn(p1, r1)` vs `Fn(p2, r2)` → recurse on params (mind the `Option` nullary param) and
  results.
- Concrete `Literal`/`Unit` → equality as today; else `TypeMismatch`.
- All ~9 call sites are in this file and already consume the return; they change only to
  pass the solver. `unify_binary_op`, `apply_arguments`, the `if`/`let`/lambda/pipe/unary
  arms keep their shape.

### 4. Let unannotated params be inferred
- `ast::Param.type_dec` and `resolved::ResolvedParam.type_dec` → `Option<TypeDeclaration>`.
- `build_param` (`build_expression.rs:197`): accept the `None` case instead of erroring;
  `MissingParamType` becomes dead (remove it).
- Lambda arm (`type_check.rs:252`): when the annotation is `None`, mint a `fresh()` var,
  `env.set` it for the binding, and use it as the param type. Annotated params keep
  resolving via `resolve_type_dec`.

### 5. Zonk before emitting the analysed tree
- After the walk, apply the final substitution to every `Type` (expression `ty`s and
  `zip_bindings_with_types` at `type_check.rs:67`) so snapshots are concrete. Renumber any
  surviving unsolved vars to compact, deterministic ids (`Var(0)`, `Var(1)`, …) for
  snapshot stability.

### 6. (Optional cleanup) fold `Type::Unit` vs `Type::Literal(Literal::Unit)`
These are distinct under `PartialEq` (`analysed.rs:23` & `Literal::Unit`). Harmless with
equality-unify since they never meet; pick one to avoid a future surprise. Low priority.

## Critical files

- `crates/trestle/src/analyse/analysed.rs` — `Type::Var(TypeVarId)`, `TypeVarId`.
- `crates/trestle/src/analyse/type_check.rs` — the solver, `unify` rewrite, lambda arm, zonk, call sites.
- `crates/trestle/src/analyse/resolved.rs` — `ResolvedParam.type_dec` → `Option`.
- `crates/trestle/src/parse/ast.rs` — `Param.type_dec` → `Option`.
- `crates/trestle/src/parse/build_expression.rs` — `build_param` accepts untyped.
- `crates/trestle/tests/corpus.rs` + `tests/programs/` — new inference programs.

## Verification (TDD via the `insta` corpus)

Drive it through the existing snapshot corpus (`trsl_test!` opt-in with
`[ast, analyse, eval]`, or `[ast, error]` for the boundary cases). Write the program +
opt-in first, run, review the snapshot, then implement until it matches hand-computed
values. New programs under `tests/programs/` (a `00-basics` sub-dir or a new inference
tier):

1. **Inferred param:** `(x) => x + 1` → analysed type `Fn(Int, Int)`; apply it and eval to a
   concrete Int.
2. **Two inferred params:** `(a, b) => a + b` → `Fn(Int, Fn(Int, Int))`.
3. **Identity, single use:** `let id = (x) => x; id(5)` → evals `5`; `id`'s param stays a
   `Var`, the call binds it to `Int`.
4. **Monomorphic boundary (error snapshot):** `let id = (x) => x; id(5); id(true)` →
   `TypeMismatch`. Documents the chosen limit; the guardrail generalization later removes.
5. **Occurs-check (error snapshot):** self-application `(x) => x(x)` → the occurs-check
   fires (proves the check works).

Run `cargo test` in `crates/trestle`; accept/review snapshots with `cargo insta review`.
Records/generics/effects corpus programs stay `ignore`d. If step 0 is included, add an
`if false 1 else 2 == 2` eval snapshot that would have caught the dropped-else bug.
