/**
 * What the language can do today, and what is coming.
 *
 * The source of truth for `shipped` is `crates/trestle/tests/corpus.rs`: a program registered
 * there *without* an `ignore = "…"` parses, analyses and evaluates, and is proven to by a
 * recorded snapshot. `planned` rows take their `note` from that file's `ignore` reasons where
 * one exists, and otherwise from `docs-and-learnings/state-and-plan/state-and-plan.md` §3 and
 * `design-decisions.md`.
 *
 * Deliberately *not* sourced from that status doc's §1 feature matrix, which has drifted: it
 * still lists `|>`, records, field access, typed `let`, zero-parameter lambdas and real
 * inference as missing, and all of those ship.
 *
 * `example` names an entry in `../examples` — the dialog opens it in a tab. Keep the two files
 * in step: every `example` here must exist there.
 */

export type Feature = {
  name: string
  /** A one-line snippet, short enough to sit inline beside the name. */
  syntax: string
  status: 'shipped' | 'planned'
  /** An `Example.name`. Shipped rows only — the dialog turns these into a link. */
  example?: string
  /** Planned: what it is waiting on. Shipped: the caveat a reader needs to know. */
  note?: string
}

export type FeatureGroup = {
  title: string
  features: Feature[]
}

export const FEATURE_GROUPS: FeatureGroup[] = [
  {
    title: 'Literals & values',
    features: [
      {
        name: 'Integers',
        syntax: '42',
        status: 'shipped',
        example: 'literals',
        note: 'Signed 64-bit. A literal past the maximum is rejected while the AST is built.',
      },
      {
        name: 'Floats',
        syntax: '3.14',
        status: 'shipped',
        example: 'literals',
        note: 'Written as digits, a dot, digits — `.5` and `1.` do not parse.',
      },
      { name: 'Strings', syntax: '"hello"', status: 'shipped', example: 'literals' },
      { name: 'Booleans', syntax: 'true / false', status: 'shipped', example: 'literals' },
      {
        name: 'Unit',
        syntax: 'unit',
        status: 'shipped',
        example: 'literals',
        note: 'Spelled as a bare word, not `()`.',
      },
      {
        name: 'Record literals',
        syntax: '{ x: 3, y: 4 }',
        status: 'shipped',
        example: 'inferred-record',
        note: 'In value position `{ … }` is a record; with expressions inside it is a block.',
      },
    ],
  },
  {
    title: 'Operators',
    features: [
      {
        name: 'Arithmetic',
        syntax: '+ - * /',
        status: 'shipped',
        example: 'arithmetic',
        note: '`Int` only, and `/` truncates toward zero.',
      },
      {
        name: 'Comparison',
        syntax: '< > <= >= == !=',
        status: 'shipped',
        example: 'comparison-and-logic',
        note: '`Int` against `Int` only — comparing Strings, Floats or Bools is a type error today.',
      },
      {
        name: 'Logical',
        syntax: '&& || !',
        status: 'shipped',
        example: 'comparison-and-logic',
        note: 'Both operands are evaluated; these do not short-circuit yet.',
      },
      { name: 'Negation', syntax: '-x', status: 'shipped', example: 'conditionals' },
      {
        name: 'Precedence & grouping',
        syntax: '1 + 2 * 3',
        status: 'shipped',
        example: 'precedence',
      },
      {
        name: 'Operators through traits',
        syntax: 'impl Add for Point',
        status: 'planned',
        note: 'Operators are hardcoded to `Int` today. They will be retrofitted onto `core` instances of `Add` / `Eq` / `Ord` once traits land, which is what removes the `Int`-only limit above.',
      },
    ],
  },
  {
    title: 'Bindings & scope',
    features: [
      { name: 'Bindings', syntax: 'let x = 42', status: 'shipped', example: 'arithmetic' },
      {
        name: 'Type annotations',
        syntax: 'let x: Int = 42',
        status: 'shipped',
        example: 'type-aliases',
        note: 'The annotation is a bare type name — inline record and function types are not accepted here yet.',
      },
      {
        name: 'Blocks',
        syntax: '{ let a = 10  a + 1 }',
        status: 'shipped',
        example: 'blocks',
        note: 'A block is an expression, valued at its last expression.',
      },
      {
        name: 'Block scoping',
        syntax: 'bindings do not leak',
        status: 'shipped',
        example: 'block-scope-leak',
      },
      { name: 'Shadowing', syntax: 'let x = 100', status: 'shipped', example: 'shadowing' },
      {
        name: 'Recursive bindings',
        syntax: 'let loop = () => loop()',
        status: 'planned',
        note: "A binding's value is resolved before its own name is bound, so a function cannot yet refer to itself. Mutual recursion needs the same pre-registration.",
      },
    ],
  },
  {
    title: 'Control flow',
    features: [
      {
        name: 'if',
        syntax: 'if (n > 3) "big"',
        status: 'shipped',
        example: 'conditionals',
        note: 'The condition is parenthesized. With no `else`, a false condition evaluates to `unit`.',
      },
      {
        name: 'if / else',
        syntax: 'if (c) a else b',
        status: 'shipped',
        example: 'conditionals',
        note: 'A conditional is an expression, so it has a value and can be a whole function body.',
      },
      {
        name: 'Branches as blocks',
        syntax: 'if (c) { … } else { … }',
        status: 'shipped',
        example: 'conditional-blocks',
      },
      {
        name: 'Pattern matching',
        syntax: 'match shape { … }',
        status: 'planned',
        note: 'needs match / pattern matching — proposed syntax',
      },
    ],
  },
  {
    title: 'Functions & pipelines',
    features: [
      {
        name: 'Lambdas',
        syntax: '(x: Int) => x * 2',
        status: 'shipped',
        example: 'functions',
        note: 'The only function form — a function is a lambda bound with `let`.',
      },
      { name: 'Zero-parameter lambdas', syntax: '() => 42', status: 'shipped', example: 'closures' },
      { name: 'Return type annotations', syntax: '(a: Int): Int => a', status: 'shipped' },
      {
        name: 'Currying',
        syntax: '(a, b) => body',
        status: 'shipped',
        example: 'currying',
        note: 'Every function is curried: `(a, b) => …` is sugar for `(a) => (b) => …`, and `f(a, b)` for `f(a)(b)`.',
      },
      {
        name: 'Partial application',
        syntax: 'add(10)',
        status: 'shipped',
        example: 'partial-application',
      },
      { name: 'Closures', syntax: '() => my_value', status: 'shipped', example: 'closures' },
      {
        name: 'Pipe operator',
        syntax: 'x |> f',
        status: 'shipped',
        example: 'single-line-pipe',
        note: 'Deliberately dumb: `x |> f` is exactly `f(x)`. All the expressiveness comes from currying.',
      },
      {
        name: 'Leading-pipe continuation',
        syntax: '|> add(3)',
        status: 'shipped',
        example: 'pipeline',
        note: 'A line beginning with `|>` continues the previous expression, so a chain needs no separators.',
      },
      {
        name: 'Builders as pipelines',
        syntax: 'config |> withHost("…")',
        status: 'shipped',
        example: 'builder-as-pipeline',
      },
      {
        name: 'Function types in annotations',
        syntax: 'f: (name: String) => Int',
        status: 'planned',
        note: 'needs function type expressions in annotations',
      },
    ],
  },
  {
    title: 'Types & records',
    features: [
      {
        name: 'Type aliases',
        syntax: 'type Celsius = Int',
        status: 'shipped',
        example: 'type-aliases',
        note: 'Structural, not nominal: `Celsius` *is* `Int`. Aliases chain.',
      },
      {
        name: 'Record types',
        syntax: 'type Point = { x: Int, y: Int }',
        status: 'shipped',
        example: 'records',
      },
      { name: 'Field access', syntax: 'p.x', status: 'shipped', example: 'field-access' },
      {
        name: 'Nested records',
        syntax: 'a.value.key',
        status: 'shipped',
        example: 'nested-field-access',
      },
      {
        name: 'Nested record types',
        syntax: 'value: Inner',
        status: 'shipped',
        example: 'nested-record',
        note: 'By naming the inner type first.',
      },
      {
        name: 'Rebuilding a record',
        syntax: '{ name: n, address: s.address }',
        status: 'shipped',
        example: 'record-builder',
        note: 'A transform spells out every field it keeps, because there is no record update yet — which is what the row polymorphism below brings.',
      },
      {
        name: 'Inline record types in field position',
        syntax: 'value: { key: String }',
        status: 'planned',
        note: 'needs *inline* record type expressions in field position',
      },
      {
        name: 'Mixed postfix chaining',
        syntax: 'a.b().c',
        status: 'planned',
        note: 'needs mixed postfix chaining (a.b().c)',
      },
      {
        name: 'ADTs / sum types',
        syntax: 'type Shape = | Circle { … }',
        status: 'planned',
        note: 'needs ADTs + constructors + match',
      },
      {
        name: 'Row polymorphism & `sat` bounds',
        syntax: '{ name: String, ... }',
        status: 'planned',
        note: 'Records unify on exactly equal field sets today. Rows add `{ name: T, ... }` for "at least these fields", with bounds written `T sat Show` — never `<:`.',
      },
    ],
  },
  {
    title: 'Type system',
    features: [
      {
        name: 'Hindley–Milner unification',
        syntax: 'inference, not annotation',
        status: 'shipped',
        example: 'inferred-parameter',
      },
      {
        name: 'Inferred parameters',
        syntax: '(a) => a + 3',
        status: 'shipped',
        example: 'inferred-parameter',
      },
      { name: 'Inferred records', syntax: 'let p = { x: 3 }', status: 'shipped', example: 'inferred-record' },
      {
        name: 'Partially known function types',
        syntax: '(f, x) => f(x)',
        status: 'shipped',
        example: 'partially-known-function',
      },
      {
        name: 'Occurs check',
        syntax: '(x) => x(x)',
        status: 'shipped',
        example: 'infinite-type',
        note: 'A type that would have to contain itself is rejected rather than built.',
      },
      {
        name: 'Polymorphic bindings',
        syntax: 'let id = (x) => x',
        status: 'planned',
        note: 'Inference is monomorphic today — a binding gets exactly one type, so a would-be generic function cannot be used at two different types.',
      },
      {
        name: 'Generics / type parameters',
        syntax: '<T>(x: T) => x',
        status: 'planned',
        note: 'needs type parameters',
      },
      {
        name: 'Generic data types',
        syntax: 'type Option<T>',
        status: 'planned',
        note: 'needs generic data types',
      },
      {
        name: 'Traits',
        syntax: 'impl Show for Point',
        status: 'planned',
        note: 'Nominal and Rust-shaped, but with higher-kinded parameters so `Functor` and `Monad` are one shared abstraction. The capstone of the type system, and what makes operators polymorphic.',
      },
    ],
  },
  {
    title: 'Diagnostics',
    features: [
      {
        name: 'Build errors',
        syntax: 'rejected before analysis',
        status: 'shipped',
        example: 'int-literal-out-of-range',
      },
      { name: 'Unbound names', syntax: 'inner', status: 'shipped', example: 'block-scope-leak' },
      {
        name: 'Duplicate bindings',
        syntax: 'let x = 1  let x = 2',
        status: 'shipped',
        example: 'duplicate-binding',
        note: 'Two labels — the diagnostic points at the original declaration as well as the redeclaration.',
      },
      { name: 'Type errors', syntax: 'InfiniteType', status: 'shipped', example: 'infinite-type' },
      {
        name: 'Runtime faults',
        syntax: '10 / 0',
        status: 'shipped',
        example: 'division-by-zero',
        note: 'Raised with a source position rather than left to panic, because a panic under WebAssembly is a trap with nothing attached.',
      },
      {
        name: 'Arithmetic overflow',
        syntax: 'max + 1',
        status: 'shipped',
        example: 'arithmetic-overflow',
        note: 'Checked rather than wrapped, so the tested behaviour and the shipped behaviour agree.',
      },
    ],
  },
  {
    title: 'On the roadmap',
    features: [
      {
        name: 'Intrinsics',
        syntax: 'print, readLine',
        status: 'planned',
        note: 'Native functions surfaced to Trestle for the leaf capabilities the language cannot express itself. There is no value-level standard library at all today — the examples stub `print` and `len` by hand.',
      },
      {
        name: 'A `core` library',
        syntax: 'Option, Result',
        status: 'planned',
        note: 'Written in Trestle and loaded before user code, so the interpreter stays small. Called `core`, not "prelude" — the language prefers plain words to functional-programming jargon.',
      },
      {
        name: 'Modules & imports',
        syntax: 'import',
        status: 'planned',
        note: 'A small auto-imported `core` plus explicit per-file imports. A package is runnable when it provides a `main`.',
      },
      {
        name: 'The effect system',
        syntax: 'effect { … }',
        status: 'planned',
        note: 'The heart of the language. `Effect` becomes a `core` type, `effect { }` desugars to `flatMap` chains, and errors and requirements accumulate as rows up the call graph until the runtime discharges them.',
      },
    ],
  },
]
