# Trestle

Trestle is a functional programming language built as a structured learning project. The goal is to deeply understand language design and interpreter implementation by building one from first principles — lexer, parser, type system, and an effect system — rather than reaching for existing tooling. It is explicitly educational, not production-oriented.

Influences: **F#**, **C#**, **TypeScript**, and the **Effect.ts** library.

## Core Idea

Trestle treats **effects as first-class citizens**. An effect describes a computation that may require some context, may fail, and eventually produces a value:

```
Effect<Value, Error, Requirements>
```

Crucially, *not everything is an effect*. Plain values stay plain values, and are automatically lifted into the effect context only when a composition needs them to be. This keeps ordinary code ordinary and reserves the effect machinery for computations that actually need it.

## Language Concepts

### Composition

There are two primary ways to compose effects, and both desugar to the same underlying `flatMap` chains:

- **The pipe operator (`|>`)** — for point-free, linear transformations. A value flows left to right through a series of functions.
- **The `effect { }` block** — for imperative-style sequential code, where each step can depend on the results of previous ones.

The pipe operator and the effect system are central to Trestle's identity, not afterthoughts. They are the two faces of the same composition model: one reads as a pipeline, the other as a procedure.

### Error Handling — Railway-Oriented

There is no `try` / `catch` syntax. Errors are **values** that propagate through pipelines. A failure short-circuits the rest of the chain, and handler functions placed anywhere in a pipeline can intercept and respond to errors. This is the "railway" model: success and failure are two parallel tracks, and the plumbing routes computation along the right one.

### Dependency Injection

Dependency injection is a **language-level feature**, not a pattern bolted on top. When a computation needs some capability (a `Requirement`), that requirement propagates *upward* through the type system. The caller doesn't have to thread dependencies manually — the type of an effect records what it still needs, and those needs accumulate until something satisfies them.

### `main` and the Runtime Boundary

`main` is itself an effect. The runtime is the **boundary** of the program: it is the single place where *all* requirements must be satisfied and *all* errors must be handled. Inside the program you compose freely, leaving requirements unmet and errors unhandled; at the edge, the runtime closes the loop.

### Typing

Trestle uses **nominal typing** (types are equal by name, not by shape), chosen primarily for implementation simplicity.

## Roadmap

The project is phase-gated: each phase is completed before moving on to the next.

| Phase | Focus | Pipeline |
|-------|-------|----------|
| **1 — Interpreter core** *(current)* | Get programs running end to end | Lexer → Parser → Tree-Walk Interpreter |
| **2 — Type system** | Static analysis | Type inference + algebraic data types |
| **3 — Effect system** | The heart of Trestle | `Effect` type, `effect { }` syntax, runtime |
| **4 — Transpiler** *(optional)* | Output target | Transpile to TypeScript |

## Current Status — Phase 1, Project 1: The Lexer

The lexer converts raw source text into a flat list of tokens. For example:

```
let x = 5 + 3
```

becomes:

```
[Let, Ident("x"), Eq, Int(5), Plus, Int(3)]
```

The token set is being derived from the language's design decisions rather than guessed at up front — design first, then derive the implementation. Design work is being done across these categories before any code is written:

- Literals
- Identifiers and keywords
- Operators (including the `|>` pipe operator)
- Delimiters
- Structural syntax

These design exercises are now resolved for the Phase 1 core — see [Core Syntax Decisions (Phase 1)](#core-syntax-decisions-phase-1) below for the answers and the concrete token enum derived from them.

## Core Syntax Decisions (Phase 1)

These are the resolved answers to the design exercises above, for the Phase 1 core surface.
Worked examples live in [`snippets/`](snippets/).

- **Bindings & functions:** `let name = value;` — a function is just a `let` bound to an arrow.
- **Arrow functions:** `(x) => expr`; a multi-parameter arrow `(a, b) => body` is sugar for `(a) => (b) => body`.
- **Application is curried:** `f(a, b)` is sugar for `f(a)(b)`. Parens-and-commas always mean *curried application*, **never a tuple**. This is a deliberate divergence from F#, where `f(a, b)` passes a single tupled argument. (If tuples-as-values are ever added, they get their own distinct treatment.)
- **Partial application:** "stop early" — `add(3)` is a function awaiting the remaining argument.
- **Pipe `|>`:** the operator is *dumb*: `x |> f` ≡ `f(x)`. Expressiveness comes from currying, not from operator magic.
- **Statement terminator:** `;`. Newlines are insignificant (plain whitespace), so a multi-line pipe parses as one expression that ends at the `;`.
- **Comments:** `//` line comments.
- **Convention (not enforced):** design functions **data-last** so partial application fills the earlier arguments and the piped value drops into the final slot.

### Derived token enum

```ts
type Token =
  | { kind: "Let" }
  | { kind: "Ident"; value: string }
  | { kind: "Int"; value: number }
  | { kind: "Plus" }      // +
  | { kind: "Star" }      // *
  | { kind: "Eq" }        // =
  | { kind: "Arrow" }     // =>
  | { kind: "Pipe" }      // |>
  | { kind: "LParen" }    // (
  | { kind: "RParen" }    // )
  | { kind: "Comma" }     // ,
  | { kind: "Semicolon" } // ;
  | { kind: "Eof" };
// skipped as trivia: whitespace, newlines, // comments
```

**Lexer note.** The only subtlety in this set is *maximal munch* on the two-character
operators: when you read `=`, peek ahead — `>` makes it `Arrow`, otherwise `Eq`; when you
read `|`, the following `>` makes it `Pipe`. Everything else is a single character or a
straightforward identifier/number scan.

**Scope.** Strings, floats, booleans, and negative numbers are intentionally *not* in this
first token set — they are future literals, left out to keep the initial enum honest to
what has actually been decided.

## Implementation Notes

- **Implementation language:** TypeScript
- **Source file extension:** `.pr` *(under consideration)*

## Guiding Principles

- The token set — and more broadly, the implementation — emerges naturally from language design decisions. Design first, derive second.
- Start simple, expand incrementally.
- Build understanding from first principles rather than leaning on existing tooling.
- Work through conceptual design exercises before writing code.