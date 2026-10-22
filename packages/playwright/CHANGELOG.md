# @hardless/playwright

## 0.1.1

### Patch Changes

- 24cdfc7: Fuller npm pages: the `hardless` README explains what Hardless is, the quick start, every command, testing and MCP setup; every package gets keywords, a homepage and an issues link, and the READMEs drop internal requirement IDs.
- Updated dependencies [24cdfc7]
  - @hardless/shim-web@0.1.1
  - @hardless/types@0.1.1

## 0.1.0

### Minor Changes

- e0acb09: Milestone M0: package skeletons, shared TypeScript/ESLint/Prettier/Vitest setup, and wire types generated from the Rust core.
- 62f0a2f: The Playwright fixture (FR-40 to FR-42): `test.use({ hardless: { mode, devices } })` installs the shim with `page.addInitScript` before the app runs, in standalone mode (engine served from `/__hardless__/`) or server mode; `hardless.printer/drawer/scanner/nfc(id)` handles with fault helpers; and the matchers `toHavePrinted`, `toHavePrintedJobs`, `toHaveCut`, `toHaveOpened`, `toBeClosed` and `toHaveWrittenNdef`.

### Patch Changes

- 5f4fb86: Each package ships its LICENSE; `@hardless/shim-web` also ships `THIRD_PARTY_LICENSES.txt` for the Rust crates in its WebAssembly engine.
- Updated dependencies [3d6e084]
- Updated dependencies [a1d8cd1]
- Updated dependencies [e0acb09]
- Updated dependencies [3130549]
- Updated dependencies [5f4fb86]
- Updated dependencies [703cc61]
- Updated dependencies [6736537]
- Updated dependencies [62f0a2f]
- Updated dependencies [183c277]
- Updated dependencies [545ce56]
- Updated dependencies [27dc800]
- Updated dependencies [962b4f3]
- Updated dependencies [87b1a40]
- Updated dependencies [962b4f3]
  - @hardless/types@0.1.0
  - @hardless/shim-web@0.1.0
