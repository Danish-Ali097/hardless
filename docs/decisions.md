# Decisions and deviations

Choices made while implementing the PRD that the PRD does not spell out, and the places where
the implementation differs from it. Newest milestone first. Items marked **open** need a
decision from the maintainer.

## M0 — Foundation

### Toolchain

- **Rust 1.99 pinned** in `rust-toolchain.toml`; MSRV 1.97 (stable minus 2, NFR-11) in
  `rust-version` and checked by a separate CI job. Pinning keeps local and CI builds
  identical; bump both together.
- **Edition 2021** as the PRD says, although edition 2024 is available.
- **TypeScript 5.9** (`~5.9.3`): the PRD says TypeScript 5.x, so newer majors are not used.
- **pnpm 12.14.0** via the `packageManager` field.

### `hardless-core`

- **`std`, not `no_std + alloc`.** The PRD says "no_std + alloc where possible". The crate
  depends on `serde_json` and `schemars` in its public API (FR-01), and nothing in phase 1
  needs `no_std` (WebAssembly has `std`). Revisit for the phase 5 USB gadget if needed.
- **`bytes` dependency.** FR-01 uses `Bytes` in `DeviceEvent::ToHost`; the type comes from the
  `bytes` crate (MIT, tokio-rs, widely used, and already a dependency of axum). Re-exported as
  `hardless_core::Bytes`.
- **Job AST is JSON.** `Job.ast` is `serde_json::Value` so the core does not depend on any
  command set (ESC/POS now, ZPL/TSPL in phase 2). Devices fill it from their own typed AST.
- **Jobs are stored by the registry**, newest first, capped at 1,000 per device
  (`RegistryConfig::job_capacity`), so `GET /devices/:id/jobs` works the same for every
  printer route. Devices number their own jobs from 1.
- **Event bus is a drained queue.** Being sans-IO, the registry cannot push to subscribers;
  callers call `drain_bus()` after each operation. Undrained events are capped at 10,000.
- **Bus events are exactly the four in FR-02.** Device -> host bytes and connection changes
  are returned to the caller from every registry method instead of adding bus event types.
  Connection changes also publish `device:state`.
- **`set_fault` and `reset` cannot emit events** in the FR-01 `Device` trait (no `now`, no
  return value). A device that needs to (e.g. `offline` disconnects) returns a due
  `next_deadline()`, and the registry runs that tick immediately and returns the events.
  **Open:** should `Device::set_fault` take `now` and return `Vec<DeviceEvent>` instead? It is
  simpler for device authors but changes the FR-01 signature.
- **Faults are open strings.** `FaultId` is a validated kebab-case string so plugins can add
  faults (goal 5); the section 10 IDs are constants and `catalogue()`. The registry does not
  check that a fault applies to a device kind, because `DevicePlugin` has no way to declare
  its faults. **Open:** add `fn faults(&self) -> &[FaultId]` to `DevicePlugin` so the REST API
  can return 400 for a fault that does not apply?
- **Device IDs** are 1–64 ASCII letters, digits, `-`, `_` and `.`, because they appear in URL
  paths. Generated IDs are `<kind>-<n>`.
- **Event log `seq`** is unique across all devices, so `?since=` works for merged views.
- **Event log records carry `deviceId`** and are JSON `{ seq, at, deviceId, type, ... }` with
  byte payloads as lowercase hex (`dataHex`), matching `bytesHex` in the REST job shape.
- **Monotonic time.** If the injected clock goes backwards, the registry keeps the last
  timestamp it used (FR-03 "monotonic timestamp").

### Wire types

- `cargo xtask gen-types` flattens ts-rs output into `packages/types/src`, rewrites imports
  to ESM (`./X.js`) and writes an `index.ts`. `u64` maps to `number` (all timestamps and
  sequence numbers fit in 2^53).
- Zod schemas (PRD section 6) are not generated yet; they are needed when the shim validates
  bridge messages in M1.

### Licences (NFR-14)

- `cargo-deny` enforces the NFR-14 allow list for Rust. Unpublished workspace crates are
  exempt (`licenses.private.ignore`).
- npm: `scripts/check-npm-licenses.mjs` uses `pnpm licenses list --prod` instead of the
  `license-checker` package named in NFR-14 (`license-checker` is unmaintained, and pnpm has
  the feature built in). It checks **production** dependencies only.
- **Open:** dev-only build tools use licences outside the NFR-14 list:
  `lightningcss` (MPL-2.0, required by Vite 8 and Tailwind 4) and `minimatch`
  (BlueOak-1.0.0, required by ESLint). Neither ships in any package or the dashboard bundle.
  Either NFR-14 applies to shipped dependencies only, or Vite/Tailwind need replacing.

### Not in M0

- `cargo-fuzz` targets and the nightly fuzz job arrive with the first parser (M1).
- The device test suite under `wasm-bindgen-test` arrives with the first real device (M1).
- `llms.txt` and its CI sync check are FR-51 (M3).
