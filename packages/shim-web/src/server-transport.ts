import type {
  ClientFrame,
  DeviceSummary,
  JsonValue,
  SerialSignals,
  ServerFrame,
} from "@hardless/types";
import {
  fromBase64,
  isConnected,
  toBase64,
  TransportError,
  type Transport,
  type TransportListener,
} from "./transport.js";

/** Options for the server transport. */
export interface ServerTransportOptions {
  /** The bridge URL, e.g. `ws://localhost:8080/bridge`. */
  url: string;
  /** Bearer token, if the server requires one (NFR-02). */
  token?: string;
  /** The `apis` sent in the `hello` message. */
  apis: string[];
  /** The shim version sent in `hello`. */
  shimVersion: string;
}

/** `Omit` that keeps each member of a union separate. */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;

/** A client message without the envelope fields the transport adds. */
type Body = DistributiveOmit<ClientFrame, "v" | "id">;

interface Pending {
  resolve: () => void;
  reject: (error: TransportError) => void;
}

const MIN_BACKOFF_MS = 250;
const MAX_BACKOFF_MS = 5000;

/**
 * The bridge to a Hardless server (FR-15, PRD section 11.2). Reconnects with exponential
 * backoff (250 ms up to 5 s) and reports a dropped bridge as device disconnects.
 */
export class ServerTransport implements Transport {
  readonly ready: Promise<void>;
  private resolveReady!: () => void;
  private socket: WebSocket | null = null;
  private list: DeviceSummary[] = [];
  private nextId = 1;
  private pending = new Map<string, Pending>();
  private backoff = MIN_BACKOFF_MS;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private stopped = false;
  private reachable = new Set<string>();

  constructor(
    private readonly options: ServerTransportOptions,
    private readonly listener: TransportListener,
  ) {
    this.ready = new Promise((resolve) => {
      this.resolveReady = resolve;
    });
    this.connect();
  }

  devices(): DeviceSummary[] {
    return this.list;
  }

  private url(): string {
    const url = new URL(this.options.url, window.location.href);
    if (this.options.token) url.searchParams.set("token", this.options.token);
    return url.href;
  }

  private connect(): void {
    if (this.stopped) return;
    let socket: WebSocket;
    try {
      socket = new WebSocket(this.url());
    } catch {
      this.resolveReady();
      this.scheduleReconnect();
      return;
    }
    this.socket = socket;
    socket.onopen = () => {
      this.backoff = MIN_BACKOFF_MS;
      this.request({
        type: "hello",
        origin: window.location.origin,
        shimVersion: this.options.shimVersion,
        apis: this.options.apis,
      }).then(
        () => {
          this.resolveReady();
        },
        () => {
          this.resolveReady();
        },
      );
    };
    socket.onmessage = (event: MessageEvent<string>) => {
      this.receive(JSON.parse(event.data) as ServerFrame);
    };
    socket.onclose = () => {
      this.socket = null;
      this.resolveReady();
      for (const [, pending] of this.pending) {
        pending.reject(new TransportError("network", "the Hardless bridge disconnected"));
      }
      this.pending.clear();
      // A dropped bridge looks like every device being unplugged (PRD section 11.2).
      for (const id of this.reachable) this.listener.disconnect(id);
      this.reachable.clear();
      this.list = this.list.map((d) => ({
        ...d,
        state: { ...(d.state as object), connected: false },
      }));
      this.listener.devices(this.list);
      this.scheduleReconnect();
    };
  }

  private scheduleReconnect(): void {
    if (this.stopped) return;
    this.timer = setTimeout(() => {
      this.connect();
    }, this.backoff);
    this.backoff = Math.min(this.backoff * 2, MAX_BACKOFF_MS);
  }

  private receive(frame: ServerFrame): void {
    switch (frame.type) {
      case "devices": {
        this.list = frame.devices;
        const now = new Set(frame.devices.filter(isConnected).map((d) => d.id));
        for (const id of now) if (!this.reachable.has(id)) this.listener.connect(id);
        for (const id of this.reachable) if (!now.has(id)) this.listener.disconnect(id);
        this.reachable = now;
        this.listener.devices(this.list);
        break;
      }
      case "ack":
        this.settle(frame.replyTo);
        break;
      case "error":
        if (frame.replyTo)
          this.settle(frame.replyTo, new TransportError(frame.code, frame.message));
        break;
      case "data":
        this.listener.data(frame.deviceId, fromBase64(frame.data));
        break;
      case "connect":
        if (!this.reachable.has(frame.deviceId)) {
          this.reachable.add(frame.deviceId);
          this.listener.connect(frame.deviceId);
        }
        break;
      case "disconnect":
        if (this.reachable.delete(frame.deviceId)) this.listener.disconnect(frame.deviceId);
        break;
    }
  }

  private settle(id: string, error?: TransportError): void {
    const pending = this.pending.get(id);
    if (!pending) return;
    this.pending.delete(id);
    if (error) pending.reject(error);
    else pending.resolve();
  }

  private request(body: Body): Promise<void> {
    const socket = this.socket;
    if (socket?.readyState !== WebSocket.OPEN) {
      return Promise.reject(new TransportError("network", "not connected to the Hardless server"));
    }
    const id = String(this.nextId++);
    return new Promise<void>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      socket.send(JSON.stringify({ v: 1, id, ...body }));
    });
  }

  open(deviceId: string, options: Record<string, unknown>): Promise<void> {
    return this.request({ type: "open", deviceId, options: options as JsonValue });
  }

  close(deviceId: string): Promise<void> {
    return this.request({ type: "close", deviceId });
  }

  write(deviceId: string, data: Uint8Array): Promise<void> {
    return this.request({ type: "write", deviceId, data: toBase64(data) });
  }

  signals(deviceId: string, signals: SerialSignals): Promise<void> {
    return this.request({ type: "signals", deviceId, signals });
  }

  async clearFault(deviceId: string, fault: string): Promise<void> {
    const base = new URL(this.options.url, window.location.href);
    base.protocol = base.protocol === "wss:" ? "https:" : "http:";
    const url = new URL(
      `/api/v1/devices/${encodeURIComponent(deviceId)}/faults/${encodeURIComponent(fault)}`,
      base,
    );
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (this.options.token) headers.Authorization = `Bearer ${this.options.token}`;
    await fetch(url, { method: "PUT", headers, body: JSON.stringify({ active: false }) });
  }

  stop(): void {
    this.stopped = true;
    clearTimeout(this.timer);
    this.socket?.close();
  }
}
