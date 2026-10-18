# Contributing to Hardless

Thanks for helping. This guide covers how to get set up, how changes are reviewed, and the
rules every change follows.

## Setup

See [AGENTS.md](AGENTS.md) for prerequisites and the exact commands. In short:

```sh
cargo test --workspace
pnpm install && pnpm -r test
```

## Making a change

1. Open an issue first for anything larger than a small fix, so we can agree on the approach.
2. Find the requirement in [docs/PRD.md](docs/PRD.md) that your change implements (FR-xx or
   NFR-xx). If there is none and the change affects behaviour, say so in the issue.
3. Write the public types and a README usage example first, then the tests, then the
   implementation.
4. Name tests after the requirement: `fn fr_08_unknown_command_yields_unknown_node()` in Rust,
   `it("FR-08: ...")` in TypeScript.
5. Run everything in AGENTS.md locally. CI runs the same checks on Linux, Windows and macOS.
6. Add release notes: Conventional Commit messages for Rust crates, and `pnpm changeset` for
   npm packages.
7. Open a pull request and fill in the template.

## Rules

- **Engine crates are sans-IO**: no I/O, no clock, and they must build for
  `wasm32-unknown-unknown`.
- **No panics on input** and no `unsafe` (except in future `hardless-transport-*` crates, with
  a `// SAFETY:` comment on every block).
- **Wire types are defined in Rust** and generated into `packages/types` with
  `cargo xtask gen-types`. Never edit generated files.
- **Never copy code from vendor SDKs.** Implement protocols from public command references and
  cite the reference in a comment.
- **No vendor trademarks** in package names, device kinds or UI labels.
- **Licences (NFR-14).** Dependencies that ship must be MIT, Apache-2.0, BSD, ISC, Zlib,
  Unicode-3.0 or BlueOak-1.0.0. Build-only tools may also be MPL-2.0. GPL-family licences are
  never allowed. The policy is `license-policy.json`; check with `cargo deny check` and
  `pnpm run check:licenses`. Justify any new dependency in the PR.
- Public APIs, message formats, endpoint paths and fault IDs come from the PRD. Changing one
  needs a maintainer's agreement first.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/), citing requirement IDs:

```
feat(escpos): parse GS ( k QR codes (FR-07)
fix(core): keep event log timestamps monotonic (FR-03)
```

## Adding a device

Devices implement `hardless_core::DevicePlugin` and `hardless_core::Device`. See the example in
[crates/hardless-core/README.md](crates/hardless-core/README.md). A device is a pure state
machine: it receives bytes, control commands and `now`, and returns `DeviceEvent`s.

## License

By contributing, you agree that your contributions are licensed under the
[Apache-2.0 License](LICENSE).
