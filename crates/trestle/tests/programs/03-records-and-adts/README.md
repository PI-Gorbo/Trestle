# Tier 03 — Records and ADTs

Structured data. Records depend on the type machinery; ADTs additionally depend
on `match` (tier 02) to be consumed.

- `records` — nominal record types with named fields and record literals.
- `field-access` — `.` reads a record field (never a method call in Trestle).
- `inferred-record-let` — a record bound with no annotation, so its type is inferred.
- `nested-field-access` — a record inside a record, read with a `.` chain.
- `nested-record-alias` — a record type nested by *naming* the inner record type.
- `record-builder-pipeline` — tier 01's builder pattern over a nested record: data-last
  steps chained with `|>`, each rebuilding rather than mutating.
- `nested-record-types` — the same nesting written inline, without naming the inner type.
- `record-function-field` — a field holding a function, invoked through `.`.
- `field-call-chain` — `a.b().c`: field read, call, field read.
- `unit-variants` — variants with no payload, so the constructor name *is* the value.
- `positional-variants` — variants carrying an ordered, unlabelled payload.
- `record-variants` — variants whose payload is a record, so the fields are named.
- `qualified-constructors` — naming a variant through its type, `Colour.Red`.
- `algebraic-data-types` — sum types with constructors, consumed by `match`.

`records`, `field-access`, `inferred-record-let`, `nested-field-access`,
`nested-record-alias` and `record-builder-pipeline` are live through
evaluation. The rest are ignored *(proposed syntax)* — `nested-record-types`
specifically because a field's type annotation is a bare identifier today, so
an *inline* record type in field position does not parse.

The three variant-form programs are one per way a variant can carry data, and
each stops at declaring and constructing — no `match`. That is deliberate:
pattern matching is a separate tier-02 blocker, so each of the three can be
un-ignored the moment its own form parses, checks and evaluates, without
waiting on it. `algebraic-data-types` is the one that needs both, and so
lands last.

`qualified-constructors` is additive over `unit-variants` rather than an
alternative to it: bare constructors are the primary form, and qualification
is what a name collision across two types forces. It is recorded now so the
intent is pinned, but it can land well after the three forms above.
