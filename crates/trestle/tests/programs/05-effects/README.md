# Tier 05 — Effects

The capstone: a monadic effect system for I/O and error handling. Reads as the
imperative twin of the `|>` pipeline (tier 01), and leans on most earlier tiers.

- `effect-block` — `effect { }` sequences steps where each can depend on earlier
  results.
- `main-as-effect` — `main` is itself an effect; the runtime is where
  capabilities are satisfied and errors handled.
- `railway-errors` — errors are values that short-circuit a pipeline; a handler
  placed anywhere can intercept them. No try/catch.

All ignored *(proposed syntax)*.
