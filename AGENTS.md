# AGENTS.md

How coding agents (and humans) build, test and work on Hardless in this repo. Keep this file
current: if a command here stops working, fix the command or this file in the same change.

The product spec is [docs/PRD.md](docs/PRD.md). Decisions and deviations from it are logged
in [docs/decisions.md](docs/decisions.md).

## Status

Milestones **M0 (foundation)**, **M1 (printer and drawer over Web Serial)** and **M2
(cross-browser paths: HTTP printers, HTTPS, scanner, scenarios, CLI, Docker)** are done. **M3**
(WebUSB, Web NFC, event log, Chrome extension, Playwright fixture, MCP server, docs) is in
progress; the release (v0.1.0) follows. Work one milestone at a time, in order (PRD section
13), and build bottom-up (PRD section 15).

User and reference docs live in [docs/](docs/quickstart.md); [llms.txt](llms.txt) indexes them
for LLMs.

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
cargo xtask docs --check                                          # docs/reference and llms.txt are up to date
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
cargo xtask licenses      # Rust crates + Go Mono font + dashboard -> crates/hardless-cli/THIRD_PARTY_LICENSES.txt
cargo xtask docs          # device, fault, ESC/POS, HTTP dialect and MCP tool references -> docs/reference, and llms.txt
```

Run it after changing any type that derives `ts_rs::TS` in `crates/hardless-core`, and commit
the result. CI fails if `packages/types` is stale.

Run `cargo xtask licenses` after changing Rust dependencies or the dashboard's bundled
packages (build the dashboard first), and commit the result; CI fails if it is stale. It needs
`cargo install cargo-about --version 0.9.2 --locked --features cli`.

Run `cargo xtask docs` after changing device configs, faults, the ESC/POS or HTTP dialect
READMEs, MCP tools, or any page in `docs/`; CI fails if `docs/reference/` or `llms.txt` is
stale. A new page in `docs/` must be added to `PAGES` in `xtask/src/docs.rs`.

### WebAssembly engine and browser tests

```sh
cargo install wasm-bindgen-cli --version 0.2.129 --locked   # once; must match Cargo.lock
cargo test -p hardless-wasm --target wasm32-unknown-unknown  # engine tests inside WebAssembly
cargo xtask build-wasm                                       # -> packages/shim-web/wasm (standalone mode)
pnpm --filter @hardless/shim-web build
pnpm --filter @hardless/shim-web exec playwright install chromium   # once
pnpm --filter @hardless/dashboard build && cargo build -p hardless-cli   # server for the tests
pnpm --filter @hardless/shim-web test:browser              # shim conformance: Web Serial, WebUSB, Web NFC, wedge
pnpm --filter @hardless/example-react-pwa test:acceptance  # M1, M2 and M3 acceptance (M3 uses @hardless/playwright)
pnpm --filter @hardless/extension build && pnpm --filter @hardless/extension test:e2e   # Chrome extension
cargo test -p hardless-cli --test fr_43_mcp                # MCP server, with the SDK's client
```

### Running it

```sh
pnpm --filter @hardless/dashboard build   # embed the dashboard (rebuild the server after)
cargo run -p hardless-cli -- start        # http://localhost:8080 and https://localhost:8443
cargo run -p hardless-cli -- scan scanner-1 4006381333931     # drive devices from the CLI
cargo run -p hardless-cli -- receipts printer-1 --last
cargo run -p hardless-cli -- mcp --url http://127.0.0.1:8080 --embedded   # MCP server on stdio
node scripts/print-sample.mjs             # print fixtures/escpos/33-full-receipt.bin to printer-1
pnpm --filter @hardless/example-react-pwa dev   # example POS app on http://localhost:5173
```

### Docker image

The image packs a static Linux binary (see `Dockerfile`). CI builds it for amd64 and arm64
in `.github/workflows/docker.yml` and checks it serves a working dashboard; it is not pushed
until the first release. To build and run it locally (amd64), with Docker running:

```sh
pnpm --filter @hardless/dashboard build
mkdir -p docker/bin/amd64
docker run --rm -v "$PWD:/src" -v hardless-musl-target:/target -w /src -e CARGO_TARGET_DIR=/target rust:1.99-alpine sh -c "apk add --no-cache musl-dev && cargo build --release --locked -p hardless-cli && cp /target/release/hardless docker/bin/amd64/"
docker build --platform linux/amd64 -t hardless/hardless .
docker run -p 8080:8080 -p 8443:8443 -v hardless-data:/data hardless/hardless
```

On Windows Git Bash, prefix `docker run` with `MSYS_NO_PATHCONV=1`.

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
crates/hardless-core           Device trait, registry, event log, faults, clock, scenarios, bridge and NFC types
crates/hardless-escpos         ESC/POS bytes -> AST (sans-IO); fuzz/ has the cargo-fuzz target
crates/hardless-render         AST -> HTML, SVG, PNG (feature png) and to_text
crates/hardless-devices        printer-escpos (serial, usb, http), cash-drawer, scanner-wedge, nfc-reader
crates/hardless-http-dialects  ePOS-Print / WebPRNT-compatible XML -> ESC/POS
crates/hardless-server         axum: /bridge, /api/v1, /api/v1/stream, HTTP printers, /mcp, dashboard
crates/hardless-mcp            MCP server for agents (rmcp): tools, resources, stdio and HTTP
crates/hardless-wasm           the engine for shim-web standalone mode (wasm-bindgen)
crates/hardless-cli            the `hardless` binary: start, scan, tap, fault, receipts, mcp, licenses
xtask/                         cargo xtask gen-types | build-wasm | licenses | docs
fixtures/escpos/               golden ESC/POS inputs and AST/HTML/text snapshots
packages/types                 generated TS wire types (never edit by hand)
packages/shim-web              navigator.serial, navigator.usb, NDEFReader and keyboard-wedge shims
packages/dashboard             React + Vite + Tailwind UI served by hardless-server
packages/extension             Chrome MV3 extension (per-site enable)
packages/playwright            Playwright fixture + matchers
packages/npm-wrapper           `npx hardless`                                                   [release]
examples/react-pwa             example POS PWA, with the M1, M2 and M3 acceptance tests
scripts/                       licence check, fixture generator, print-sample
docs/                          user docs (quickstart, testing, agents, REST API, plugins), docs/reference (generated)
docs/PRD.md                    the spec; docs/decisions.md records decisions and deviations
llms.txt                       index of the docs for LLMs (generated)
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
