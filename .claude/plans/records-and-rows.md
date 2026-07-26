# Records + Type Declarations via Row Polymorphism

*Successor to [hm-unification.md](./hm-unification.md). Concepts first, then the seams in the
codebase where each piece lands.*

**Status:** the four Step 0 regression tests are written and `#[ignore]`d (see §0a, §0c). Everything
else is unimplemented.

## Context

Trestle has primitive evaluation and a first cut of HM inference in `crates/trestle/src/type_check/`.
The next milestone is **structural record types on a row-polymorphism engine**, per the decision
recorded in `docs-and-learnings/state-and-plan/design-decisions.md` — the same engine that later
carries effect `E`/`R` rows.

Three findings shape everything below.

**1. Unions will not use rows — and shouldn't.** Rows *can* express variants: a row is a label→type
map, and you get records by wrapping it in a product constructor (`Π ρ`) and variants by wrapping the
*same* row in a sum constructor (`Σ ρ`). That's Gaster & Jones 1996, and the duality is real. "Open"
points opposite directions in the two cases:

| | open record `{ x: Int \| ρ }` | open variant `⟨ Circle: Int \| ρ ⟩` |
|---|---|---|
| means | *at least* these fields | *at most* these cases, maybe more |
| helps | **consumers** — `r.x` works on anything with an `x` | **producers** — a value flows into any `match` covering its cases |
| grows by | adding fields makes the type *more* specific | adding cases makes the type *less* specific |

