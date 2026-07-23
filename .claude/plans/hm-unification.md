# Plan: Hindley–Milner unification in the type checker

## Context

Trestle's type checker (`analyse` pass 2) is synthesis-only and every `Type` is concrete.
Unannotated lambda parameters currently have nowhere to get a type: `resolve_type_dec`'s
`None` arm mints a `Type::Var`, but `unify` is just an equality check, so `(a) => a + 3`
fails (`Var != Int`). The goal is real unification: infer a param's type from how it's used.

Driving test (uncommitted): `tests/programs/01-unification/lambda-parameters/int-lambda-parameter.trsl`

```
let add3 = (a) => a + 3
add3(2)
```

Expected: infer `a : Int` from `a + 3`, so `add3 : Fn(Int, Int)`, and `add3(2)` evaluates to `5`.
Because the analysed snapshot is Debug-formatted and `AnalysedExpression.ty` / `Param.ty` /
`AnalysedBinding.ty` are all non-optional `Type`, the output must be **fully concrete — no
`Var(_)` may survive**.

**Already done**: `analysed.rs` has `struct TypeVarId(pub usize)` and `Type::Var(TypeVarId)`.
No further change to `analysed.rs` is needed for the core work.

## Design decision: union-find representation

Solve variables with a **textbook union-find forest** (parent pointers + path compression +
union-by-rank) — this is the data structure being learned, not just its effect.

Key subtlety that shapes the node type: a solved variable can map to a *compound that still
contains variables* (e.g. `f : Fn(Var(1), Var(2))`), so a node is not a clean "parent-or-concrete"
binary. Each node is one of three states:

```rust
enum Node {
    Root { rank: u32 },     // unbound representative — a free type variable
    Link(TypeVarId),        // parent pointer (union-find internal)
    Solved(Type),           // representative bound to a term (may itself contain Vars)
}

struct UnionFind { nodes: Vec<Node> }
```

Two index spaces stay **separate**: the existing `TypeEnv` maps `BindingId -> Type`;
`UnionFind` maps `TypeVarId -> Node`. `TypeEnv` may legitimately hold `Var(_)` mid-walk; the
union-find resolves it.

## Core operations (new, in `type_check.rs`)

- `fresh(&mut self) -> TypeVarId` — `nodes.push(Node::Root{rank:0}); TypeVarId(len-1)`.
  `nodes.len()` is the var counter, guaranteeing every live var has a node (needed by zonk).
- `find(&mut self, v: TypeVarId) -> TypeVarId` — walk `Link` pointers to the root, **compressing
  the path** (repoint visited nodes at the root). Takes `&mut self` for compression.
- `resolve(&mut self, ty: &Type) -> Type` — *shallow*: if `ty` is `Var(v)`, `find(v)`; if that
  root is `Solved(t)` return `t`, else return `Var(root)`. Does **not** descend into `Fn`
  children — used by `unify` before structural inspection.
- `bind_root(&mut self, root: TypeVarId, t: Type)` — set `nodes[root] = Solved(t)` (root only).
- `union_vars(&mut self, a, b)` — union-by-rank: link the lower-rank root under the higher,
  bump rank on tie.
- `occurs(&mut self, v: TypeVarId, t: &Type) -> bool` — resolve-and-recurse; true if `v`'s root
  appears in `t`. Guards against infinite types.
- `zonk(&mut self, t: &Type) -> Type` — *deep*: recursively apply the substitution. `Var` ->
  `find` root; if `Solved` recurse into it, if unbound `Root` it's a leftover free var. `Fn` ->
  zonk both sides; `Literal`/`Unit` -> clone. This produces the concrete output.

## The new `unify`

Signature: `fn unify(uf: &mut UnionFind, found: &Type, expected: &Type, span) -> Result<Type, AnalysisError>`

1. `let a = uf.resolve(found); let b = uf.resolve(expected);` — heads are now concrete or unbound vars.
2. Match:
   - `Var(i), Var(j)`: same root -> ok; else `union_vars(i, j)`.
   - `Var(v), t` / `t, Var(v)` (v unbound root): `occurs(v, t)` -> **occurs-check fail**
     (add `AnalysisError::InfiniteType`, or reuse `TypeMismatch` for now); else `bind_root(v, t)`.
   - `Fn(p1, r1), Fn(p2, r2)`: reconcile `Option` params — `(None,None)` ok, `(Some,Some)` recurse,
     mixed -> `TypeMismatch`; then recurse on `r1, r2`.
   - `Literal(x), Literal(y)` equal / `Unit, Unit` -> ok.
   - else -> `TypeMismatch { expected: b, found: a }` (report *resolved* heads, not raw vars).
