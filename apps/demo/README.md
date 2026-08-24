# Trestle Playground

Write, type-check and run Trestle programs in the browser. A single static deployable —
no server, no API. The compiler itself is `crates/trestle` compiled to WebAssembly and run
in a Web Worker.

```
pnpm install
pnpm build:wasm   # optional; see "The compiler" below
pnpm dev          # http://localhost:4200
```

## The compiler

`pnpm build:wasm` builds `crates/trestle-wasm` (a thin `wasm-bindgen` shim over
`parse` → `analyse` → `evaluate`) into `app/lib/trestle-wasm/`. That directory is a build
artifact: it is gitignored and rebuilt by CI.

The app **works without it**. When the package is absent it falls back to a mock compiler
and says so, loudly, with an amber `MOCK COMPILER` badge in the header. The mock only
checks lexical structure — unbalanced brackets, unterminated strings, characters the
grammar cannot accept — and refuses to evaluate rather than inventing a value. It exists so
the playground is usable while `crates/trestle` is being worked on.

Prerequisites for the real build:

```
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

## How it fits together

| Piece | Where |
|---|---|
| Wire format between Rust and TS | `crates/trestle-wasm/src/dto.rs` ↔ `app/lib/compiler/types.ts` |
| Byte offset → editor line/column | `crates/trestle-wasm/src/diagnostics.rs` |
| Worker pool, panic and timeout recovery | `app/lib/compiler/client.ts` |
| Mock compiler | `app/lib/compiler/stub.ts` |
| Monaco language, theme, tokenizer | `app/lib/monaco/trestle.ts` |
| Tabs, persisted to localStorage | `app/composables/usePrograms.ts` |

Diagnostics are read through the `miette::Diagnostic` trait rather than by matching on error
variants, so a new `TypeCheckError` variant surfaces in the editor with no change to either
side of the boundary.

Compilation runs in a pool of up to four workers. That is partly for parallelism across
tabs, but mostly for isolation: `crates/trestle` still has reachable `todo!()` holes, and a
panicked WebAssembly instance is poisoned permanently — so a worker that traps or exceeds
its five-second budget is terminated, replaced, and reported as an internal compiler error
rather than left hanging.

## Deploying

`pnpm build` emits a static site to `.output/public`, deployable to any static host.

For Cloudflare Pages, from the repository root:

| Setting | Value |
|---|---|
| Build command | `cd apps/demo && pnpm install && pnpm build:wasm && pnpm build` |
| Output directory | `apps/demo/.output/public` |
| Node version | `22` |

The build host needs Rust, the `wasm32-unknown-unknown` target and `wasm-pack` for
`build:wasm`. Drop that step and the site still deploys — on the mock compiler.

## Examples

The Examples menu loads programs verbatim from the conformance corpus at
`crates/trestle/tests/programs/`. If one of them stops working, that is a real regression
rather than playground rot. Tiers 02 (`match`), 04 (generics) and 05 (effects) are
deliberately absent: that syntax does not parse yet.
