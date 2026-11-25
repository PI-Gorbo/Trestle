# TODO

Known issues in the type checker, found while building the row-polymorphism test corpus.
Paths are relative to `crates/trestle/src/type_check/`.

## Bugs

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

- [ ] **Open-vs-open records whose tails share a root** (`{ x | r } ~ { y | r }`). No program
  found yet that reaches this branch.
- [ ] **A mismatch found while unifying record tails.** No test checks which side is
  reported as expected and which as found there.
- [ ] **The callee-shape `unify`** (`inference.rs:733`) can't produce a mismatch from source,
  so its expected/found order is untested.