3. Return `Ok(a)`. **The return value is now advisory** — the solution lives in the union-find,
   so the existing call sites that discard `unify`'s result stay correct (they used it only for
   its error effect).

## Threading `&mut UnionFind`

Add `uf: UnionFind` as a field on the existing `TypeCheckState`, initialize in `type_check`, and
thread `&mut state.uf` alongside `&mut TypeEnv` through:

- `infer_type_of_expression(expr, env, uf, bindings)`
- `unify_binary_op(uf, ...)`
- `get_type_after_applying_arguments(uf, ...)` and `apply_arguments(uf, ...)`
- `resolve_type_dec(uf, dec, span)` — its `None` arm becomes `Ok(Type::Var(uf.fresh()))`,
  fixing the one currently-uncompilable line.
- `zip_bindings_with_types(bindings, env, uf)` — zonk each binding type as it reads it out.

Trace for the test: `resolve_type_dec(None)` mints `Var(0)` for `a`; `env.set(a, Var(0))`;
`a + 3` runs `unify(uf, Var(0), Int)` -> `bind_root(0, Int)`; lambda type built as
`Fn(Var(0)-clone, Int)`; zonk rewrites to `Fn(Int, Int)`; `add3(2)` unifies `Int` vs `Int` -> `Int`.

## Final zonk pass (mandatory)

After the fold, before building `AnalysedProgram`, deep-zonk **every** `AnalysedExpression.ty`
(recursing through all `ExpressionKind` arms: `Binary` operands, `Unary`, `If` cond/branches,
`Lambda` `Param.ty` + body, `FunctionInvocation` args, `Let` value, `Block`) and every
`AnalysedBinding.ty`. Node `.ty` fields hold vars/compounds cloned *before* later unifications
solved them; nothing back-patches those clones, so the zonk is what makes the output concrete.
A `Var` that survives zonk = genuinely unbound/ambiguous -> an error (reuse `MissingAnnotation`
or `UntypedBindingAfterTypeCheck`); do **not** silently default. For this test nothing survives.

## Out of scope (do not build yet)

let-generalization / polymorphism / type schemes (`add3` is monomorphic here), instantiation,
the bidirectional check-mode sketched in the module doc (synthesis + unification suffices),
records/rows, ambiguous-var defaulting. **Keep** the occurs-check — it won't fire here but is the
standard soundness guard and is cheap.

## Ordered steps

1. `type_check.rs`: add `enum Node`, `struct UnionFind`, and its methods
   (`new`, `fresh`, `find` w/ path compression, `resolve`, `bind_root`, `union_vars`, `occurs`, `zonk`).
2. Rewrite `unify` to the `(uf, found, expected, span)` resolve-match-recurse algorithm above.
3. Fix `resolve_type_dec` `None` arm -> `Ok(Type::Var(uf.fresh()))`; add `uf` param.
4. Thread `&mut UnionFind` through `infer_type_of_expression`, `unify_binary_op`,
   `get_type_after_applying_arguments`, `apply_arguments`; add `uf` to `TypeCheckState` init.
5. Add the zonk tree-walker; call it after the fold; pass `uf` into `zip_bindings_with_types`.
   Surviving `Var` -> error.
6. (Optional) add `AnalysisError::InfiniteType` variant in `mod.rs` for occurs-check failures;
   otherwise map to `TypeMismatch`.
7. Update the 4 in-file unit tests — `too_many_arguments_is_an_error`,
   `arguments_to_argumentless_function_is_an_error`, `applying_correct_arguments_returns_result_type`
   call `get_type_after_applying_arguments` directly and must now pass a `&mut UnionFind::new()`.
   (`analyse_src`-based tests are unaffected; `let_annotation_mismatch` still yields `TypeMismatch`.)

## Verification

- `cargo test -p trestle` — 4 unit tests pass after step 7; `let_annotation_mismatch` still `TypeMismatch`.
- `cargo test -p trestle unification_` — runs `unification__ast`, `unification__analysed`, `unification__eval`.
- `cargo insta test --review` (or `INSTA_UPDATE=new`) to create the three snapshots, then
  **manually review** `int-lambda-parameter.analysed.snap`: param `a` is `Literal(Int)`,
  `add3`'s binding is `Fn(Some(Literal(Int)), Literal(Int))`, **no `Var(_)` anywhere**;
  `.eval.snap` shows `5`.

## Critical files

- `crates/trestle/src/analyse/type_check.rs` — all core work
- `crates/trestle/src/analyse/analysed.rs` — `TypeVarId` + `Var(TypeVarId)` already present
- `crates/trestle/src/analyse/mod.rs` — optional `InfiniteType` error variant
- `crates/trestle/tests/programs/01-unification/lambda-parameters/int-lambda-parameter.trsl` — driving test
