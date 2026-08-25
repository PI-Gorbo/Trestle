# Trestle Playground

Write, type-check and run Trestle programs in the browser. A single static deployable —
no server, no API. The compiler itself is `crates/trestle` compiled to WebAssembly and run
in a Web Worker.

```
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

pnpm install
pnpm build:wasm
pnpm dev          # http://localhost:4200
```

## The compiler

`pnpm build:wasm` builds `crates/trestle-wasm` (a thin `wasm-bindgen` shim over
`parse` → `analyse` → `evaluate`) into `app/lib/trestle-wasm/`. That directory is a build
artifact: it is gitignored and rebuilt by CI.

**It is not optional.** There is no second implementation to fall back to. Skip the step and
the page still loads — `pnpm lint`, `pnpm typecheck` and `pnpm build` all run with no Rust
toolchain, because the app resolves the package through `import.meta.glob` and an absent one
is simply an empty match — but the header shows a red `COMPILER UNAVAILABLE` badge, `Run` is
disabled, and every compile answers with the reason instead of a result.

## How it fits together

| Piece | Where |
|---|---|
| Wire format between Rust and TS | `crates/trestle-wasm/src/dto.rs` ↔ `app/lib/compiler/types.ts` |
| Byte offset → editor line/column | `crates/trestle-wasm/src/diagnostics.rs` |
| Worker pool, panic and timeout recovery | `app/lib/compiler/client.ts` |
| Monaco language, theme, tokenizer | `app/lib/monaco/trestle.ts` |
| Tabs, persisted to localStorage | `app/composables/usePrograms.ts` |

Diagnostics are read through the `miette::Diagnostic` trait rather than by matching on error
variants, so a new `TypeCheckError` variant surfaces in the editor with no change to either
side of the boundary. Each one also carries `miette`'s own graphical rendering — source
excerpt, caret art, every label — which the diagnostics panel prints verbatim. That is the
same renderer the CLI uses: `trestle-wasm` enables `miette/fancy-no-syscall`, which is
`fancy` minus `backtrace` and the terminal-probing crates that cannot work in a browser.

Compilation runs in a pool of up to four workers. That is partly for parallelism across
tabs, but mostly for isolation: `panic = "abort"` on wasm means a Rust panic is a trap with
nothing to catch on either side of the boundary — so a worker that traps or exceeds its
five-second budget is terminated, replaced, and reported as an internal compiler error rather
than left hanging. `crates/trestle` no longer has `todo!()` holes, but it does hold `expect`s
encoding grammar invariants, and its walkers recurse without a depth limit against a stack
smaller than a native one.

Faults a *correct* program can reach are diagnostics, not traps: an integer literal too large
for an `i64`, division by zero, arithmetic overflow.

## Deploying

`pnpm build` emits a static site to `.output/public`, deployable to any static host.

For Cloudflare Pages, from the repository root:

| Setting | Value |
|---|---|
| Build command | `cd apps/demo && pnpm install && pnpm build:wasm && pnpm build` |
| Output directory | `apps/demo/.output/public` |
| Node version | `22` |

The build host needs Rust, the `wasm32-unknown-unknown` target and `wasm-pack` for
`build:wasm`. Drop that step and the site still deploys, but it deploys a playground that
cannot compile anything.

## Examples

The Examples menu loads programs verbatim from the conformance corpus at
`crates/trestle/tests/programs/`. If one of them stops working, that is a real regression
rather than playground rot. Tiers 02 (`match`), 04 (generics) and 05 (effects) are
deliberately absent: that syntax does not parse yet. A handful of examples are programs the
compiler is *meant* to reject — they are there to show what a diagnostic looks like.
