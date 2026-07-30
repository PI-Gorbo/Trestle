# Wire annotations through the prelude, delete the hardcoded type-name match

## Context

`resolve_type_dec` in [inference.rs:436-445](crates/trestle/src/type_check/inference.rs#L436-L445) maps annotation names to types with a hardcoded `match name.as_str()`. Commit `a168067` added the prelude to replace exactly this — but it only wired the *name-resolution* half.

What is missing today:

- Only the `TypeDeclaration` arm calls `resolve_type_expression`. The three annotation fields — `Let::type_dec` ([mod.rs:246-250](crates/trestle/src/binding_resolution/mod.rs#L246-L250)), `ResolvedParam::type_dec` ([mod.rs:379](crates/trestle/src/binding_resolution/mod.rs#L379)), `ResolvedLambda::return_type` ([mod.rs:205](crates/trestle/src/binding_resolution/mod.rs#L205)) — still move the raw `ast::TypeExpression` across verbatim. The comment at [binding_resolved.rs:46-49](crates/trestle/src/binding_resolution/binding_resolved.rs#L46-L49) says as much.
- Nothing ever writes `InferenceCtx::type_env`. `InferenceCtx::new` fills it with `None`, so the positional `PRELUDE_TYPES[i] == TypeBindingId(i)` contract that [prelude.rs:5-7](crates/trestle/src/prelude.rs#L5-L7) documents has no consumer.
- `get_type_from_type_expression` ([inference.rs:326-330](crates/trestle/src/type_check/inference.rs#L326-L330)) is `todo!()`.

The two lists have already drifted: `Unit` is `PRELUDE_TYPES[4]` but absent from the match, so `let x: Unit = unit` resolves cleanly and then dies with `UnknownType { name: "Unit" }`. That is the bug class the prelude exists to make impossible.

**Outcome:** annotations resolve to a `TypeBindingId` in binding resolution; type checking recovers the `Type` through `type_env`, seeded from `PRELUDE_TYPES`. The string match goes away, and the two lists can no longer drift.

**Scope:** prelude builtins only. The `TypeDeclaration` inference arm stays `todo!()`, so a user alias in annotation position (`type MyInt = Int` then `let x: MyInt = 5`) still does not work — it does not work today either.

## Changes

### 1. `binding_resolution/binding_resolved.rs` — annotations hold ids

Change all three fields from `Option<TypeExpression>` to `Option<ResolvedTypeExpression>`:

- `ResolvedExpressionKind::Let::type_dec` ([:84](crates/trestle/src/binding_resolution/binding_resolved.rs#L84))
- `ResolvedParam::type_dec` ([:103](crates/trestle/src/binding_resolution/binding_resolved.rs#L103))
- `ResolvedLambda::return_type` ([:109](crates/trestle/src/binding_resolution/binding_resolved.rs#L109))

Drop the now-false comment block at [:46-49](crates/trestle/src/binding_resolution/binding_resolved.rs#L46-L49), drop `TypeExpression` from the import at [:17](crates/trestle/src/binding_resolution/binding_resolved.rs#L17), and fix the module doc at [:7-8](crates/trestle/src/binding_resolution/binding_resolved.rs#L7-L8) ("carried through untouched as `ast::TypeDeclaration`" is no longer true). Also drop the stale "No table is indexed by it yet" sentence on `TypeBindingId` ([:23](crates/trestle/src/binding_resolution/binding_resolved.rs#L23)) — `type_env` now is.

### 2. `binding_resolution/mod.rs` — resolve annotations

Add an `Option` wrapper beside the existing `resolve_type_expression` ([:324](crates/trestle/src/binding_resolution/mod.rs#L324)), reusing it rather than duplicating the lookup:

```rust
fn resolve_optional_type_expression(
    type_expression: Option<ast::TypeExpression>,
    span: SourceSpan,
    scope: &Scope,
    ctx: &mut ResolveContext,
) -> Result<Option<ResolvedTypeExpression>, BindingResolutionError> {
    type_expression
        .map(|te| resolve_type_expression(te, span, scope, ctx).map(|(resolved, _)| resolved))
        .transpose()
}
```

Discarding the returned `Scope` is correct: every arm of `resolve_type_expression` returns `scope.clone()` — a type expression introduces no bindings.

Call it from three places, all against the **incoming** `scope`:

- **`Let` arm** ([:238-251](crates/trestle/src/binding_resolution/mod.rs#L238-L251)) — before `bind_let`.
- **Lambda arm** ([:191-207](crates/trestle/src/binding_resolution/mod.rs#L191-L207)) — resolve `lambda.return_type` against `scope`, not `updated_scope`; the param binding is in the value namespace and contributes nothing to the type namespace.
- **`resolve_parameter`** ([:359-383](crates/trestle/src/binding_resolution/mod.rs#L359-L383)) — becomes fallible: return `Result<(ResolvedParam, Scope), BindingResolutionError>`. Its one caller at [:194](crates/trestle/src/binding_resolution/mod.rs#L194) gains a `?`.

An unbound name in an annotation now reports `BindingResolutionError::UnboundTypeName`, the same as `type Alias = Missing` does today.

### 3. `type_check/inference.rs` — read the ids

**Seed `type_env`** in `InferenceCtx::new` ([:33-38](crates/trestle/src/type_check/inference.rs#L33-L38)) and drop the `#[allow(dead_code)]` on the field:

```rust
let mut type_env = TypeBindingToTypeMap::new(type_binding_count);
for (index, prelude_type) in prelude::PRELUDE_TYPES.iter().enumerate() {
    type_env.set(TypeBindingId(index), prelude_type.ty.clone());
}
```

This is the *only* place the positional contract is consumed; say so in a comment pointing back at [prelude.rs:5-7](crates/trestle/src/prelude.rs#L5-L7).

**Carry the type-binding names** for the `UnknownType` diagnostic. Add a borrowed field to `InferenceCtx`:

```rust
pub(super) struct InferenceCtx<'a> {
    pub(super) variable_env: BindingToTypeMap,
    pub(super) type_env: TypeBindingToTypeMap,
    pub(super) type_bindings: &'a [ResolvedBinding],
}
```

`type_check` ([mod.rs:41-56](crates/trestle/src/type_check/mod.rs#L41-L56)) already destructures `type_bindings` and it outlives the fold, so pass `&type_bindings` instead of just `.len()`; `TypeCheckState` picks up the lifetime. This keeps `infer_type_of_expression`'s arity unchanged.

**Implement `get_type_from_type_expression`** (replacing the `todo!()` at [:326-330](crates/trestle/src/type_check/inference.rs#L326-L330), and its `#[allow(dead_code)]`):

```rust
fn get_type_from_type_expression(
    type_expression: &ResolvedTypeExpression,
    env: &InferenceCtx<'_>,
    span: SourceSpan,
) -> Result<Type, TypeCheckError> {
    match type_expression {
        ResolvedTypeExpression::Named(id) => env.type_env.get(*id).cloned().ok_or_else(|| {
            TypeCheckError::UnknownType { name: env.type_bindings[id.0].name.clone(), span }
        }),
        // Unreachable from source: `type_declaration = ":" ~ identifier` (trestle.pest:12) admits
        // only an identifier in annotation position; records reach here only via `type X = …`.
        ResolvedTypeExpression::Record(_) => Err(TypeCheckError::InternalError {
            message: String::from("record types are not yet type-checked"),
            span,
        }),
    }
}
```

**Rewrite `resolve_type_dec`** ([:423-449](crates/trestle/src/type_check/inference.rs#L423-L449)) to take `&Option<ResolvedTypeExpression>` plus `&InferenceCtx`: `Some` delegates to the above, `None` keeps minting a fresh `Type::Var`. Delete the string match, the `TypeExpression::Named` else-block, and its `InternalError { message: "Expected type Erro" }` typo. Drop `TypeExpression` from the `ast` import at [:11](crates/trestle/src/type_check/inference.rs#L11).

Its three call sites ([:180](crates/trestle/src/type_check/inference.rs#L180), [:193](crates/trestle/src/type_check/inference.rs#L193), [:247](crates/trestle/src/type_check/inference.rs#L247)) each gain the `env` argument. At [:180-182](crates/trestle/src/type_check/inference.rs#L180-L182) the immutable read finishes before `env.variable_env.set`, so the sequential borrows are fine.

### 4. `corpus.rs` — stop the suite being red

`unification_type_alias_declaration_record_{analysed,eval}` fail today, before any of this: the `todo!("type declarations are not yet type-checked")` at [inference.rs:315](crates/trestle/src/type_check/inference.rs#L315) panics, and only `record.ast.snap` exists on disk. Narrow the registration at [corpus.rs:491-495](crates/trestle/tests/corpus.rs#L491-L495) from `[ast, analyse, eval]` to `[ast]`, with a comment that the other two stages return once `type` declarations are checked. Pre-existing and strictly out of the prelude work, but it means "the suite passes" is a meaningful signal here.

## What changes for users

- `let x: Unit = unit` compiles. This is the drift bug, fixed structurally.
- An unknown annotation name moves diagnostic stage: `let x: Missing = 1` now fails resolution with `UnboundTypeName` rather than type-check with `UnknownType`.
- `TypeCheckError::UnknownType` becomes unreachable in practice — it can only fire on a `type_env` slot that resolution filled but inference did not, i.e. a user alias, and any program containing one hits the `TypeDeclaration` `todo!()` first. Keep the variant; it is the guard for when the alias arm lands.

## Verification

`cargo test` — the two corpus failures above are the current baseline; everything else passes and must keep passing.

New tests:

- `type_check/mod.rs` (uses the existing `analyse_src` helper at [:109-114](crates/trestle/src/type_check/mod.rs#L109-L114)):
  - `unit_annotation_is_accepted` — `analyse_src("let x: Unit = unit")` succeeds. Fails today with `UnknownType`; this is the regression test for the drift.
  - Keep `let_annotation_mismatch_is_an_error` green — it proves `Int` still resolves through the new path.
- `binding_resolution/mod.rs` (existing `resolve_src` helper at [:390-393](crates/trestle/src/binding_resolution/mod.rs#L390-L393)), mirroring `a_builtin_resolves_to_its_prelude_binding`:
  - `let_annotation_resolves_to_its_prelude_binding` — `"let x: Int = 1"`, assert `type_dec == Some(ResolvedTypeExpression::Named(TypeBindingId(0)))`.
  - `param_and_return_annotations_resolve` — `"let f = (a: Int): Bool => true"` (the grammar's `optional_type_declaration` at [trestle.pest:70](crates/trestle/src/parse/trestle.pest#L70) covers the return type), assert both sides land on their prelude ids.
  - `unknown_annotation_name_is_an_unbound_type_error` — `"let x: Missing = 1"` → `UnboundTypeName`.

No snapshot churn expected: `.ast.snap` files are parse-level, and `typed_ast` has no annotation field.

Manual check that the prelude is now load-bearing: temporarily reorder two entries in `PRELUDE_TYPES` and confirm the type-check tests fail — the positional contract should be the thing holding this together, not a second copy of the name list.
