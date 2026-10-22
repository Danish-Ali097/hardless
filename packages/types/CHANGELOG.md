# @hardless/types

## 0.1.1

### Patch Changes

- 24cdfc7: Fuller npm pages: the `hardless` README explains what Hardless is, the quick start, every command, testing and MCP setup; every package gets keywords, a homepage and an issues link, and the READMEs drop internal requirement IDs.

## 0.1.0

### Minor Changes

- 3d6e084: Bridge protocol types: `ClientFrame`, `ServerFrame` and `BridgeErrorCode`.
- a1d8cd1: TypeScript types for the ESC/POS command AST (`Node` and its enums).
- e0acb09: Milestone M0: package skeletons, shared TypeScript/ESLint/Prettier/Vitest setup, and wire types generated from the Rust core.
- 3130549: NFC types: `NdefRecord`, `NfcReading`, `NfcEvent` and `NfcRejection`; the bridge `nfc-write`, `nfc-make-read-only`, `nfc-reading` and `nfc-error` messages; the `not_allowed` error code; `nfc` event log records. `ControlCommand` gains `nfc-write` and `nfc-make-read-only`, and a tap's `records` are now typed and optional.
- 703cc61: New `PeripheralSignal` type and `peripheral` event log records; `DeviceSummary` has a new `attachedTo` field.
- 6736537: Scenario runner types (FR-04): `Scenario`, `Step`, `ScenarioReport`, `StepResult` and `StepStatus`, for `POST /api/v1/scenarios/run`.
- 545ce56: Web Serial shim: `install({ mode: "server" | "standalone" })` replaces `navigator.serial` with an emulated port picker, ports, streams, events and errors that follow the Web Serial spec. Adds the `SerialSignals` type and the bridge `signals` message.
- 87b1a40: `DeviceSummary` has a new `supportedFaults` field listing the faults that can be set on the device.
- 962b4f3: New `WedgeScan` type, the bridge `wedge-scan` message and `wedge` event log records for the keyboard-wedge scanner.

### Patch Changes

- 5f4fb86: Each package ships its LICENSE; `@hardless/shim-web` also ships `THIRD_PARTY_LICENSES.txt` for the Rust crates in its WebAssembly engine.
