# Tier 03 — Records and ADTs

Structured data. Records depend on the type machinery; ADTs additionally depend
on `match` (tier 02) to be consumed.

- `records` — nominal record types with named fields and record literals.
- `field-access` — `.` reads a record field (never a method call in Trestle).
- `inferred-record-let` — a record bound with no annotation, so its type is inferred.
- `nested-field-access` — a record inside a record, read with a `.` chain.
- `nested-record-alias` — a record type nested by *naming* the inner record type.
- `nested-record-types` — the same nesting written inline, without naming the inner type.
- `record-function-field` — a field holding a function, invoked through `.`.
- `field-call-chain` — `a.b().c`: field read, call, field read.
- `algebraic-data-types` — sum types with constructors, consumed by `match`.

`records`, `field-access`, `inferred-record-let`, `nested-field-access` and
`nested-record-alias` are live through evaluation. The rest are ignored *(proposed
syntax)* — `nested-record-types` specifically because a field's type annotation is a
bare identifier today, so an *inline* record type in field position does not parse.
