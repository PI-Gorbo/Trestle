# Trestle — Design Decisions

_Last updated: 2026-07-13_

This is the depth behind the summary in
[state-and-plan.md §5](./state-and-plan.md#5-design-directions-recorded). §5 records *what*
was decided; this doc records *why*, the tradeoffs weighed, and where to read more. These
are **directions**, not final specifications — enough to aim Phase 2 (type system) and
Phase 2.5/3 (`core` + effects) at a coherent target.

A recurring theme drives every choice below: **accessibility**. Trestle should feel
approachable to someone who knows TypeScript/C#, not require fluency in functional-language
vocabulary. Powerful machinery is fine; inscrutable *surface* is not.

---

## Accessibility principle

Trestle deliberately prefers plain, familiar words over functional-programming jargon.

| Use (approachable) | Avoid (jargon) |
|---|---|
| `trait`, `impl` / `extend` | type class, instance dictionary |
| `record`, `{ name: T, ... }` | row type, `{ name: T \| ρ }` |
| `import`, `core` | prelude |
| `sat` (satisfies) | `<:`, subtype bound |
| `effect { }` | monad, Kleisli, bind |

The rule: **keep the powerful machinery under the hood** (higher-kinded traits, row
polymorphism, effect rows, Hindley–Milner inference are all *present*), but let what the
user reads and writes stay legible. When a concept needs a name, pick the one a working
programmer already understands. This is a hard design constraint — it is why the standard
library is `core`, not "prelude," and why bounds read `T sat { name: T, ... }` instead of
`T <: { name: T }`.

---

## Dispatch model

**Decision.** Nominal, Rust-shaped **traits**: `trait` declarations + `impl … for T` blocks
(multiple impls per type allowed, co-located with the type or elsewhere), generic traits
and generic methods, a `self` receiver, and dot-call method resolution. Global coherence
(one impl per (trait, type)); implicit, type-directed resolution. The trait system allows
**higher-kinded (type-constructor) trait parameters** so `Functor`/`Monad` are one shared
abstraction. **Associated types are deferred** — start with generic parameters. Operators
(`+ - * /`, `< == …`) are retrofitted onto `core` instances of `Add`/`Eq`/`Ord`.

### The three concepts being juggled

Ad-hoc polymorphism is what we're building: *one name, different implementations chosen by
type*. Distinguish it from its siblings:

- **Parametric** polymorphism — *one* implementation, uniform across all types because it
  never inspects the type (`identity`, `length`). Type parameters are opaque.
- **Subtype** polymorphism — dispatch via an object carrying its own method table (classic
  OOP inheritance).
- **Ad-hoc** polymorphism — *many* implementations, one per type, selected by type (`show`,
  `==`, `+`). This is the traits/type-classes/interfaces family.

Why parametric and ad-hoc feel like the same thing: **constrained generics** are both at
once. C#'s `T Max<T>(T a, T b) where T : IComparable<T>` is parametric in shape but
ad-hoc at the `CompareTo` call, gated by the `where` constraint. A trait bound
(`T sat Ord`) is exactly that `where` clause. Type classes *implement* ad-hoc polymorphism
*as* constrained parametric polymorphism — the constraint compiles to a hidden dictionary
argument (Wadler & Blott's "less ad hoc" result). That equivalence is why the two concepts
blur, and it's the mental model to keep.

### Why traits, not the alternatives

The candidates were Haskell-style type classes, Rust-style traits, and structural
interfaces (TS/Go). Two facts decided it:

1. **Operators are binary methods over one type** (`+ : T -> T -> T`). Structural
   interfaces model "one object's surface," so they choke on binary methods — the classic
   structural weakness. Since Learning 2 wants `+`/`==` as `Add`/`Eq`, structural
   interfaces are out for *dispatch*. (They remain great for *records* — see below.)
2. **The effect system wants shared `map`/`flatMap` across `Option`/`Result`/`Effect`** —
   i.e. `Functor`/`Monad`, which need **higher-kinded types** (abstracting over a type
   *constructor* `f`, not a type). Type classes have this natively; **Rust deliberately
   does not**, which is why `Option::map`, `Result::map`, and `Iterator::map` are three
   separate hand-written methods in Rust.

So the model is **type-class semantics** (global coherence, dictionary passing, HKT) worn
with **Rust's syntax** (impl blocks, `self`, dot-call). The dispatch *semantics* and the
*surface syntax* are orthogonal; we take the powerful semantics and the familiar surface.
Because Trestle's evaluator is a tree-walker, dispatch is runtime dictionary/type lookup
anyway — Rust's static-dispatch (monomorphization) advantage never applies, removing the
main reason one might prefer Rust's model.

### HKT, associated types, GATs

- **Higher-kinded types (HKT).** Kinds are "the types of types." `Int` has kind `*`;
  `Option` is a *type constructor* of kind `* -> *` (`Option` alone isn't a type, only
  `Option<Int>` is). HKT = being generic over a constructor `f`, as in
  `trait Functor f { map : (a -> b) -> f a -> f b }`. Required for shared `map`/`flatMap`.
  **Commitment:** our kind system is a notch more expressive than Rust's, even though the
  syntax stays Rust-like.
- **Associated types** are trait *output* types the impl determines (`type Item` on
  `Iterator` — exactly one per implementing type), versus generic trait *parameters* which
  the caller chooses and a type may implement many ways (`Add<Rhs>`). Rule of thumb:
  generic parameter when a type could implement multiple ways; associated type when the
  type uniquely determines the companion type. **Deferred** — they're a checker-precision
  feature and our dispatch is runtime; start with generic parameters, add associated types
  only when ergonomics demand.
- **GATs (generic associated types)** are associated types that themselves take
  parameters (`type Item<'a>`). They push associated types toward HKT via encoding, but
  aren't native HKT. Not needed early; noted for completeness.

### `impl` blocks vs C# extension methods

An `impl`/`extend` block separates methods from the type's definition, like a C# extension
method (or Swift extension, Scala `extension`). The **upgrade**: C# extension methods are
pure call-site sugar and don't satisfy interfaces; a trait `impl` *also proves conformance*
— it makes the type count toward `T sat Trait` bounds, with coherence. So we get the
dot-method ergonomics **and** dispatch/conformance from the same construct.

### Operators as `core` instances

Even though users may not (initially) declare their own operators, operators are built on
the trait mechanism internally: `+`/`==`/`<` on `Int`/`Float`/`String` become `core`
instances of `Add`/`Eq`/`Ord`. This dissolves today's "Int-only comparisons/arithmetic"
limitation for free, keeps numeric/string operators out of the interpreter as special
cases (matching the primitives-in-Rust / library-in-Trestle split), and makes
"let users define operators" a later *policy toggle* rather than a rearchitecture.

### Reading

- Wadler & Blott, *How to make ad-hoc polymorphism less ad hoc* (1989) — founding
  type-class paper; dictionary passing; the parametric↔ad-hoc bridge.
- Cardelli & Wegner, *On Understanding Types, Data Abstraction, and Polymorphism* (1985) —
  the canonical taxonomy.
- Scala 3 contextual abstractions (`given`/`using` + `extension`) — type classes with the
  concise, named-instance, dot-method syntax we're after.
- Swift protocols + extensions; "Protocol-Oriented Programming in Swift" (WWDC 2015) —
  `Self`, retroactive nominal conformance, default methods via extensions.
- Oliveira, Moors & Odersky, *Type Classes as Objects and Implicits* (2010) — connects the
  type-class model to OO/DI intuitions (note: Scala implicits give up global coherence).
- Rust Book ch. 10 (traits) + any "why no HKT / GATs" write-up (Niko Matsakis) — to feel
  the ceiling we're stepping past.

---

## Records, rows, and `sat` bounds

**Decision.** Records are **structural**, built on a **row-polymorphism** engine.
Three-dot syntax: `{ name: T, ... }` = "at least these fields" (open; introduces a row
variable), `{ name: T }` = exactly these (closed). Bounds use **`sat`** (satisfies) for
both nominal-trait and structural bounds. **Variants are nominal ADTs.** Inference is real
Hindley–Milner with `Type::Var` *and* row variables.

### Rows are the inference-friendly way to say "at least these fields"

"At least these fields" has two classical spellings: structural subtyping (`T <: { name }`)
and row polymorphism (`{ name | r }`). They accept the same records, but differ downstream:

- **Row polymorphism keeps the leftovers *named*** (the row variable), so a function can
  *thread them through* and return the exact input type. Plain subtyping forgets them once
  it upcasts.
- **Inference:** row polymorphism extends Hindley–Milner cleanly and keeps **principal
  types** (best type inferred, no annotations). Full structural *subtyping* + inference is
  much harder — it slides into subtype-constraint solving (why TypeScript has no complete
  inference and leans on annotations). Given the goal of *full* inference, rows win.

So we get the **structural feel** the design wants with **full inference**, by using rows
as the engine and hiding the row variable. The row variable is normally inferred and
invisible; a function that reads `.name` is automatically row-polymorphic.

### The `sat` keyword and the `rename` example

Bounds read `sat`, never `<:` (which forces reasoning about a confusing "more fields = more
specific" direction). One keyword covers both constraint kinds:

- Nominal trait bound: `T sat Show`
- Structural/row bound: `T sat { name: T, ... }`

A **named** bound is what binds the row variable once so input and output share it:

```
function rename<T, R sat { name: T, ... }>(rec: R, newName: T): R {
  return { ...rec, name: newName }
}
```

`R sat { name: T, ... }` means `R = { name: T | ρ }` for a fresh row variable `ρ`. Inside,
`{ ...rec, name: newName }` has type `{ name: T | ρ }` — the *same* `ρ` — which **is** `R`.
No coercion, no loss: the row variable proves the other fields survive. If the bound were
written inline in both positions instead of named, the two `...` would be *different* row
variables and the return type wouldn't be tied to the input — which is exactly why named
(F-bounded) quantification exists.

**No implicit coercion between two distinct bounded variables.** Given
`<A sat { name: T, ... }, B sat { name: T, ... }>`, `A` and `B` have different tails; the
checker won't turn an `A` into a `B` (unsound — they may carry different other fields).
Converting unknown types needs *evidence*: either they're the same variable (identity), or
a witness trait (`A sat Into<B>`, dictionary-passed). This is the same principle as "you
can't coerce an arbitrary `T` to an arbitrary `U`."

### Three-dot `...`, deliberately

`...` mirrors value-level spread: `{ name: T, ... }` in a *type* = "these fields and a row
of more"; `{ ...rec, name: v }` in a *value* = "spread the existing fields, then override."
Type-level "the rest exists" and value-level "copy the rest" share one symbol — and TS/JS
already train everyone on value-spread. That symmetry is the reason for three dots over
Rust's `..`.

### Variants: nominal, not structural

Records are structural; **variants are nominal ADTs** (`type Shape = Circle(..) |
Square(..)`, pattern-matched). Structural/open ("polymorphic") variants are the dual of
open records and are deferred: they make inference hairier and produce inscrutable error
messages (OCaml's polymorphic variants are the cautionary tale), which cuts against
accessibility. Effect rows are a controlled special case of open variants, but they stay
hidden behind `effect { }`, so users rarely touch raw row syntax.

### Reading

- Koka (Daan Leijen) — the reference language for row-typed effects; start here.
- Leijen, *Extensible Records with Scoped Labels* — a clean, inference-friendly rows design.
- Gaster & Jones, *A Polymorphic Type System for Extensible Records and Variants* (1996).
- PureScript row-typed records; OCaml object/row types (for the "hide the row var" feel).

---

## Effects

**Direction.** Full E/R tracking via **row-polymorphic effects** (Koka-style). An `Effect`
carries a value type `A` plus an error row `E` and a requirement row `R`. Effects **add** to
the rows as computations compose (unify tails) and **subtract** as handlers/the runtime
discharge them — "types as sets, add and subtract sets." Surfaced through an `effect { }`
block so users rarely see raw row syntax.

Rows are the shared machinery: the same `Type::Row` + row-variable unification that powers
open records powers effect tracking. `E`/`R` accumulate up the call graph until the
**runtime/platform** — the single boundary — satisfies every requirement and handles every
error (the spec's railway / DI model; and see the Roc platform mapping below).

**Staged rollout**, gated on the Phase 2 real-inference milestone:
1. `Effect` types only its value `A`.
2. Add `E` (error row).
3. Add `R` (requirement row).

Full **set-theoretic / semantic-subtyping** types (union, intersection, and true
*difference* as first-class type operations) are the aspirational north star for union
types done right — but complete inference there is genuinely hard (decidability concerns),
so rows are the proven first path, not that. Union types land naturally as the row-over-
variants dual once rows exist.

### Reading

- Koka; Leijen, *Type Directed Compilation of Row-typed Algebraic Effects*.
- Algebraic effects & handlers: Eff, Frank; Unison abilities.
- Castagna / CDuce semantic subtyping (types as sets, with difference); Elixir's set-
  theoretic type system — the north-star direction for unions.

---

## Modules and `core`

**Direction.** An opinionated **workspace of packages**, Roc/Go-shaped. **Runnable = the
package provides a `main`** — intrinsic to the package, not a configuration flag. The
**platform/config selects which `core`** is in scope. A small, opinionated `core` is
**auto-imported**; everything else is **explicit per-file `import`**.

### The Roc platform mapping

Roc splits code into apps, platforms, and packages. An app declares which *platform* it
runs on and `provides main` to it; the **platform** defines the available effects/
primitives, the expected type of `main`, and effectively the environment — so the platform
determines the "prelude." A **package** is a pure library with no platform and can't run.

This maps directly onto Trestle: **the platform is the effect runtime** — the single
boundary that satisfies all requirements (`R`) and handles all errors (`E`). "Configuration
changes your `core`" becomes "which platform," a first-class choice rather than a matrix of
build flags. "Runnable = has a `main`" is Roc/Go's model: a property of the code, not the
manifest.

### The `core` loader is the module system

"Auto-import `core`" is just "every module has an implicit `import core`," where `core` is
the platform-provided standard library. So the `core` loader (Phase 2.5) and the
module/import system are **one workstream**, not two — design them together. Mechanically:
parse + analyse `core` once, cache the resolved bindings, and reuse it as the shared
`Rc`-linked parent environment of every user module (cheap structural sharing).

### Imports: hybrid

- **Core is auto-imported** (`Option`, `Result`, `Effect`, operator instances, basic
  list/record utilities) — zero ceremony for the common 90%.
- **Everything else is explicit, per-file `import`** — readable, tooling-friendly
  dependency clarity (Rust/Go/Roc/TS all do this).

### Reading

- Roc: roc-lang.org tutorial; Richard Feldman's platform talks ("The Design of the Roc
  Programming Language").
- Go `package main` (runnable = has `main`); Cargo workspaces (workspace/package mechanics).
