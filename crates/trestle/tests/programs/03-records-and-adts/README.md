# Tier 03 — Records and ADTs

Structured data. Records depend on the type machinery; ADTs additionally depend
on `match` (tier 02) to be consumed.

- `records` — nominal record types with named fields and record literals.
- `field-access` — `.` reads a record field (never a method call in Trestle).
- `inferred-record-let` — a record bound with no annotation, so its type is inferred.
- `nested-field-access` — a record inside a record, read with a `.` chain.
- `nested-record-types` — a type alias whose field type is itself a record.
- `record-function-field` — a field holding a function, invoked through `.`.
- `field-call-chain` — `a.b().c`: field read, call, field read.
- `algebraic-data-types` — sum types with constructors, consumed by `match`.

`records` and `field-access` are live. `inferred-record-let` is live but currently
*failing*: inferring an unannotated record binding reaches the unimplemented
`Type::Record` arm of `root_occurs_in_type`. The rest are ignored *(proposed syntax)*.
