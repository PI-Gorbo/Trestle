# Trestle — Deployment

_Last updated: 2026-09-25_

The playground at `apps/demo` is the only deployable thing in this repository. It ships to
Cloudflare as an assets-only Worker, built and published on every push to `main` by
[`deploy-to-cloudflare.yml`](../../.github/workflows/deploy-to-cloudflare.yml).

## What ships

There is no server-side Rust. `crates/trestle-wasm` is compiled by `wasm-pack --target web`
into an ES module and a `.wasm` file, which Vite fingerprints and emits as an ordinary asset
alongside the rest of the bundle. Cloudflare only ever serves those bytes — the compiler is
instantiated and run in a Web Worker on the visitor's machine.

That makes the deployment target unusually simple. `apps/demo/wrangler.jsonc` declares no
`main`, so there is no Worker script and nothing executes at the edge:

```jsonc
{
  "name": "trestle-web",
  "routes": [{ "pattern": "trestle.samstack.org", "custom_domain": true }],
  "assets": {
    "directory": "./.output/public",
    "not_found_handling": "single-page-application"
  }
}
```

`not_found_handling` is what makes an unknown path return the app shell at 200 rather than
Nuxt's `404.html` at 404. The app is a single client-routed page, so without it every URL
other than `/` would look broken.

There is a great deal of headroom:

| | Deployed | Limit |
|---|---|---|
| Files | 27 | 20,000 on the free plan |
| Largest file | 663 KB — the wasm | 25 MiB |

Requests for static assets are neither billed nor counted, and stored assets cost nothing,
so the running cost of the playground is zero.

## Where it lives

The playground is served from **`trestle.samstack.org`**.

Until 2026-09-25 it was on the apex, `samstack.org`, attached as a Custom Domain through the
Cloudflare dashboard — which meant the single most important fact about the deployment,
where it answers, was recorded nowhere in this repository. The apex now serves a blog from
a different repository, and the playground moved down a level.

The hostname is declared in `wrangler.jsonc` under `routes`, so a change of address shows up
in a diff.

### That list is a replacement, not an addition

Worth internalising, because getting it wrong takes the site down rather than merely failing.

`wrangler deploy` only touches Custom Domains at all if the config declares at least one —
which is why the dashboard-attached apex survived months of deploys from a config with no
`routes` block. But the moment one is declared, wrangler sends the whole list with
`replace_state=true`. **Any Custom Domain attached to `trestle-web` but missing from this
file is removed, DNS record included.** Cloudflare's own wording: *"If you change your routes
in the dashboard, Wrangler will override them in the next deploy."*

The apex handover was sequenced around that:

1. List **both** `samstack.org` and `trestle.samstack.org` here, and deploy. The subdomain is
   created; the apex is untouched because it is still in the list.
2. Deploy the blog Worker with `samstack.org` in *its* routes. In CI this reassigns the
   existing record in place — a GitHub runner has no TTY, so wrangler sets
   `override_existing_origin` and `override_existing_dns_record` instead of prompting. No
   delete, no NXDOMAIN window, no new certificate.
3. Only then remove `samstack.org` from this file.

Between steps 2 and 3, any deploy of this repository silently takes the apex back. Keep that
window to minutes.

Had the order been reversed — dropping the apex here before the blog claimed it — the record
would have been deleted outright. Cloudflare's SOA negative-cache TTL is 1800 seconds, so a
resolver that asked during the gap would keep answering NXDOMAIN for up to half an hour after
the fix landed.

Retiring a hostname for good is still a dashboard action if it is the last one on a script,
and the Advanced Certificate Cloudflare issued for it is never auto-deleted — check
**SSL/TLS → Edge Certificates** afterwards.

## Why Workers, not Pages

Pages is in absorb-and-maintain mode. Existing projects keep working and keep receiving bug
fixes, but since Workers gained native static-asset serving, Cloudflare's own guidance for
new projects is Workers, and new capability — Durable Objects, Cron Triggers, gradual
deployments, observability, the Vite plugin — lands there rather than on Pages.

**Nothing was invested in Pages when the choice was made**, so there was no migration cost
on the other side of the ledger. For a site with no server component the two are
near-identical in practice; the difference is which of them will still be gaining features
in two years.

## Why the wasm is built in CI

Cloudflare's build images carry Node, Bun, Go, Python and Ruby. They do not carry a Rust
toolchain, and they do not carry `wasm-pack`. Handing the build to Cloudflare — through the
Git integration, or a `build` block in `wrangler.jsonc` — therefore cannot work.

