/**
 * Starter programs, lifted from the conformance corpus at
 * `crates/trestle/tests/programs/`. Using the real corpus means the playground demonstrates
 * exactly what the compiler is tested against — if an example stops working, that is a
 * genuine regression rather than playground rot.
 *
 * Tiers 02 (match), 04 (generics) and 05 (effects) are deliberately absent: that syntax is
 * aspirational and does not parse yet. Tier 03 is only partly here — records and field access
 * work; ADTs, nested record types and field-call chains do not.
 *
 * Programs are copied verbatim, with two exceptions. The stale `// @skip:` markers some corpus
 * files still carry are dropped, and where a file's preamble is a note to whoever is fixing the
 * unifier rather than something a reader wants, the commentary is rewritten — never the code,
 * which is what makes a broken example a real regression.
 *
 * A few are meant to fail. They are the only way to show what a diagnostic looks like, and
 * they cover the three stages that can produce one: analysis, type checking, and evaluation.
 */

export type Example = {
  name: string
  /** Where it came from, relative to `crates/trestle/tests/programs/`. */
  source: string
  description: string
  code: string
}

export const EXAMPLES: Example[] = [
  {
    name: 'arithmetic',
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
    name: 'shadowing',
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
  {
    name: 'currying',
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
    source: '01-unification/lambda-parameters',
    description: 'An unannotated parameter — inference works out that it is an Int.',
    code: `let add3 = (a) => a + 3

add3(2)
`,
  },
  {
    name: 'pipeline',
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
    name: 'records',
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
    name: 'block-scope-leak',
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
]

/** The program a brand-new tab starts with. */
export const BLANK_PROGRAM = `// A new Trestle program.

let greeting = "hello"
let answer = 6 * 7

answer
`
