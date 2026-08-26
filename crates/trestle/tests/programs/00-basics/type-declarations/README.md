# House — type declarations

The `type` keyword as a subject in its own right: what may stand on the right-hand
side of `type X = …`, and what is rejected there. Records-as-*values* — literals,
`.` field access, nesting — are tier 03; this house is only about the declaration.

Placed last in tier 00 because the programs lean on typed `let` and lambdas from the
houses above.

## Working today

- **named-alias** — `type Celsius = Int`, `type Temperature = Celsius`. An alias is
  *structural*, not nominal: `Celsius` **is** `Int`, so a value annotated with it
  unifies with plain `Int` arithmetic. Aliases chain.
- **alias-to-every-builtin** — breadth where `named-alias` has depth: all five type
  names the prelude seeds (`Int`, `Float`, `String`, `Bool`, `Unit`) reached through
  an alias, each annotating a `let`.
- **record** — record types by shape: single-line, multi-line, and the two empty
  spellings `{}` and `{ }`. The program is declarations only, so it evaluates to
  `Unit`.
- **duplicate-type-declaration** — the same type name declared twice. This currently
  *succeeds*, the second declaration shadowing the first, where a repeated `let` in
  the same block is a `DuplicateBinding` error. The program exists to make that
  asymmetry visible, not to bless it; if types should reject redeclaration too, it
  becomes a negative test.

## Negative tests

- **unknown-type-name** — a right-hand side naming an undeclared type. Parses, then
  fails in `binding_resolution` with `UnboundTypeName`: the type namespace's
  counterpart to `UnboundName`.
- **duplicate-record-field** — a record type naming the same field twice. Rejected
  while the AST is still being built (`DuplicateRecordField`), so it carries
  `build_error` and no `ast` stage — there is no tree to snapshot.

## Not yet supported — function types

The right-hand side of a `type` declaration cannot yet be a function type:
`type_expression` in the grammar is `record_type_expression | type_identifier`. The
three programs below are registered and ignored, pinning the decided syntax.

**The form is `=>` with optional parameter names**, mirroring lambda syntax. A name
is documentation, not part of the type, so `(n: Int) => Int` and `(Int) => Int` are
the same type. A multi-parameter function type curries exactly as a multi-parameter
lambda does, and `() => Int` is the nullary case. All of this maps onto the
*existing* `Type::Fn(Option<Box<Type>>, Box<Type>)` — the type system already models
function types in full, so the blocker is the grammar, not the checker.

- **function-type** — the base case, the optional name, and the nullary `() => Int`.
- **function-type-curried** — `(a: Int, b: Int) => Int` alongside its explicit
  desugaring `(a: Int) => (b: Int) => Int`, plus partial application through the
  sugared alias.
- **function-type-higher-order** — a function type nested inside another's parameter
  list, both named (`IntOp`) and inline.

Two programs elsewhere wait on the same feature —
`00-basics/functions/function-typed-parameter` and
`03-records-and-adts/record-function-field` — which need it in *annotation* position
(`":" ~ identifier` today) rather than as a `type` right-hand side.
