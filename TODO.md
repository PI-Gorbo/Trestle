# TODO

Known issues in the type checker, found while building the row-polymorphism test corpus.
Paths are relative to `crates/trestle/src/type_check/`.

## Bugs

- [ ] **Binding a variable doesn't lower the levels inside the type it's bound to.**
  `union_with_concrete_type` (`unification.rs:494`) sets a free root at level L to a type
  whose own variables can sit deeper than L, and leaves them there. They're now reachable
  from something in an outer scope, but `generalise` still quantifies them. The
  Free<->Concrete arms of `union_vars` (`:566`, `:574`) have the same gap. Fix: before
  binding, lower every free root in the (resolved) type to `min(its level, L)`. Pinned by
  three ignored corpus tests: `captured-function-result-is-not-generalised`,
  `captured-row-is-not-generalised`, `captured-tail-is-not-generalised`.

- [ ] **Unknown variables are replaced by the whole enclosing type.** In `resolve` and
  `resolve_shallow` (`unification.rs:132`, `:142`), `None => ty.clone()` substitutes the
  *entire* type being mapped for a variable missing from the union-find map. Nothing triggers
  it today, but if it ever happens the result is a wrong type with no error. Return
  `Type::Var(*id)`, or panic with an internal-error message.

- [ ] **Two errors share one diagnostic code.** `UntypedBindingAfterTypeCheck` (`error.rs:119`)
  uses `trestle::missing_annotation`, the same code as `MissingAnnotation` (`error.rs:111`).
  Give it its own code, e.g. `trestle::untyped_binding`.

## Cleanup

- [ ] **Mark the code paths that can no longer run.** Since `resolve_shallow` resolves solved
  variables completely, these can't be reached:
  - the `Some(root_concrete_type)` branch in `union_with_concrete_type` (`unification.rs:492`)
  - the two `todo!("flatten records whose tail is already solved")` in the open-vs-open
    record arm (`unification.rs:389`, `:394`)

  Turn them into `unreachable!`/`expect` with a message saying why, so they fail loudly if
  that ever stops being true.

- [ ] **Readable error messages.** `error.rs` formats types with `{:?}`, so users see
  `expected Literal(Int), found Literal(String)`. `trestle-wasm` already has a `format_type`
  that could be shared.

## Performance

- [ ] **`resolve_shallow` is no longer shallow.** It now resolves every variable in the type,
  the same as `resolve`, and `unify_inner` calls it at every step. Fine for now; if it becomes
  slow, write a version that only follows a record's tail chain.

## Missing tests

- [x] **Open-vs-open records whose tails share a root.** Success path:
  `open-meets-open-same-row`. Mismatch path (`{ x | r } ~ { y | r }`): unit test
  `open_records_sharing_a_tail_cannot_gain_different_fields`. The mismatch can't come from
  source: annotations only build closed records and field access only extends a tail.
- [x] **A mismatch found while unifying record tails.** Can't happen. `resolve_shallow`
  flattens solved tails into the fields, so both tails are free roots when the record arm
  runs, and binding a free root can only fail the occurs check (`InfiniteType`, no
  expected/found). Covered by `infinite-row`.
- [x] **The callee-shape `unify`** (`inference.rs:733`). `callee` is always a free root after
  `resolve_shallow`, so the only failure is `InfiniteType`. Covered by
  `infinite-type/self-application` and `row-polymorphism/infinite-row`.
