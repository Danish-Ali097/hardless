import type { ControlCommand, DeviceSummary, EventRecord, Job, NdefRecord } from "@hardless/types";
import type { Page } from "@playwright/test";

/** Per-scan overrides of a scanner's settings. */
export interface ScanOptions {
  terminator?: "enter" | "tab" | "none";
  delayMs?: number;
}

/**
 * Where the devices live: the page (standalone mode) or a Hardless server. Every device
 * handle and matcher goes through this, so both modes behave the same in tests.
 */
export interface Backend {
  devices(): Promise<DeviceSummary[]>;
  /** Print jobs, newest first. */
  jobs(deviceId: string): Promise<Job[]>;
  /** The event log, oldest first. */
  events(deviceId: string): Promise<EventRecord[]>;
  setFault(
    deviceId: string,
    fault: string,
    active: boolean,
    options?: Record<string, unknown>,
  ): Promise<void>;
  reset(deviceId: string): Promise<void>;
  /** Scans a barcode and resolves once the page has typed it. */
  scan(deviceId: string, code: string, options?: ScanOptions): Promise<void>;
  /** Taps an NFC card and resolves once the page has had the reading. */
  tap(deviceId: string, uid: string, records?: NdefRecord[]): Promise<void>;
  control(deviceId: string, command: ControlCommand): Promise<void>;
}

/** What the page exposes through the shim's `HardlessShim.installation()`. */
interface PageInstallation {
  transport: {
    ready: Promise<void>;
    devices(): DeviceSummary[];
    jobs(id: string): Promise<Job[]>;
    events(id: string): Promise<EventRecord[]>;
    setFault(id: string, fault: string, active: boolean, options?: object): Promise<void>;
    reset(id: string): Promise<void>;
    control(id: string, command: ControlCommand): Promise<void>;
  };
  wedge: { typed: number } | null;
  scan(id: string, code: string, options?: ScanOptions): Promise<void>;
  tap(id: string, uid: string, records?: NdefRecord[]): Promise<void>;
}

declare global {
  // The shim's IIFE build defines this global in the page.
  var HardlessShim: { installation(): PageInstallation | null } | undefined;
}

const NOT_LOADED =
  "The Hardless devices run in the page in standalone mode: navigate to your app (page.goto) first.";

/**
 * Standalone mode: the engine runs in the page as WebAssembly, so every call is a
 * `page.evaluate`, which the page's Content Security Policy does not restrict. Each function
 * below runs in the page and finds the shim itself. The devices start fresh on every
 * navigation.
 */
export class PageBackend implements Backend {
  constructor(private readonly page: Page) {}

  private async run<A, R>(fn: (arg: A) => Promise<R>, arg: A): Promise<R> {
    try {
      // Playwright's own types for evaluate arguments are stricter than these plain objects.
      return await this.page.evaluate(fn as (arg: unknown) => Promise<R>, arg as unknown);
    } catch (error) {
      if (error instanceof Error && error.message.includes("hardless-not-loaded")) {
        throw new Error(NOT_LOADED, { cause: error });
      }
      throw error;
    }
  }

  devices(): Promise<DeviceSummary[]> {
    return this.run(async () => {
      const shim = globalThis.HardlessShim?.installation();
      if (!shim) throw new Error("hardless-not-loaded");
      await shim.transport.ready;
      return shim.transport.devices();
    }, null);
  }

  jobs(deviceId: string): Promise<Job[]> {
    return this.run(async (id) => {
      const shim = globalThis.HardlessShim?.installation();
      if (!shim) throw new Error("hardless-not-loaded");
      return shim.transport.jobs(id);
    }, deviceId);
  }

  events(deviceId: string): Promise<EventRecord[]> {
    return this.run(async (id) => {
      const shim = globalThis.HardlessShim?.installation();
      if (!shim) throw new Error("hardless-not-loaded");
      return shim.transport.events(id);
    }, deviceId);
  }

  setFault(
    deviceId: string,
    fault: string,
    active: boolean,
    options: Record<string, unknown> = {},
  ): Promise<void> {
    return this.run(
      async (a) => {
        const shim = globalThis.HardlessShim?.installation();
        if (!shim) throw new Error("hardless-not-loaded");
        await shim.transport.setFault(a.deviceId, a.fault, a.active, a.options);
      },
      { deviceId, fault, active, options },
    );
  }

  reset(deviceId: string): Promise<void> {
    return this.run(async (id) => {
      const shim = globalThis.HardlessShim?.installation();
      if (!shim) throw new Error("hardless-not-loaded");
      await shim.transport.reset(id);
    }, deviceId);
  }

  scan(deviceId: string, code: string, options: ScanOptions = {}): Promise<void> {
    return this.run(
      async (a) => {
        const shim = globalThis.HardlessShim?.installation();
        if (!shim) throw new Error("hardless-not-loaded");
        await shim.scan(a.deviceId, a.code, a.options);
      },
      { deviceId, code, options },
    );
  }

