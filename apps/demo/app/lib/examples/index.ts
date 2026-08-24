/**
 * Starter programs, lifted from the conformance corpus at
 * `crates/trestle/tests/programs/`. Using the real corpus means the playground demonstrates
 * exactly what the compiler is tested against — if an example stops working, that is a
 * genuine regression rather than playground rot.
 *
 * Tiers 02 (match), 04 (generics) and 05 (effects) are deliberately absent: that syntax is
 * aspirational and does not parse yet.
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
]

/** The program a brand-new tab starts with. */
export const BLANK_PROGRAM = `// A new Trestle program.

let greeting = "hello"
let answer = 6 * 7

answer
`
