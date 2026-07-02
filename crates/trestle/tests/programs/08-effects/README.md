# Tier 08 — Effects (the heart of Trestle)

**Status: pending** (all files `@skip`). **Phase 3. Syntax proposed, not settled.**

The end goal the whole ladder works backwards from: effects as first-class
citizens. `Effect<Value, Error, Requirements>` describes a computation that may
require context, may fail, and eventually produces a value.

## Covers (proposed)
- `effect { }` blocks — imperative-style sequential composition (desugars to
  `flatMap` chains, the sibling of `|>`)
- Railway-oriented error handling — errors are values that short-circuit
- Dependency injection — requirements propagate upward through the type
- `main` as an effect; the runtime is the boundary that satisfies requirements
  and handles errors

## Prerequisites
Tier 07 (generics) — `Effect<V, E, R>` is a three-parameter generic type.
Tiers 06 (ADTs, for error types) and 03 (`|>`) also feed in.

## Open questions
Essentially all of the effect-system surface — this tier is the design
destination, not a near-term parser target. Keep these as north-star examples.
