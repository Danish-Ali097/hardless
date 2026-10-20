# AGENTS.md

How coding agents (and humans) build, test and work on Hardless in this repo. Keep this file
current: if a command here stops working, fix the command or this file in the same change.

The product spec is [docs/PRD.md](docs/PRD.md). Decisions and deviations from it are logged
in [docs/decisions.md](docs/decisions.md).

## Status

Milestones **M0 (foundation)** and **M1 (printer and drawer over Web Serial)** are done. M2
(HTTP printers, scanner, CLI, Docker) is next.
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
pnpm run check:licenses      # NFR-14: shipped vs build-tool licences (license-policy.json)
```

One package: `pnpm --filter @hardless/dashboard test`.

### Generated code

```sh
cargo xtask gen-types     # Rust wire types -> packages/types/src (ts-rs). Never edit those files.
```

Run it after changing any type that derives `ts_rs::TS` in `crates/hardless-core`, and commit
the result. CI fails if `packages/types` is stale.

### WebAssembly engine and browser tests

```sh
cargo install wasm-bindgen-cli --version 0.2.129 --locked   # once; must match Cargo.lock
cargo test -p hardless-wasm --target wasm32-unknown-unknown  # engine tests inside WebAssembly
cargo xtask build-wasm                                       # -> packages/shim-web/wasm (standalone mode)
pnpm --filter @hardless/shim-web build
pnpm --filter @hardless/shim-web exec playwright install chromium   # once
pnpm --filter @hardless/dashboard build && cargo build -p hardless-cli   # server for the tests
pnpm --filter @hardless/shim-web test:browser              # Web Serial conformance (section 8.1)
pnpm --filter @hardless/example-react-pwa test:acceptance  # M1 acceptance, example app
```

### Running it

```sh
pnpm --filter @hardless/dashboard build   # embed the dashboard (rebuild the server after)
cargo run -p hardless-cli -- start        # http://localhost:8080 and https://localhost:8443
cargo run -p hardless-cli -- scan scanner-1 4006381333931     # drive devices from the CLI
cargo run -p hardless-cli -- receipts printer-1 --last
node scripts/print-sample.mjs             # print fixtures/escpos/33-full-receipt.bin to printer-1
pnpm --filter @hardless/example-react-pwa dev   # example POS app on http://localhost:5173
```

### ESC/POS fixtures

```sh
node scripts/gen-escpos-fixtures.mjs                     # regenerate fixtures/escpos/*.bin
INSTA_UPDATE=always cargo test -p hardless-escpos -p hardless-render   # then review the diffs
```

### Release notes

- Rust crates: Conventional Commits; `release-plz` writes changelogs (`release-plz.toml`).
- npm packages: add a changeset with `pnpm changeset` for every user-visible change.

## Repo layout

```
crates/hardless-core           Device trait, registry, event log, faults, clock, bridge types  [M0, M1]
crates/hardless-escpos         ESC/POS bytes -> AST (sans-IO); fuzz/ has the cargo-fuzz target  [M1]
crates/hardless-render         AST -> HTML and to_text (SVG/PNG in M2)                          [M1]
crates/hardless-devices        printer-escpos, cash-drawer (scanner, NFC later)                 [M1]
crates/hardless-http-dialects  ePOS-Print / WebPRNT-compatible XML -> AST                       [M2]
crates/hardless-server         axum: /bridge, /api/v1, /api/v1/stream, dashboard                [M1]
crates/hardless-mcp            MCP server for agents                                            [M3]
crates/hardless-wasm           the engine for shim-web standalone mode (wasm-bindgen)           [M1]
crates/hardless-cli            the `hardless` binary: `start` (more subcommands in M2)          [M1]
xtask/                         `cargo xtask gen-types`, `cargo xtask build-wasm`
fixtures/escpos/               golden ESC/POS inputs and AST/HTML/text snapshots
packages/types                 generated TS wire types (never edit by hand)
packages/shim-web              navigator.serial shim (usb, NDEFReader, wedge later)             [M1]
packages/dashboard             React + Vite + Tailwind UI served by hardless-server             [M1]
packages/extension             Chrome MV3 extension                                             [M3]
packages/playwright            Playwright fixture + matchers                                    [M3]
packages/npm-wrapper           `npx hardless`                                                   [release]
examples/react-pwa             example POS PWA, with the M1 acceptance test                     [M1]
scripts/                       licence check, fixture generator, print-sample
docs/PRD.md                    the spec; docs/decisions.md records decisions and deviations
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
- Licences follow `license-policy.json` (NFR-14). The dashboard build fails if it bundles a
  package whose licence may not ship, and writes `THIRD_PARTY_LICENSES.txt` into `dist/`.
- New dependencies outside PRD section 6 need a justification (maintenance, licence) in the PR
  and in `docs/decisions.md`.
