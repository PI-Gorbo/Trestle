# Tier 06 — Records & algebraic data types

**Status: pending** (all files `@skip`). **Phase 2. Syntax proposed, not settled.**

Introduces the first compound/nominal data types. This is where `.` enters the
language — reserved **exclusively** for record field access, never method calls.

## Covers (proposed)
- Record types and literals
- Field access with `.` (`point.x`)
- Algebraic data types (sum types) and their constructors

## Prerequisites
Tiers 01–04. Conceptually the start of Phase 2 (the type system).

## Open questions
- Record declaration syntax (structural literal vs. named `type` declaration —
  the spec commits to **nominal** typing).
- ADT declaration + constructor-application syntax.
- How construction interacts with the "parens-and-commas = curried application"
  rule.
