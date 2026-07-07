# Tier 03 — Records and ADTs

Structured data. Records depend on the type machinery; ADTs additionally depend
on `match` (tier 02) to be consumed.

- `records` — nominal record types with named fields and record literals.
- `field-access` — `.` reads a record field (never a method call in Trestle).
- `algebraic-data-types` — sum types with constructors, consumed by `match`.

All ignored *(proposed syntax)*.
