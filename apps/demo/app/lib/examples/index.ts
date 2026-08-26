/**
 * Starter programs, lifted from the conformance corpus at
 * `crates/trestle/tests/programs/`. Using the real corpus means the playground demonstrates
 * exactly what the compiler is tested against — if an example stops working, that is a
 * genuine regression rather than playground rot.
 *
 * The set aims to cover every feature the compiler actually ships, which is to say every
 * program `crates/trestle/tests/corpus.rs` registers *without* an `ignore = "…"`. Tiers 02
 * (match), 04 (generics) and 05 (effects) are deliberately absent: that syntax is aspirational
 * and does not parse yet. Tier 03 is only partly here — records, field access and records
 * nested by naming the inner type work; ADTs, an *inline* record type in field position, and
 * field-call chains do not. The Features dialog lists what is missing and why.
 *
 * Programs are copied verbatim, with two exceptions. The stale `// @skip:` markers some corpus
 * files still carry are dropped, and where a file's preamble is a note to whoever is fixing the
 * unifier rather than something a reader wants, the commentary is rewritten — never the code,
 * which is what makes a broken example a real regression.
 *
 * A few are meant to fail. They are the only way to show what a diagnostic looks like, and
 * between them they cover every phase that can produce one: `parse`, `resolve`, `typecheck`
 * and `evaluate`.
 */

/** Dropdown sections, in the order they are rendered. */
export const EXAMPLE_GROUPS = [
  'Basics',
  'Functions',
  'Pipelines',
  'Types & records',
  'Diagnostics',
] as const

export type ExampleGroup = (typeof EXAMPLE_GROUPS)[number]

export type Example = {
  name: string
  group: ExampleGroup
  /** Where it came from, relative to `crates/trestle/tests/programs/`. */
  source: string
  description: string
  code: string
}