The reason it has to be avoided rather than merely worked around is that **the failure is
silent**. `apps/demo/scripts/build-wasm.mjs` exits with a clear message when `cargo` is
missing, but `nuxt generate` afterwards still succeeds: the app resolves the compiler
through `import.meta.glob`, so an absent package is an empty match rather than a resolve
error. The published site would load, render, and report itself as unavailable — the
degraded mode set out under *The compiler* in the
[playground's README](../../apps/demo/README.md).

Building in GitHub Actions, where `dtolnay/rust-toolchain` and `jetli/wasm-pack-action`
supply what is missing, and uploading the finished directory, keeps that failure loud. The
workflow also asserts the artifact is present before publishing anything:

```yaml
- name: Check the compiler was bundled
  run: ls .output/public/_nuxt/*.wasm
```

## One-time setup

Done for this repository on 2026-08-25, and recorded here so it can be redone against a
different Cloudflare account.

Nothing needs creating in the dashboard beforehand — the first `wrangler deploy` creates the
Worker named in `wrangler.jsonc`. What is needed is an account ID and a token.
`pnpm dlx wrangler login`, then `pnpm dlx wrangler whoami`, prints the 32-character account
ID. The token is minted under **My Profile → API Tokens → Create Token → Create Custom
Token**:

| Setting | Value |
|---|---|
| Permission policy | **Edit Cloudflare Workers** (the built-in template) |
| Account resources | Include → the account |
| Zone resources | Include → `samstack.org` |
| TTL | Optional, but a start and end date is cheap insurance |

An assets-only deploy with no `routes` needs only *Account → Workers Scripts → Edit* plus
*Account → Account Settings → Read*, and that is what this repository used until
2026-09-25. Declaring a Custom Domain in `wrangler.jsonc` asks for more: wrangler has to
create a DNS record and a certificate in the zone. The *Edit Cloudflare Workers* template
covers that, which is also what Cloudflare's own CI guidance recommends.

A token scoped to *Cloudflare Pages → Edit* does **not** work — that is a different
permission, and it fails with error 10000.

Both values then go in as repository secrets, under **Settings → Secrets and variables →
Actions**, or:

```
gh secret set CLOUDFLARE_API_TOKEN
gh secret set CLOUDFLARE_ACCOUNT_ID
```

The names are read verbatim by the workflow and have to match.

## Deploying

A push to `main` touching `apps/demo/**`, `crates/**`, `Cargo.toml`, `Cargo.lock` or the
workflow file publishes. Pull requests run the same build, lint, typecheck and wire-format
checks but stop short of publishing, and attach the built site to the run as an artifact
instead. **Actions → deploy to cloudflare → Run workflow** on `main` publishes too, which is
how to redeploy without a code change.

Concurrency is per-ref. Pull request runs cancel when superseded; `main` runs queue instead,
so the deployed site ends up matching the newest commit rather than whichever build happened
to finish last.

The Worker is named `trestle-web`, so the site is at `trestle-web.<subdomain>.workers.dev`,
and the finished run writes the exact URL into its job summary.

To serve the built site locally exactly as Cloudflare will:

```
cd apps/demo
pnpm build
pnpm dlx wrangler dev
```

## Verifying

A green run is not sufficient on its own — it proves the wasm was in the directory, not that
it loads. Open the site, then DevTools → Network filtered to `wasm`. One request for
`_nuxt/trestle_wasm_bg-*.wasm` should come back 200 with `content-type: application/wasm`
and the year-long `cache-control` described below.

Then compile something the compiler is meant to reject, and confirm a diagnostic renders.
That is the only check exercising the whole round trip — parse in Rust through to miette's
graphical output in the panel — rather than the UI's idea of its own state.

The caching is worth understanding, because Workers' default for static assets is "cache,
but revalidate every time", which would put a conditional request in front of the 663 KB
compiler on every visit. `apps/demo/public/_headers` pins the hashed assets instead:

```
/_nuxt/*
  Cache-Control: public, max-age=31536000, immutable
```

That is safe because Vite content-hashes everything under `_nuxt/`, the wasm included — the
URL changes whenever the bytes do. `index.html` is deliberately not covered, which is what
lets a new deployment be picked up at all.

## Troubleshooting

**`Error: No pnpm version is specified`.** `pnpm/action-setup` reads `package.json` from the
repository root by default, and this root is a Cargo workspace with no `package.json` at
all; `packageManager` lives in `apps/demo/package.json`. The step carries an explicit
`package_json_file` for exactly this reason. Note that `defaults.run.working-directory` does
not help, since it governs `run:` steps only and never `uses:` ones. This broke every run of
the workflow between its introduction and 2026-08-25.

**The site loads but shows `COMPILER UNAVAILABLE`.** The wasm was not in the published
directory. The *Check the compiler was bundled* step should have failed the run before this
reached production, so start at *Build the WebAssembly compiler* in the same run — a break
in `crates/trestle` is the usual cause.

**`Authentication error [code: 10000]`.** The token lacks *Workers Scripts → Edit*, or the
account ID belongs to a different account than the token does. A Pages-scoped token produces
this.

**`You need to register a workers.dev subdomain before publishing`.** A first-time account
has no `*.workers.dev` subdomain yet. Claim one under **Workers & Pages → Subdomain**, or
set `"workers_dev": false` and attach a custom domain instead.

**A console warning about wasm streaming compile.** Harmless. The wasm-bindgen glue catches
a failed `instantiateStreaming` and falls back to `arrayBuffer` and `instantiate`.
Cloudflare does send `application/wasm`, so it should not appear at all; if it does, it
costs a little startup time rather than correctness.

**The site 404s with `error code: 1042` shortly after the very first deploy.** Observed
once, on 2026-08-25: the first-ever deploy served correctly for a few minutes, then the
`workers.dev` hostname began returning 404 consistently, from more than one network. The
error comes from Cloudflare ahead of the Worker rather than from the assets, and a plain
redeploy — **Run workflow** on `main`, no code change — restored it permanently. Treat it as
first-time route registration settling rather than anything in `wrangler.jsonc`, and reach
for a redeploy before changing configuration.

**A deploy succeeded but the old site is still showing.** Hard-reload. Everything under
`_nuxt/` changes name every build, so a stale `index.html` in the browser cache is the only
thing that can pin a visitor to an old version.
