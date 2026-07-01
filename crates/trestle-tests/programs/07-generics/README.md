# Tier 07 — Generics & higher-order data types

**Status: pending** (all files `@skip`). **Phase 2. Syntax proposed, not settled.**

Type parameters — the machinery the effect type (tier 08) is built on, hence its
place near the top of the ladder.

## Covers (proposed)
- Generic functions (type parameters on `let`-bound arrows)
- Generic / higher-order data types (a type parameterised by another type)

## Prerequisites
Tier 06 (records/ADTs) — generic containers are parameterised ADTs/records.

## Open questions
- Type-parameter syntax: `<T>` (TypeScript/C#-style, matching the spec's
  `Effect<V,E,R>`) vs. something lighter.
- Are type annotations required, or is there inference (the Phase-2 goal)?