export const EXAMPLES: Example[] = [
  // ── Basics ──────────────────────────────────────────────
  {
    name: 'literals',
    group: 'Basics',
    source: '00-basics/literals/every-literal',
    description: 'Every literal form the language has, one binding each.',
    code: `// Every literal form the grammar has, one binding each. \`unit\` is spelled as a bare word
// rather than \`()\`, and a \`{ … }\` in value position is a record literal, not a block.
// Open the Bindings tab to see the type inference settled on for each of them.

let count   = 42
let ratio   = 3.14
let name    = "trestle"
let ready   = true
let nothing = unit

let origin = { x: 0, y: 0 }

origin
`,
  },
  {
    name: 'arithmetic',
    group: 'Basics',
    source: '00-basics/bindings/arithmetic',
    description: 'Integer arithmetic and references to earlier bindings.',
    code: `// Arithmetic — addition and multiplication over integers and variables.

let a = 1 + 2 + 3        // left-associative: ((1 + 2) + 3) = 6
let b = 2 * 3 * 4        // 24
let c = a + b            // references earlier bindings
c
`,
  },
  {
    name: 'precedence',
    group: 'Basics',
    source: '00-basics/operators/precedence-and-grouping',
    description: '`*` binds tighter than `+`; parentheses override it.',
    code: `// Precedence & grouping — \`*\` binds tighter than \`+\`; parens override it.

let natural = 1 + 2 * 3       // 1 + (2 * 3) = 7
let grouped = (1 + 2) * 3     // 9
let nested  = ((1 + 2) * (3 + 4))
nested
`,
  },
  {
    name: 'comparison-and-logic',
    group: 'Basics',
    source: '00-basics/operators/comparison-and-logic',
    description: 'The six comparisons and the three logical combinators, composed.',
    code: `// The boolean expressions: the six comparisons, and the three logical combinators over
// their results. Comparison is \`Int\` against \`Int\` today — comparing Strings, Floats or
// Bools is a type error until operators dispatch through traits.

let low  = 3
let high = 9

let ordered  = low < high && high > low          // true
let bounded  = low <= 3 && high >= 9             // true
let distinct = low != high && !(low == high)     // true

ordered && bounded && distinct
`,
  },
  {
    name: 'conditionals',
    group: 'Basics',
    source: '00-basics/conditionals/if-else-expression',
    description: '`if (cond) a else b` — a conditional is an expression, so it has a value.',
    code: `// \`if\` is an expression, not a statement: it evaluates to whichever branch it takes, so it
// can be the whole body of a function. The condition is parenthesized; \`else\` is optional,
// and an \`if\` without one evaluates to \`unit\` when the condition is false.

let abs = (n: Int) => if (n < 0) 0 - n else n
let bigger = (n: Int) =>
    if (n > 3)
        "Bigger"
    else
        "Smaller"

let valueTrue = bigger(abs(-10))
let falseValue = bigger(0)
falseValue
`,
  },
  {
    name: 'conditional-blocks',
    group: 'Basics',
    source: '00-basics/blocks/if-else-block',
    description: 'Either branch of an if/else can be a block.',
    code: `// Both branches of an if/else can be blocks; each block's last expression is the
// value of that branch.

let bigger =
    if (3 > 2) {
        let a = 5
        a + a   // 10
    } else {
        20
    }
bigger
`,
  },
  {
    name: 'blocks',
    group: 'Basics',
    source: '00-basics/blocks/block-with-bindings',
    description: 'A block is a sub-program with its own scope, valued at its last expression.',
    code: `// A block is a true sub-program: it can introduce its own \`let\` bindings, scoped
// to the block, then produce a final value. \`a\` and \`b\` are not visible outside.

let total = {
    let a = 10
    let b = 20
    a + b   // 30
}
total
`,
  },
  {
    name: 'shadowing',
    group: 'Basics',
    source: '00-basics/blocks/shadowing',
    description: 'A block-local binding masks an outer one without mutating it.',
    code: `// Shadowing: a block-local \`let\` may reuse a name from an enclosing scope. Inside
// the block the inner binding wins; the outer binding is untouched and visible
// again once the block closes — shadowing masks, it does not mutate.

let x = 1
let inner = {
    let x = 100   // shadows the outer \`x\` within this block
    x             // 100
}
inner + x         // 100 + 1 = 101
`,
  },

  // ── Functions ───────────────────────────────────────────
  {
    name: 'functions',
    group: 'Functions',
    source: '00-basics/functions/function-invocation',
    description: 'A function is a lambda bound with `let`, then invoked by name.',
    code: `// There is no \`fn\` keyword: a function is an arrow lambda bound with \`let\`, and calling
// it is the ordinary \`name(args)\` form. double(2) evaluates to 4.

let double = (x: Int) => x * 2
double(2)
`,
  },
  {
    name: 'currying',
    group: 'Functions',
    source: '00-basics/functions/currying',
    description: 'Every function is curried; applying too few arguments returns a function.',
    code: `// Currying — every function is curried.
//   (a, b) => body   is sugar for   (a) => (b) => body
//   f(a, b)          is sugar for   f(a)(b)

let add = (a: Int, b: Int) => a + b

let add10 = add(10)   // partial application: a function awaiting \`b\`

let z = add10(5)      // add(10)(5) = 15
let w = add(3, 4)     // add(3)(4)  = 7
z
`,
  },
  {
    name: 'partial-application',
    group: 'Functions',
    source: '00-basics/functions/partial-application',
    description: 'The mechanism the pipe operator relies on.',
    code: `// TARGET: applying fewer arguments than a function takes returns a function
// awaiting the rest. This is the mechanism the pipe operator relies on.

let add   = (a: Int, b: Int) => a + b
let mul   = (a: Int, b: Int) => a * b

let inc     = add(1)     // awaits \`b\`; inc(n) = n + 1
let triple  = mul(3)     // awaits \`b\`; triple(n) = n * 3

let result = triple(inc(9))   // triple(10) => 30
result
`,
  },
  {
    name: 'closures',
    group: 'Functions',
    source: '00-basics/functions/closures',
    description: 'A returned function keeps the binding it captured.',
    code: `let create_closure = () => {
    let my_value = 1

    () => my_value
}

let closure = create_closure()

closure()
`,
  },
  {
    name: 'inferred-parameter',
    group: 'Functions',
    source: '01-unification/lambda-parameters',
    description: 'An unannotated parameter — inference works out that it is an Int.',
    code: `let add3 = (a) => a + 3

add3(2)
`,
  },
  {
    name: 'partially-known-function',
    group: 'Functions',
    source: '01-unification/partially-known-function',
    description: 'A function passed as an argument, with neither parameter annotated.',
    code: `// Neither of \`apply\`'s parameters is annotated, so both start as type variables. Passing
// \`inc\` in is what settles them: \`f\` unifies with \`Int => Int\` and \`x\` with \`Int\`. Hover
// \`apply\`, or open the Bindings tab, to see the type inference arrived at.

let inc = (n: Int) => n + 1
let apply = (f, x) => f(x)

apply(inc, 1)
`,
  },

  // ── Pipelines ───────────────────────────────────────────
  {
    name: 'pipeline',
    group: 'Pipelines',
    source: '01-pipelines/pipeline',
    description: 'The |> operator is "dumb": x |> f is f(x).',
    code: `// Pipeline — the |> operator is "dumb": x |> f  ==  f(x)
// A line that begins with |> continues the previous expression
// (like Kotlin's leading \`.\`), so the chain needs no semicolons.

let add = (a: Int, b: Int) => a + b
let print = (value: Int) => value

let main =
    5
    |> add(3)    // add(3)(5)  = 8
    |> ((value: Int) => value * 2)
    |> print     // prints 16

main
`,
  },
  {
    name: 'single-line-pipe',
    group: 'Pipelines',
    source: '01-pipelines/single-line-pipe',
    description: 'The whole chain on one line still means nested application.',
    code: `// TARGET: the whole chain on one line still means nested application.
//   x |> f |> g  ==  g(f(x))

let double = (x: Int) => x * 2
let add = (a: Int, b: Int) => a + b

let result = 5 |> add(3) |> double    // double(add(3)(5)) = 16
result
`,
  },
  {
    name: 'builder-as-pipeline',
    group: 'Pipelines',
    source: '01-pipelines/builder-as-pipeline',
    description: 'Builders are just pipelines — data-last curried functions replace method chains.',
    code: `// "Builders are just pipelines" — data-last, curried functions chained
// with |> replace fluent method chains (Trestle has no \`.\` method calls).

let withHost = (host: String, config: String) => config     // stubbed transforms; data-last
let withPort = (port: Int, config: String) => config
let build    = (config: String) => config

let emptyConfig = "This is an example builder pattern"

let server =
    emptyConfig
    |> withHost("localhost")
    |> withPort(8080)
    |> build

server
`,
  },
  {
    name: 'record-builder',
    group: 'Pipelines',
    source: '03-records-and-adts/record-builder-pipeline',
    description: 'The same pattern over a nested record — each step rebuilds rather than mutates.',
    code: `// Builders are pipelines, part two — this time the thing being built is a nested record.
//
// Every step is data-last and curried, so \`withName("api")\` is a \`Server => Server\` still
// waiting for its record, and \`|>\` threads one step's result into the next. There is no
// record-update syntax yet — \`{ ...server, name: name }\` is what row polymorphism will
// bring — so each step spells out the fields it keeps. That makes the important property
// visible rather than hidden: a step returns a *new* record, it does not mutate the one it
// was handed.

type Address = { host: String, port: Int }
type Retry   = { attempts: Int, backoffMs: Int }
type Server  = { name: String, address: Address, retry: Retry }

let base: Server = {
    name: "unnamed",
    address: { host: "localhost", port: 80 },
    retry: { attempts: 0, backoffMs: 0 }
}

let withName = (name: String, server: Server) => {
    name: name,
    address: server.address,
    retry: server.retry
}

let withAddress = (host: String, port: Int, server: Server) => {
    name: server.name,
    address: { host: host, port: port },
    retry: server.retry
}

let withRetry = (attempts: Int, backoffMs: Int, server: Server) => {
    name: server.name,
    address: server.address,
    retry: { attempts: attempts, backoffMs: backoffMs }
}

// A step that reads the record it is handed rather than only overwriting it: \`.\` chains
// through the nesting to reach the port, and the new port is derived from the old one.
let shiftPort = (offset: Int, server: Server) => {
    name: server.name,
    address: { host: server.address.host, port: server.address.port + offset },
    retry: server.retry
}

let server =
    base
    |> withName("api")
    |> withAddress("0.0.0.0", 8000)
    |> withRetry(3, 250)
    |> shiftPort(80)          // 8000 + 80 = 8080

server
`,
  },

  // ── Types & records ─────────────────────────────────────
  {
    name: 'type-aliases',
    group: 'Types & records',
    source: '01-unification/type-alias-declaration/named-alias',
    description: 'A type name is an alias — for a builtin, or for another alias.',
    code: `// A \`type\` declaration binds a name to a type. The name is an alias, not a new nominal
// type: \`Celsius\` and \`Int\` are the same type, so a \`Celsius\` still adds to an \`Int\`.
// An alias may name a builtin, or another alias.

type Celsius = Int
type Temperature = Celsius

let freezing: Celsius = 0
let boiling: Temperature = 100

let range = boiling - freezing   // 100
range
`,
  },
  {
    name: 'records',
    group: 'Types & records',
    source: '03-records-and-adts/records',
    description: 'Record types and record literals, via a `type` alias.',
    code: `type Point = { x: Int, y: Int }

let origin : Point = { x: 0, y: 0 }
let p      : Point = { x: 3, y: 4 }

p
`,
  },
  {
    name: 'field-access',
    group: 'Types & records',
    source: '03-records-and-adts/field-access',
    description: 'Reading fields back off a record with dot notation.',
    code: `type Point = { x: Int, y: Int }

let p: Point = { x: 3, y: 4 }

let sum = p.x + p.y     // 7

sum
`,
  },
  {
    name: 'inferred-record',
    group: 'Types & records',
    source: '03-records-and-adts/inferred-record-let',
    description: 'A record bound with no annotation — its type is inferred from the literal.',
    code: `// No annotation on \`p\`: the binding starts as a type variable, and unifying it against
// the record literal is what settles both the field types and the record's own type.
// Hover \`p\` in the editor, or open the Bindings tab, to see what inference decided.

let p = { x: 3, y: 4 }

p.x + p.y     // 7
`,
  },
  {
    name: 'nested-record',
    group: 'Types & records',
    source: '03-records-and-adts/nested-record-alias',
    description: 'A record type nested inside another, by naming the inner type first.',
    code: `// Nesting a record inside a record type means naming the inner type first: a field's
// annotation is a bare identifier, so \`value: { key: String }\` written inline does not
// parse yet. \`Nested\` still expands to the full nested type — hover \`nestedValue\`, or open
// the Bindings tab, to see it — and \`.\` chains as far as the nesting goes.

type Inner = { key: String, value: String }

type Nested = {
    name: String,
    value: Inner
}

let nestedValue: Nested = { name: "Sam", value: { key: "A key", value: "A value" } }

nestedValue.value.value       // "A value"
`,
  },
  {
    name: 'nested-field-access',
    group: 'Types & records',
    source: '03-records-and-adts/nested-field-access',
    description: 'A record literal inside a record literal, read through a chain of `.`.',
    code: `// A record literal nested inside a record literal, read through a chain of \`.\`. Nothing
// here is annotated: the shape of the whole thing is inferred from the literal alone.

let nestedValue = { name: "Sam", value: { key: "A key", value: "A value" } }

let key = nestedValue.value.key       // "A key"

key
`,
  },

  // ── Diagnostics ─────────────────────────────────────────
  // One per stage that can produce a diagnostic, so the panel has something real to show.
  {
    name: 'int-literal-out-of-range',
    group: 'Diagnostics',
    source: '00-basics/literals/int-literal-out-of-range',
    description: 'Deliberately broken — rejected while building the AST, before analysis.',
    code: `// Int literals are signed 64-bit. This one is one past the maximum, so it is rejected while
// the AST is being built — the grammar itself is happy with any run of digits, which makes
// this the earliest stage a program can fail at.

9223372036854775808
`,
  },
  {
    name: 'block-scope-leak',
    group: 'Diagnostics',
    source: '00-basics/blocks/block-scope-leak',
    description: 'Deliberately broken — a block-local binding does not leak. Shows diagnostics.',
    code: `// A block-local binding is scoped to its block. Referencing it after the block
// closes is an unbound-name error, caught at analysis time: the binding does
// not leak into the enclosing scope.

let x = {
    let inner = 1 + 1
    inner + 10
}
inner
`,
  },
  {
    name: 'duplicate-binding',
    group: 'Diagnostics',
    source: '00-basics/bindings/duplicate-binding',
    description: 'Deliberately broken — a name declared twice in one scope. Two labels.',
    code: `// A name may not be declared twice in the same scope. Shadowing requires a *new*
// scope (a block or a lambda param); re-\`let\`-ing \`x\` at the same level is a
// \`DuplicateBinding\` error, caught at analysis time.

let x = 1
let x = 2
x
`,
  },
  {
    name: 'infinite-type',
    group: 'Diagnostics',
    source: '01-unification/infinite-type',
    description: 'Deliberately broken — a type that would have to contain itself.',
    code: `// The occurs check.
//
// \`x\` is unannotated, so it starts as a type variable. Applying \`x\` to itself constrains
// that variable to \`Fn(x, _)\` — a type that contains itself, which no finite type can
// satisfy. The checker rejects it rather than building a cyclic type.

let selfApply = (x) => x(x)
`,
  },
  {
    name: 'division-by-zero',
    group: 'Diagnostics',
    source: '00-basics/operators/division-by-zero',
    description: 'Deliberately broken — type-checks, then faults while evaluating.',
    code: `// A well-typed program that faults at runtime: \`Int / Int\` type-checks, and the checker has
// no way to know the divisor is zero. Raised as an \`EvalError\` rather than left to panic,
// because a panic under WebAssembly is a trap with no source position attached.
let numerator = 10
let divisor = 0

numerator / divisor
`,
  },
  {
    name: 'arithmetic-overflow',
    group: 'Diagnostics',
    source: '00-basics/operators/arithmetic-overflow',
    description: 'Deliberately broken — the other well-typed runtime fault.',
    code: `// The other well-typed runtime fault. Left unchecked this is worse than a panic: \`+\` wraps
// silently in a release build while panicking in a debug one, so the tested behaviour and
// the shipped behaviour differ.
let max = 9223372036854775807

max + 1
`,
  },
]

/** The program a brand-new tab starts with. */
export const BLANK_PROGRAM = `// A new Trestle program.

let greeting = "hello"
let answer = 6 * 7

answer
`
