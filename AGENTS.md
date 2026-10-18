# AGENTS.md

How coding agents (and humans) build, test and work on Hardless in this repo. Keep this file
current: if a command here stops working, fix the command or this file in the same change.

The product spec is [docs/PRD.md](docs/PRD.md). Decisions and deviations from it are logged
in [docs/decisions.md](docs/decisions.md).

## Status

Milestone **M0 (foundation)** is done. M1 (printer and drawer over Web Serial) is next.
Work one milestone at a time, in order (PRD section 13), and build bottom-up (PRD section 15).

## Prerequisites

| Tool       | Version                                      | Notes                                                                                          |
| ---------- | -------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Rust       | 1.99 (pinned in `rust-toolchain.toml`)       | `rustup` installs it, `rustfmt`, `clippy` and the `wasm32-unknown-unknown` target on first use |
| Rust MSRV  | 1.97 (`rust-version` in `Cargo.toml`)        | stable minus 2 (NFR-11); CI checks it                                                          |
| Node.js    | 22 or later                                  |                                                                                                |
| pnpm       | 12.14.0 (`packageManager` in `package.json`) | `npm i -g pnpm@12.14.0` or `corepack enable`                                                   |
| cargo-deny | 0.20 or later                                | `cargo install cargo-deny --locked` (only for the licence/advisory check)                      |

## Commands

Run from the repo root. These are exactly what CI runs (`.github/workflows/ci.yml`).

### Rust

```sh
cargo fmt --all -- --check                                        # formatting
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features                             # unit, integration, doc tests
cargo build --target wasm32-unknown-unknown -p hardless-core -p hardless-escpos -p hardless-render -p hardless-devices -p hardless-http-dialects -p hardless-wasm
cargo xtask gen-types --check                                     # packages/types is up to date
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo +1.97 check --workspace --all-targets --all-features        # MSRV
cargo deny check                                                  # licences (NFR-14), advisories, sources
```

Run one crate's tests: `cargo test -p hardless-core`. Run one FR's tests:
`cargo test -p hardless-core fr_03`.

Snapshot tests use `insta`. To accept new or changed snapshots, review them, then run the
test with `INSTA_UPDATE=always` (or `cargo insta review` if you have `cargo-insta`).

### TypeScript (pnpm workspace)

```sh
pnpm install --frozen-lockfile
pnpm lint            # ESLint (typescript-eslint, strict type-checked)
pnpm format:check    # Prettier; `pnpm format` to fix
pnpm typecheck       # tsc --noEmit in every package
pnpm -r test         # Vitest in every package
pnpm build           # every package; the dashboard builds to packages/dashboard/dist
pnpm run check:licenses      # NFR-14 licence check of production npm dependencies
```

One package: `pnpm --filter @hardless/dashboard test`.

### Generated code

```sh
cargo xtask gen-types     # Rust wire types -> packages/types/src (ts-rs). Never edit those files.
```

Run it after changing any type that derives `ts_rs::TS` in `crates/hardless-core`, and commit
the result. CI fails if `packages/types` is stale.

### Release notes

- Rust crates: Conventional Commits; `release-plz` writes changelogs (`release-plz.toml`).
- npm packages: add a changeset with `pnpm changeset` for every user-visible change.

## Repo layout

```
crates/hardless-core           Device trait, registry, event log, faults, clock (sans-IO)   [M0: done]
crates/hardless-escpos         ESC/POS bytes -> AST (sans-IO)                                 [M1]
crates/hardless-render         AST -> HTML / SVG / PNG, to_text (sans-IO)                    [M1/M2]
crates/hardless-devices        printer, drawer, scanner, NFC (sans-IO)                       [M1+]
crates/hardless-http-dialects  ePOS-Print / WebPRNT-compatible XML -> AST (sans-IO)          [M2]
crates/hardless-server         axum: /bridge, /api/v1, HTTP printers, dashboard              [M1]
crates/hardless-mcp            MCP server for agents                                         [M3]
crates/hardless-wasm           engine for shim-web standalone mode (wasm-bindgen)            [M1]
crates/hardless-cli            the `hardless` binary                                         [M2]
xtask/                         `cargo xtask gen-types`
packages/types                 generated TS wire types (never edit by hand)
packages/shim-web              navigator.serial / usb / NDEFReader / wedge shims             [M1]
packages/dashboard             React + Vite + Tailwind UI served by hardless-server           [M1]
packages/extension             Chrome MV3 extension                                          [M3]
packages/playwright            Playwright fixture + matchers                                 [M3]
packages/npm-wrapper           `npx hardless`                                                [release]
docs/PRD.md                    the spec
```

## Rules that CI or review enforces

- **Sans-IO engine.** `hardless-core`, `-escpos`, `-render`, `-devices`, `-http-dialects` do
  no I/O and read no clock. Devices get `now: Millis` from the caller; tests use `FakeClock`.
  No test may depend on wall-clock time.
- **No `unsafe`** outside future `hardless-transport-*` crates (workspace lint
  `unsafe_code = "forbid"`).
- **No panics on input.** `clippy::unwrap_used` and `clippy::expect_used` are denied outside
  tests. Parsers return errors or `Unknown` nodes.
- **Every FR has a test named after it**: Rust `fn fr_08_…`, TypeScript `it("FR-08: …")`.
- **Wire types live in Rust** and are generated into `packages/types`.
- **Public items are documented**: rustdoc (with an example) for Rust, TSDoc for TypeScript.
- **TypeScript**: `strict`, ESM only, no `any`.
- **Commits**: Conventional Commits that cite FR/NFR IDs, e.g.
  `feat(core): event log with per-device cap (FR-03)`.
- Do not change a public API, message format, endpoint path or fault ID from the PRD without
  asking first. If a browser spec contradicts the PRD, follow the spec and flag it.
- Never copy code from vendor SDKs; cite the public command reference in a comment.
- New dependencies outside PRD section 6 need a justification (maintenance, licence) in the PR
  and in `docs/decisions.md`.
