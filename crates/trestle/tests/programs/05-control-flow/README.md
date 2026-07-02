# Tier 05 — Control flow

**Status: pending** (all files `@skip`). **Syntax proposed, not settled.**

Trestle is expression-oriented, so control flow produces values rather than
executing statements. The spec has not pinned this syntax yet — these files are
**design prompts**. Finalise the syntax before implementing.

## Covers (proposed)
- `if cond then a else b` as an expression
- `match` / pattern matching over values

## Prerequisites
Tier 04 (booleans/comparison give you conditions to branch on).

## Open questions
- `if/then/else` keyword form vs. a braced form?
- Is there an `else`-less `if`? (Probably not, if `if` is an expression.)
- What can patterns match in Phase 1, before ADTs (tier 06) exist — literals and
  identifiers only?