  tap(deviceId: string, uid: string, records?: NdefRecord[]): Promise<void> {
    return this.run(
      async (a) => {
        const shim = globalThis.HardlessShim?.installation();
        if (!shim) throw new Error("hardless-not-loaded");
        await shim.tap(a.deviceId, a.uid, a.records ?? undefined);
      },
      { deviceId, uid, records: records ?? null },
    );
  }

  control(deviceId: string, command: ControlCommand): Promise<void> {
    return this.run(
      async (a) => {
        const shim = globalThis.HardlessShim?.installation();
        if (!shim) throw new Error("hardless-not-loaded");
        await shim.transport.ready;
        await shim.transport.control(a.deviceId, a.command);
      },
      { deviceId, command },
    );
  }
}

/** The REST base (`http://host:port/api/v1`) of a bridge URL (`ws://host:port/bridge`). */
export function restBase(bridgeUrl: string): string {
  const url = new URL(bridgeUrl);
  url.protocol = url.protocol === "wss:" ? "https:" : "http:";
  url.pathname = "/api/v1";
  url.search = "";
  return url.href;
}

/** Server mode: the devices live in a Hardless server, reached over its REST API. */
export class ServerBackend implements Backend {
  private readonly base: string;

  constructor(
    bridgeUrl: string,
    private readonly token: string | undefined,
    private readonly page: Page,
  ) {
    this.base = restBase(bridgeUrl);
  }

  async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const headers: Record<string, string> = {};
    if (this.token) headers.Authorization = `Bearer ${this.token}`;
    if (body !== undefined) headers["Content-Type"] = "application/json";
    const response = await fetch(`${this.base}${path}`, {
      method,
      headers,
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    const text = await response.text();
    if (!response.ok) {
      throw new Error(`Hardless ${method} ${path} failed with ${String(response.status)}: ${text}`);
    }
    return (text ? JSON.parse(text) : undefined) as T;
  }

  devices(): Promise<DeviceSummary[]> {
    return this.request("GET", "/devices");
  }

  jobs(deviceId: string): Promise<Job[]> {
    return this.request("GET", `/devices/${encodeURIComponent(deviceId)}/jobs?limit=1000`);
  }

  events(deviceId: string): Promise<EventRecord[]> {
    return this.request("GET", `/devices/${encodeURIComponent(deviceId)}/events`);
  }

  async setFault(
    deviceId: string,
    fault: string,
    active: boolean,
    options?: Record<string, unknown>,
  ): Promise<void> {
    await this.request(
      "PUT",
      `/devices/${encodeURIComponent(deviceId)}/faults/${encodeURIComponent(fault)}`,
      options ? { active, options } : { active },
    );
  }

  async reset(deviceId: string): Promise<void> {
    await this.request("POST", `/devices/${encodeURIComponent(deviceId)}/reset`);
  }

  /** Waits until the page's shim has connected to the bridge, so it hears scans and taps. */
  private async connected(): Promise<void> {
    await this.page
      .evaluate(async () => {
        await globalThis.HardlessShim?.installation()?.transport.ready;
      })
      .catch(() => undefined);
  }

  /** How many scans the page's keyboard wedge has typed (0 if the shim is not loaded). */
  private typed(): Promise<number> {
    return this.page
      .evaluate(() => globalThis.HardlessShim?.installation()?.wedge?.typed ?? 0)
      .catch(() => 0);
  }

  async scan(deviceId: string, code: string, options: ScanOptions = {}): Promise<void> {
    await this.connected();
    const before = await this.typed();
    await this.request("POST", `/devices/${encodeURIComponent(deviceId)}/scan`, {
      code,
      ...options,
    });
    // The keys reach the page over the bridge; wait until they have been typed.
    await this.page
      .waitForFunction(
        (n) => (globalThis.HardlessShim?.installation()?.wedge?.typed ?? 0) > n,
        before,
        { timeout: 10_000 },
      )
      .catch(() => undefined);
  }

  async tap(deviceId: string, uid: string, records?: NdefRecord[]): Promise<void> {
    await this.connected();
    await this.request(
      "POST",
      `/devices/${encodeURIComponent(deviceId)}/tap`,
      records === undefined ? { uid } : { uid, records },
    );
  }

  async control(deviceId: string, command: ControlCommand): Promise<void> {
    const id = encodeURIComponent(deviceId);
    switch (command.type) {
      case "scan":
        return this.scan(deviceId, command.code, {
          ...(command.terminator == null ? {} : { terminator: command.terminator }),
          ...(command.delayMs == null ? {} : { delayMs: command.delayMs }),
        });
      case "tap":
        return this.tap(deviceId, command.uid, command.records);
      case "close-drawer":
        await this.request("POST", `/devices/${id}/close`);
        return;
      default:
        throw new Error(`control command ${command.type} is not available in server mode`);
    }
  }
}