But design-decisions.md already decided **variants are nominal ADTs**, deferring polymorphic variants
(OCaml's error messages as the cautionary tale). A nominal ADT needs no row unification at all —
`Shape ~ Shape` is name equality, plus argument unification once they're generic. The row engine's
second customer is **effect rows**, not unions. This plan covers records only; ADTs + `match` are
separate work.

**2. Three defects in the current unification engine become fatal the moment records exist.** See
Step 0. Each is independently reachable today; records just make them routine.

**3. Rows without let-generalization are nearly invisible.** `let getName = (r) => r.name` has its
row variable *solved* by the first call site, so a second call with a different record shape errors.
That's why generalization is in scope here rather than deferred to the generics tier.

---

## The concept: how row unification actually works

A row is a finite label→type map plus an optional **tail** variable:

```
{ x: Int, y: Bool | ρ }      open   — these fields, plus whatever ρ turns out to be
{ x: Int, y: Bool }          closed — exactly these fields
```

The tail is what distinguishes rows from plain structural subtyping, and it's the whole payoff: the
leftovers stay **named**, so a function can thread them through and return the exact type it
received. Subtyping forgets them at the upcast. Rows also keep principal types under HM inference,
which subtype-constraint solving does not — the real reason to prefer them given the full-inference
goal.

### The algorithm

To unify `{ L | ρ₁ }` against `{ M | ρ₂ }`:

1. Resolve both tails first (walk `ρ` through the union-find; if it's solved to a row, merge those
   fields in and adopt *its* tail; repeat).
2. Unify the field types for every label present in **both**.
3. Let `A` = labels only on the left, `B` = labels only on the right.
4. Then, by tail:

| left tail | right tail | rule |
|---|---|---|
| closed | closed | `A` and `B` must both be empty, else `MissingField` |
| closed | `ρ₂` | `B` must be empty; solve `ρ₂ := { A }` **closed** |
| `ρ₁` | closed | `A` must be empty; solve `ρ₁ := { B }` **closed** |
| `ρ₁` | `ρ₂`, `ρ₁ ≠ ρ₂` | mint fresh `ρ₃`; solve `ρ₁ := { B \| ρ₃ }` and `ρ₂ := { A \| ρ₃ }` |
| `ρ₁` | `ρ₂`, same var | `A` and `B` must both be empty |

The fresh `ρ₃` in the both-open case is the part that's easy to get wrong. You can't just point `ρ₁`
at `ρ₂` — each side is missing what the other has, so they need a *shared* new tail representing "the
fields neither of us named."

### A worked trace

`let getName = (r) => r.name`, then `getName({ name: "a", age: 1 })`.

```
r : α                                 (fresh param var)
r.name  →  mint β (field type), ρ (rest)
           unify α  ~  { name: β | ρ }
           α := { name: β | ρ }        ← getName : { name: β | ρ } -> β

argument : { name: String, age: Int }  (closed — record literals are closed)
           unify { name: String, age: Int }  ~  { name: β | ρ }
           both:  name  →  unify String ~ β   ⇒  β := String
           A = {age}, B = {}
           left closed, right open  ⇒  B empty ✓ ,  ρ := { age: Int } closed
           result: String ✓
```

Call it again with `{ name: "b" }` and — **without generalization this fails**, because `ρ` is
already pinned to `{ age: Int }`. With generalization `getName` is stored as
`∀ β ρ. { name: β | ρ } -> β` and each call site gets fresh `β`, `ρ`. That's Step 3, and it's why
it isn't optional.

### The occurs check is mandatory here

There is no occurs check anywhere in `src/` today. Harmless with only `Fn` and literals, but rows
make it reachable from ordinary source:

```
(r) => { ...r, x: 1 }        unified against  r
   ⇒  ρ ~ { x: Int | ρ }     ⇒  infinite row, infinite loop
```

Every tail solve must check the variable doesn't occur in what it's about to be assigned.

### Duplicate labels: last one wins

Leijen's *Extensible Records with Scoped Labels* allows `{ x: Int, x: Bool }` and keeps **both**,
with a shadowing discipline and an `unrestrict` operation to reveal the hidden one. The Gaster &
Jones alternative forbids duplicates via "lacks" constraints (`ρ \ x`), which drags qualified types
into the engine. **Last-wins is a third, simpler point**: normalise duplicates away at construction,
keeping only the final occurrence.

With the flattened `BTreeMap` representation this is free — build the row by inserting fields
left-to-right and let `insert` overwrite:

```
{ x: 1, y: 2, x: "hi" }   ⇒   { x: String, y: Int }   value  x = "hi"
```

Apply the same normalisation in three places so type and value never disagree:

- **Record type annotations** (`resolve_type_dec`) — left-to-right insert.
- **Record literal inference** — left-to-right insert.
- **Record literal evaluation** — evaluate *every* field expression in source order (so it stays
  consistent once effects exist), inserting left-to-right, so the surviving value is the last one.

Two consequences:

- **The shadowed field is gone, not hidden.** Unlike scoped labels there's no way to recover it.
  That's the right trade for accessibility, and it makes `{ ...rec, x: v }` (deferred) mean plain
  override with no extra machinery — spread first, then the later explicit field wins, same rule.
- **During unification duplicates cannot arise**, because a tail is only ever solved to labels
  *absent* from the head. `resolve_row` should still prefer the head's fields over a merged-in
  tail's, defensively — one line, and it keeps the invariant local rather than assumed.

---

## Step 0 — Repair the unification engine

All in `src/type_check/unification.rs`. Worth landing on its own: each is a real bug today, and
you'll want them out of the way before rows confuse the picture.

**The four regression tests are already written and `#[ignore]`d.** Un-ignore each as its fix lands —
that's the checkpoint per fix. All four have been confirmed to fail today in exactly the way
described.

### 0a. Resolve to representatives before dispatching — the important one

`union_with_concrete_type:124` and `union_vars:196` compare already-solved concrete roots with
`PartialEq`. So a variable whose root is `Fn(Var(1), Int)` will **not** unify against
`Fn(Int, Int)` — it reports a mismatch instead of solving `Var(1) := Int`. Records make this
constant, because a record is the first type that routinely has variables inside it *and* sits
behind a variable.

The fix is a restructure rather than a patch: call the existing shallow `representative` on both
sides *first*, then dispatch structurally.

```rust
match (map.representative(expected), map.representative(found)) {
    (Type::Var(a), Type::Var(b))          => union_vars(a, b),
    (Type::Var(a), t) | (t, Type::Var(a)) => bind_var(a, &t),   // occurs check inside
    (Type::Literal(a), Type::Literal(b))  => match a == b { .. },
    (Type::Fn(p1, r1), Type::Fn(p2, r2))  => ..recurse..,
    (Type::Record(r1), Type::Record(r2))  => unify_rows(map, &r1, &r2, span),
    (found, expected)                     => create_type_mismatch_error(..),
}
```

Because the root is resolved *before* dispatch, the `Concrete`-vs-`Concrete` branches of
`union_with_concrete_type` and `union_vars` never arise — delete them rather than fixing them.

Two things fall out for free: you retire the 4×4 exhaustive product (which becomes 6×6 with rows —
you do not want to hand-write that), and you fix the existing asymmetry where `(Var, Unit)` solves
but `(Unit, Var)` errors (`:254` vs `:266`).

While you're in there: the recursive `Fn`-parameter call at `:238` passes `(param1, param2)` where
`param1` came from `expected`, so nested parameter mismatches report the two types the wrong way
round. Keep the outer `(found, expected)` argument order and fix the call.

**Tests (written, ignored):**

- `unification.rs::tests::a_solved_variable_unifies_structurally_with_a_compatible_function` —
  binds `v0 := Fn(v1, Int)`, then unifies against `Fn(Int, Int)` and asserts `v1 := Int`. Currently
  fails with `TypeMismatch { expected: Fn(Some(Var(1)), Int), found: Fn(Some(Int), Int) }`. This one
  hits the `union_with_concrete_type` site.
- `tests/programs/01-unification/partially-known-function/` — hits the *other* site, `union_vars`:

  ```trestle
  let inc = (n: Int) => n + 1
  let apply = (f, x) => f(x)
  apply(inc, 1)
  ```

  `apply`'s params are unannotated, so `f: α`, `x: γ`. Inferring `f(x)` takes the `Var`-callee branch
  at `inference.rs:381`, minting `β` and solving `α := Fn(γ, β)`. Separately `let inc = …` binds
  `inc` to a fresh `Var(δ)` whose root is `Concrete(Fn(Int, Int))` — bindings hold variables, not
  concrete types. At the call, `apply_arguments` unifies `Var(α)` against `Var(δ)`, both roots are
  `Concrete`, and `union_vars` compares them with `PartialEq`. Currently fails with
  `expected Fn(Some(Int), Int), found Fn(Some(Var(3)), Var(4))`. Should evaluate to `2`.

Note the argument order while working here: `unify(map, found, expected, span)`, but the `match`
scrutinee is `(expected, found)`.

### 0b. Add the occurs check

`fn occurs(&self, var: TypeVarId, ty: &Type) -> bool`, walking `Fn` children, record fields and row
tails through `find_root`. Call it in `bind_var` and at every row-tail solve. New error variant
`TypeCheckError::InfiniteType { var, ty, span }` in `error.rs`.

No test is pre-written — it isn't reachable until rows exist. Add the `ρ ~ { x: Int | ρ }` error
program in Step 2.

### 0c. Fix the self-union cycle

`union_vars:166-174`: when both ids resolve to the *same* free root, it writes
`map[r] = Reference(r)`, and the next `find_root(r)` recurses until the stack blows. Early-return
`Ok(())` when `first_root_id == second_root_id`, before the `match` on the two root nodes.

**Tests (written, ignored). Both fail by stack overflow → SIGABRT, aborting the whole test binary.**
That is why they're ignored rather than merely failing; run them isolated until fixed.

- `unification.rs::tests::unifying_a_variable_with_itself_is_a_no_op` — `unify(&v, &v)` on a free
  variable. Reaching the assert at all is the test.
  `cargo test -p trestle --lib unifying_a_variable_with_itself -- --ignored`
- `tests/programs/00-basics/conditionals/branches-share-a-variable/`:

  ```trestle
  let pick = (b: Bool, x) => if (b) x else x
  pick(true, 1)
  ```

  Both branches are the same unannotated parameter, so both are `Var(α)`, and `inference.rs:282`
  unifies them with each other. The overflow surfaces at the *call site*, when the argument is
  unified against the parameter and `find_root(α)` first walks the self-referential slot. Should
  evaluate to `1`.

### 0d. Collapse the duplicate unit

`Type::Unit` and `Type::Literal(Literal::Unit)` both exist and never unify with each other. Drop
`Type::Unit`; use the literal at the two sites that produce it (`inference.rs:236` and `:247`).
Optional, but it removes five match arms and a latent bug before you add six more type shapes.
Churns `.analysed.snap` files.

---

## Step 1 — `type` declarations, closed records, field access

### Type representation

Two encodings are standard. **Nested extension** (Leijen/Koka) models a row as
`Empty | Var(ρ) | Extend { label, ty, rest }`. **Flattened map + tail** models it as a map plus an
optional tail. Take the flattened one: label order stops mattering, unification becomes the set
partition above rather than a rotation search, and `insta` snapshots stay readable. `BTreeMap` not
`HashMap`, so snapshot output is deterministic.

In `typed_ast.rs:19`:

```rust
pub enum Type {
    Literal(Literal),
    Var(TypeVarId),
    Fn(Option<Box<Type>>, Box<Type>),
    Record(Row),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub fields: BTreeMap<String, Type>,
    pub tail: Option<TypeVarId>,   // None = closed. Step 2 starts populating this.
}
```

Row variables can share the type-variable id space and the single `UnificationMap` — they just solve
to a *row* rather than a type, so `RootUnionNode` needs a third shape:

```rust
enum RootUnionNode {
    FreeTypeVariable { level: Level, kind: VarKind },  // VarKind::{Type, Row}
    Concrete(Type),
    ConcreteRow(Row),
}
```

`kind` is defensive only — row vars are minted and consumed exclusively by row code, so a mismatch
means a bug and should surface as `InternalError` rather than misbehaving quietly. Add `level` now
even though nothing reads it until Step 3; it saves a second churn through this enum.

(The alternative — a second union-find keyed by a distinct `RowVarId` — is more type-safe but
duplicates `find_root`, `set`, occurs, and the level bookkeeping. Not worth it at this size.)

### Grammar

In `trestle.pest`:

- `type_definition = { "type" ~ identifier ~ "=" ~ type_expression }`, and
  `program = { SOI ~ (type_definition | expr)* ~ EOI }`. Guard `type` against identifiers using the
  same `!(alphanumeric_word_boundary)` idiom `let_kw` uses at `:9` — otherwise `typeName` mis-lexes.
  (`if`, `else` and `unit` are currently unguarded too, if you want to sweep that up.)
- `type_expression = { record_type | identifier }`, and `type_declaration = { ":" ~ type_expression }`
  replacing today's `":" ~ identifier` at `:12`.
- `record_type` needs to accept `{ }`, `{ x: Int }`, and — from Step 2 — `{ x: Int, ... }` and
  `{ ... }`. The trailing `...` is the fiddly bit in PEG; expect to write the body as "fields then
  optional `...`" rather than a uniform comma list.
- `record_literal = { "{" ~ field_init ~ ("," ~ field_init)* ~ "}" }`, added to the `expr`
  alternation at `:77` **before** `list_of_expressions`.

That ordering is the one PEG hazard worth thinking about, since `{ … }` is already a block. Ordered
choice handles it: `{ x }` tries `record_literal`, fails at the missing `:`, backtracks, and matches
the block. `{}` is unambiguous since `list_of_expressions` requires `expr+`. Exactly the kind of
thing to unit-test in the existing `build_expression.rs` test module.

Field access is **postfix** and binds tighter than every infix operator, so it does *not* belong in
the Pratt table. Make it a suffix loop on `primary`:
`primary = { (function_invocation | literal | identifier | "(" ~ expr ~ ")") ~ ("." ~ identifier)* }`,
built in `build_primary`.

### AST

`ast.rs:99` — the comment there already anticipates this ("grows into Generic/Record/Fn in later
tiers"):

```rust
pub enum TypeDeclaration {
    Named(String),
    Record { fields: Vec<(String, TypeDeclaration)>, open: bool },  // `open` = `...` present
}

// ExpressionKind gains:
RecordLiteral(Vec<(String, Expression)>),
FieldAccess { record: Box<Expression>, field: String },
```

`ParsedProgram` gains `type_definitions: Vec<TypeDefinition>` beside `expressions`.

Both node kinds must be mirrored through all three IRs — parsed (`ast.rs:76`), resolved
(`binding_resolved.rs:39`), typed (`typed_ast.rs:62`) — plus their walkers. The seam list: pest →
`build_expr` dispatch → `ExpressionKind` → `resolve_subexpr` → `infer_type_of_expression` +
`subsitute` → `eval_expr` → `trsl_test!`.

### Where type aliases get resolved

Type names are a **separate namespace** from value bindings, so aliases have no business in
`binding_resolution` — pass them through untouched and resolve them in `type_check`. Collect them
into a `HashMap<String, TypeDeclaration>` *before* the expression fold at `type_check/mod.rs:49`, so
aliases work regardless of declaration order and can reference each other. Thread the table into
`resolve_type_dec`.

`resolve_type_dec` then gains an alias lookup before its `UnknownType` error, and a
`TypeDeclaration::Record` arm that builds a `Row`. Alias expansion needs a visited-set —
`type A = { x: A }` otherwise expands forever. New `RecursiveTypeAlias` error.

### The two inference rules

New arms in `infer_type_of_expression`, each producing the module's `(kind, ty)` tuple:

- **Record literal** → infer each field, build `Record(Row { fields, tail: None })`. Literals are
  always **closed** — you know exactly what you wrote.
- **Field access `r.x`** → mint fresh `α` (the field type) and fresh `ρ` (the rest), then
  `unify(r.ty, Record(Row { fields: {x: α}, tail: Some(ρ) }))`; the node's type is `α`.

The second rule does all the work. Three lines, and it's the entire reason a function that reads
`.name` becomes row-polymorphic without an annotation — you never write `ρ`, the rule mints it. In
Step 1, before row variables exist, degrade it: require `representative(r.ty)` to already be a
`Record` and the field to be present, else `UnknownField`.

### Propagation and the evaluator

Both `subsitute` functions — the deep one on `UnificationMap` and the tree walk in `substitution.rs` —
must recurse into record fields and expand solved tails. (Keep the module's `subsitute` spelling;
it's consistent, and a rename is a separate change.)

Evaluator (`evaluate/mod.rs:18`): `Value::Record(Rc<BTreeMap<String, Value>>)` — `Rc` keeps
`Value: Clone` cheap, same reason `Closure` uses it — plus two `eval_expr` arms. A missing field is
`unreachable!("field access type-checks")`, consistent with that module's "resolution + type checking
make runtime faults impossible" invariant and its empty `EvalError`.

### Deliberately left out

Value-level spread `{ ...rec, x: v }` needs a record-*update* typing rule (and is where the occurs
check earns its keep), and named `sat` bounds need the generics tier. Both are in design-decisions.md;
neither is required for the core row story, and adding them here roughly doubles the surface.

---

## Step 2 — Row variables and `...`

Now `record_type` accepts a trailing `...`, `resolve_type_dec` mints a fresh row variable when
`open: true` and leaves `tail: None` otherwise, and field access mints its own row variable so
`(r) => r.x` needs no annotation at all.

Implement `unify_rows` per the table above, plus a `resolve_row` helper: walk the tail through
`find_root`, and whenever it's solved to a `ConcreteRow`, merge those fields in (unifying on label
collision) and adopt *its* tail, repeating until the tail is free or `None`. Getting `resolve_row`
right is most of the difficulty — do it before `unify_rows` and test it alone.

New errors: `MissingField { label, span }` and `UnknownField { label, record, span }`. No
duplicate-label error — that's normalised away by last-wins at construction.

Unit-test all five rows of the tail table plus the occurs case directly in `unification.rs` — these
are pure functions over the map and much easier to test there than through source programs.

**One caveat to know before it confuses you:** two *independently written* open annotations mint
*different* row variables. So

```
(r: { name: String, ... }) => { name: String, ... }
```

does **not** thread the tail through — the return type's `ρ` is unrelated to the parameter's.
Binding a row variable once across both positions is what `R sat { name: T, ... }` is for, and that
needs the generics tier. This is exactly the point design-decisions.md makes about named (F-bounded)
quantification in the `rename` example. Inferred row variables (from `.field`) don't have this
problem, because the rule mints one variable and it flows.

---

## Step 3 — Let-generalization with levels

Without this, Step 2's row variables get solved at the first use site and open records only help
within a single call chain — which is to say, rows buy almost nothing user-visible.

Use **Rémy's levels**. The naive alternative is "generalize any variable not free in the
environment", which means scanning the whole environment at every `let`; levels get the same answer
by bookkeeping.

- `UnificationMap` tracks `current_level: u32`; `mint_new_type_var` stamps it onto
  `FreeTypeVariable { level, .. }`.
- When linking two free roots, the survivor takes the `min` of the two levels. When solving a
  variable to a concrete type, lower every free variable inside that type to the variable's level.
  (This second half is easy to forget and produces unsound over-generalization.)
- In the `Let` arm (`inference.rs:214`): `enter_level()` → infer the value → `exit_level()` →
  `generalize(value.ty)`, quantifying every free variable whose level exceeds the now-current level.
- `BindingToTypeMap` (`binding_table.rs:9`) holds `Option<Scheme>`,
  `Scheme { quantified: Vec<TypeVarId>, ty: Type }`. Lambda parameters store `quantified: vec![]`,
  so there's no special-casing at use sites.
- The `Var` arm (`:53`) and `FunctionInvocation` arm (`:184`) `instantiate` instead of `clone` — a
  fresh variable at the current level per quantified variable. A monomorphic scheme instantiates to
  itself, so this is uniform.
- `TypeCheckedBinding` (`typed_ast.rs:37`) gains `quantified: Vec<TypeVarId>` so snapshots show the
  polymorphism, and `zip_bindings_with_types` carries it across. Note this flips a meaning: a
  surviving `Var` in a generalized binding is now **correct output**, not an unsolved leak.

No value restriction needed — Trestle is pure, with no mutable references, so generalizing
`let x = f()` is sound.

Two things this doesn't fix, worth a comment where they bite:

- **Recursive `let` still fails**, because the `Let` arm records the binding only *after* inferring
  its value. That needs the pre-registration seam `binding_resolution/mod.rs:11-13` already
  describes — and when you do it, the binding must stay *monomorphic* inside its own body
  (monomorphic recursion), or inference becomes undecidable.
- **Polymorphic parameters** (`(f) => (f(1), f("a"))`) remain rejected. That's rank-1 HM working as
  designed, not a bug.

---

## Verification

**Corpus** — `tests/corpus.rs`. Programs are not auto-discovered; each needs a `trsl_test!` line.

The two existing programs in `tests/programs/03-records-and-adts/` use the **nominal**
`Point { x: 3, y: 4 }` constructor form, and their comments say "nominal record types" — both stale
relative to the structural decision. Rewrite them and drop the `ignore =` attributes:

```trestle
type Point = { x: Int, y: Int }
let p = { x: 3, y: 4 }        // structural: no constructor name
p.x + p.y                     // 7
```

Worth adding:

- `03-records-and-adts/duplicate-labels/` — `{ x: 1, y: 2, x: "hi" }`, asserting the last-wins
  normalisation lands in both the `.analysed` and `.eval` snapshots (type `{ x: String, y: Int }`,
  value `x = "hi"`). A decision, not an error, so it belongs in the passing corpus.
- `03-records-and-adts/row-polymorphism/` — `let getName = (r) => r.name` applied to
  `{ name: "a", age: 1 }` **and** `{ name: "b" }`. This is *the* acceptance test for the whole plan:
  it fails without Step 2 and fails without Step 3, so it only passes when both are right.
- `01-unification/let-polymorphism/` — `let id = (x) => x` used at `Int` and at `String`.
- Error-stage programs: missing field on a closed record; `.z` on `{ x: Int }`; and the
  `ρ ~ { x: Int | ρ }` occurs case.

**Commands:** `cargo test -p trestle`, then `cargo insta review`. Expect wide `.analysed.snap` churn
from 0d and Step 3. `insta.yaml` sets `update: unseen`, so brand-new snapshots are written
automatically — the ones to actually read are the *changed* ones, particularly any binding type that
gained a `quantified` list it shouldn't have (over-generalization from the level-lowering half of
Step 3).

**Unit tests** belong in the existing per-module `#[cfg(test)] mod tests` blocks. The row algorithm
in particular is far easier to test directly against `UnificationMap` than through source programs.

**Worth doing early:** `main.rs` is parse-only, so you can't run a `.trsl` end to end outside the
harness. Wiring it to `parse → resolve → type_check → evaluate` against the existing `lib.rs`
pipeline is a few lines, closes a task already on the state-and-plan list, and makes poking at record
evaluation by hand possible — which you'll want repeatedly during Step 2.

## Follow-up (not this plan)

Nominal ADTs + constructors + `match` + exhaustiveness checking: a nominal type table and a new
expression form across all four phases, with no row machinery involved. Then `sat` bounds,
value-level spread, and eventually effect rows — which is where this row engine gets reused.
