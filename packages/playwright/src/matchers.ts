import type { Job, NdefRecord } from "@hardless/types";
import type { DeviceHandle } from "./devices.js";
import { DrawerHandle, NfcHandle, PrinterHandle } from "./devices.js";

/** Names of the custom matchers (FR-41). */
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

/** Playwright's default `expect` timeout. */
const DEFAULT_TIMEOUT = 5_000;
const POLL_MS = 100;

/** What Playwright passes to a matcher as `this`. */
export interface MatcherContext {
  isNot: boolean;
  timeout?: number;
}

/** A matcher's answer. */
export interface MatcherResult {
  pass: boolean;
  message: () => string;
  name: string;
}

/** One look at the device: whether the matcher passes now, and what was seen. */
interface Check {
  pass: boolean;
  saw: string;
}

/**
 * Polls `check` until it gives the wanted answer (pass, or fail for `.not`) or the timeout
 * passes, like Playwright's web-first assertions (FR-41).
 */
async function poll(
  context: MatcherContext,
  name: string,
  expected: string,
  check: () => Promise<Check>,
): Promise<MatcherResult> {
  const timeout = context.timeout ?? DEFAULT_TIMEOUT;
  const deadline = Date.now() + timeout;
  let last: Check = { pass: false, saw: "nothing yet" };
  let error: unknown = null;
  for (;;) {
    try {
      last = await check();
      error = null;
      if (last.pass !== context.isNot) break;
    } catch (e) {
      // The page may be navigating; try again.
      error = e;
    }
    if (Date.now() >= deadline) break;
    await new Promise((resolve) => setTimeout(resolve, POLL_MS));
  }
  const not = context.isNot ? "not " : "";
  return {
    pass: last.pass,
    name,
    message: () =>
      `${name}: expected ${not}${expected}, after ${String(timeout)} ms saw ${last.saw}` +
      (error instanceof Error ? `\nLast error: ${error.message}` : ""),
  };
}

function need<T extends DeviceHandle>(
  received: unknown,
  type: new (...args: never[]) => T,
  name: string,
  handle: "printer" | "drawer" | "nfc",
): T {
  if (!(received instanceof type)) {
    throw new Error(`${name} needs a device handle, e.g. expect(hardless.${handle}("id"))`);
  }
  return received;
}

function texts(jobs: Job[]): string {
  if (jobs.length === 0) return "no receipts";
  return jobs
    .slice(0, 5)
    .map((j) => JSON.stringify(j.text))
    .join(", ");
}

function hasCut(job: Job | undefined): boolean {
  const ast = job?.ast;
  return Array.isArray(ast) && ast.some((n) => (n as { type?: unknown } | null)?.type === "cut");
}

/** Records equal for the matcher: the same fields, ignoring ones left out. */
function sameRecords(actual: NdefRecord[], expected: Partial<NdefRecord>[]): boolean {
  if (actual.length !== expected.length) return false;
  return expected.every((want, i) => {
    const got = actual[i] as Record<string, unknown> | undefined;
    if (!got) return false;
    return Object.entries(want).every(([key, value]) => got[key] === value);
  });
}

/** The matchers, for `expect.extend` (FR-41). Every matcher polls until it passes. */
export const matchers = {
  /** A receipt's text contains `text`, or matches the regular expression. */
  async toHavePrinted(this: MatcherContext, received: unknown, text: string | RegExp) {
    const printer = need(received, PrinterHandle, "toHavePrinted", "printer");
    const describe = typeof text === "string" ? JSON.stringify(text) : String(text);
    return poll(this, "toHavePrinted", `a receipt with ${describe}`, async () => {
      const jobs = await printer.jobs();
      const pass = jobs.some((j) =>
        typeof text === "string" ? j.text.includes(text) : text.test(j.text),
      );
      return { pass, saw: texts(jobs) };
    });
  },

  /** The printer has printed exactly `count` receipts. */
  async toHavePrintedJobs(this: MatcherContext, received: unknown, count: number) {
    const printer = need(received, PrinterHandle, "toHavePrintedJobs", "printer");
    return poll(this, "toHavePrintedJobs", `${String(count)} receipts`, async () => {
      const jobs = await printer.jobs();
      return { pass: jobs.length === count, saw: `${String(jobs.length)} receipts` };
    });
  },

  /** The latest receipt ends with (or contains) a paper cut. */
  async toHaveCut(this: MatcherContext, received: unknown) {
    const printer = need(received, PrinterHandle, "toHaveCut", "printer");
    return poll(this, "toHaveCut", "the latest receipt to be cut", async () => {
      const job = await printer.lastJob();
      return {
        pass: hasCut(job),
        saw: job
          ? `receipt ${String(job.id)} ${hasCut(job) ? "with" : "without"} a cut`
          : "no receipts",
      };
    });
  },

  /** The drawer has opened, `times` times if given (at least once otherwise). */
  async toHaveOpened(this: MatcherContext, received: unknown, options: { times?: number } = {}) {
    const drawer = need(received, DrawerHandle, "toHaveOpened", "drawer");
    const expected =
      options.times === undefined
        ? "to have opened"
        : `to have opened ${String(options.times)} times`;
    return poll(this, "toHaveOpened", expected, async () => {
      const state = await drawer.state();
      const opened = typeof state.opened === "number" ? state.opened : 0;
      return {
        pass: options.times === undefined ? opened > 0 : opened === options.times,
        saw: `${String(opened)} openings`,
      };
    });
  },

  /** The drawer is closed. */
  async toBeClosed(this: MatcherContext, received: unknown) {
    const drawer = need(received, DrawerHandle, "toBeClosed", "drawer");
    return poll(this, "toBeClosed", "the drawer to be closed", async () => {
      const state = await drawer.state();
      return { pass: state.state === "closed", saw: `it ${String(state.state)}` };
    });
  },

  /**
   * The app's latest NFC write holds these records: each expected record's fields must match
   * (fields left out are not compared).
   */
  async toHaveWrittenNdef(this: MatcherContext, received: unknown, records: Partial<NdefRecord>[]) {
    const nfc = need(received, NfcHandle, "toHaveWrittenNdef", "nfc");
    return poll(this, "toHaveWrittenNdef", `a write of ${JSON.stringify(records)}`, async () => {
      const written = await nfc.lastWritten();
      return {
        pass: written !== null && sameRecords(written.records, records),
        saw: written ? JSON.stringify(written.records) : "no writes",
      };
    });
  },
};
