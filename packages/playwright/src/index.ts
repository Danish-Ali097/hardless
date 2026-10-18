/**
 * `@hardless/playwright` provides a Playwright fixture that injects the Hardless shim and
 * matchers such as `toHavePrinted` and `toHaveOpened`.
 *
 * Milestone M0 ships the package skeleton only; the fixture and matchers (FR-40 to FR-42)
 * arrive in milestone M3.
 *
 * @packageDocumentation
 */

/** Names of the custom matchers this package will register (FR-41). */
export const MATCHERS = [
  "toHavePrinted",
  "toHavePrintedJobs",
  "toHaveCut",
  "toHaveOpened",
  "toBeClosed",
  "toHaveWrittenNdef",
] as const;

/** A matcher name. */
export type MatcherName = (typeof MATCHERS)[number];
