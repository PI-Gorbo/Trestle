# Tier 01 — Pipelines

Forward composition with the `|>` operator: `x |> f  ==  f(x)`. A line that
begins with `|>` continues the previous expression, so chains need no separators.
Builds directly on `00-basics/functions` — `|>` relies on data-last, partially
applied functions.

- `single-line-pipe` — a whole chain on one line: `x |> f |> g == g(f(x))`.
- `pipeline` — a multi-line chain with leading-`|>` continuation.
- `builder-as-pipeline` — "builders are just pipelines": data-last transforms
  chained with `|>` replace fluent method chains (Trestle has no `.` calls).

All ignored until the `|>` operator lands.
