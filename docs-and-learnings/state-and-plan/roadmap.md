# Trestle — Roadmap (skill tree)

The ordered list of what is left, from here to effects and transpilation. Rationale for the
direction lives in [design-decisions.md](./design-decisions.md); the history of how we got
here lives in [state-and-plan.md](./state-and-plan.md).

```
 TIER 0  cleanup    CLI runs full pipeline · doc sweep · unification nits
 TIER 1  roots      [A] recursive let    [B] intrinsics      [C] ADTs + constructors
 TIER 2                                  [G] core v0         [D] match   [E] type Option<T>
 TIER 3             [H] core in Trestle: Option, Result, List, map/fold  →  [I] modules
 TIER 4             [J] Effect<A> as an ADT  →  [K] effect { }  →  [L] E row → R row
 SIDE BRANCH        traits → operators via Add/Eq/Ord → Show → HKT   (not needed for J/K)
 RUNS ALONGSIDE     memory management (gated on [A])
 FUTURE             transpilation to TypeScript
```

**Next up:** 1 → 4 (with 18 alongside) → 5 → 6 + 7.

**Traits are off the critical path.** `Effect<A>` can be a concrete ADT with its own
`flatMap`, and `effect { }` only needs to desugar to calls to that name. A `Monad` trait and
higher-kinded types only matter once one `flatMap` should serve Option, Result and Effect.

## Tier 0 — Cleanup

1. [ ] Make the CLI (`main.rs`) run parse → analyse → evaluate. Today it only parses.
2. [ ] Update `apps/demo/app/lib/features/features.ts` and `state-and-plan.md` to match what
       has shipped: polymorphic `let`, function type expressions, row polymorphism.
3. [ ] Unification nits:
        - Delete the commented-out `root_concrete_type` block in `union_with_concrete_type`.
       - `union_vars` only ever receives two free variables. Replace its Free↔Concrete and
         Concrete↔Concrete arms with `unreachable!` or delete them; as written they skip
         the level move and occurs check, and compare with `==` instead of unifying.
       - Turn the `todo!("flatten records whose tail is already solved")` panics into
         errors. They look unreachable, but a panic under WASM is a trap.
3a. [ ] Error message nits:
       - `InfiniteType` prints the variable as `_3` but the type's variables as `A`, `B`, so
         they never match. Name both from one shared table (`write_var` in `render.rs`).
       - Sweep error messages outside the type checker (binding resolution, build, eval)
         for `Debug` output and spans that cover more than the offending expression.

## Tier 1 — Roots

4. [ ] **Recursive `let`.** Bind the name to a fresh type variable before checking the
       right-hand side, then generalise. Pre-register every binding in a block the same way
       for mutual recursion. Blocks `map`, `fold`, `length` — any library code.
5. [ ] **Intrinsics.** A `PRELUDE_VALUES` table beside `PRELUDE_TYPES`: name, `Scheme`,
       native function. Start with `concat`, `string_eq`, `int_to_string`, `string_length`,
       float arithmetic and `print`. No traits needed; monomorphic is fine for now.
6. [ ] **Sum types (ADTs) and constructors.** Nominal variants.

## Tier 2 — The type system grows

7. [ ] **`match`** on flat patterns: constructor, variable, wildcard, literal.
8. [ ] **Generic data types**, e.g. `type Option<T> = None | Some(T)`. Schemes already
       quantify; the new part is scoping type variables in type declarations.
9. [ ] **core v0.** A hardcoded `core.trsl`, analysed before user code in a shared scope.
10. [ ] *(optional)* Explicit `<T>` on lambdas. Inference covers most uses; it is needed for
        `sat` bounds.

## Tier 3 — Bootstrapping in Trestle

11. [ ] Write `core` in Trestle: `Option`, `Result`, `List`, `map`, `fold`, `filter`.
12. [ ] **Modules.** One scope per file, an `import` statement, a CLI that loads a
        directory. `core` becomes an implicit import. Packages, workspaces and platforms
        wait until effects need a runtime boundary.
13. [ ] Nested patterns and exhaustiveness checking for `match`.

## Tier 4 — Effects

14. [ ] `Effect<A>` as an ADT in `core`, with its own `flatMap`.
15. [ ] `effect { }` desugars to `flatMap` chains.
16. [ ] A runtime executor, with `main` as an effect.
17. [ ] An error row `E` on `Effect`, then a requirements row `R` satisfied at the platform
        boundary. Both reuse the record row engine.

## Memory management (runs alongside, gated on item 4)

The interpreter already frees memory through reference counting (`Rc`). The only leak is a
cycle, and recursive `let` is the first feature that creates one: a closure whose
environment contains itself.

18. [ ] **Recursive closures without reference cycles.** A closure re-binds its own name
        when called instead of storing itself in its environment, so `Rc` stays enough.
19. [ ] A test that recursive programs do not leak (count live closures, or a memory tool).
20. [ ] **Cycle collection, only if needed.** Mutable references or a hand-written VM would
        force it; an arena or a crate such as `gc` / `dumpster` covers it. If transpilation
        ships first, the target's GC does this for free.

## Side branch — Traits (not needed for effects)

21. [ ] `trait` and `impl … for T`, with type-directed resolution.
22. [ ] Operators move onto `Add` / `Eq` / `Ord` in `core`, removing the Int-only limit.
23. [ ] `Show`, and string interpolation built on it.
24. [ ] Higher-kinded parameters: one `Functor` / `Monad` for Option, Result and Effect.

## Future — Transpilation

25. [ ] Input is the typed AST after desugaring, so `|>`, currying and `effect { }` are gone.
26. [ ] Emit TypeScript for literals, records, lambdas, `if` and `let`.
27. [ ] Sum types become tagged unions; `match` becomes a `switch` on the tag.
28. [ ] Intrinsics map to the target runtime (`concat` → `+`, `print` → `console.log`).
29. [ ] `core` and user modules compile to ES modules.
30. [ ] Run `Effect` on a small JavaScript runtime.
31. [ ] Run the test corpus against transpiled output and check it matches the interpreter.
32. [ ] *(stretch)* Emit TypeScript types from Trestle types.
33. [ ] *(stretch)* Run transpiled output directly in the playground.

## Done recently

- [x] Let-generalisation with levels, including row variables and tails
      (`feat/RowVariables`, merged). The `captured_function_result_is_not_generalised`
      corpus test is un-ignored.
- [x] Type-check errors print types with `Display` (`Int`, not `Literal(Int)`), and a
      call-argument mismatch underlines the argument, not the whole call.
