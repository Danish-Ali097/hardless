import type { BusEvent, DeviceSummary, Job } from "@hardless/types";

/**
 * Talks to the Hardless REST API and event stream on the same origin as the dashboard.
 * If the server requires a token (NFR-02), open the dashboard with `?token=...` once; it is
 * kept in sessionStorage for the tab.
 */

const TOKEN_KEY = "hardless:token";

/** The token from `?token=` or sessionStorage, if any. */
export function token(): string | null {
  if (typeof window === "undefined") return null;
  const fromUrl = new URLSearchParams(window.location.search).get("token");
  if (fromUrl) {
    try {
      sessionStorage.setItem(TOKEN_KEY, fromUrl);
    } catch {
      // Storage can be unavailable (private mode); the URL still carries the token.
    }
    return fromUrl;
  }
  try {
    return sessionStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

/** Adds `token` to a URL when one is set, for iframes, links and WebSockets. */
export function withToken(url: string): string {
  const t = token();
  if (!t) return url;
  return `${url}${url.includes("?") ? "&" : "?"}token=${encodeURIComponent(t)}`;
}

/** An error response from the API: `{ error: { code, message } }`. */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = {};
  const t = token();
  if (t) headers.Authorization = `Bearer ${t}`;
  const init: RequestInit = { method, headers };
  if (body !== undefined) {
    headers["Content-Type"] = "application/json";
    init.body = JSON.stringify(body);
  }
  const response = await fetch(`/api/v1${path}`, init);
  if (response.status === 204) return undefined as T;
  const json: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = (json as { error?: { code?: string; message?: string } } | null)?.error;
    throw new ApiError(
      response.status,
      error?.code ?? "http_error",
      error?.message ?? `${method} ${path} failed with ${String(response.status)}`,
    );
  }
  return json as T;
}

const enc = encodeURIComponent;

/** The REST calls the dashboard uses (PRD section 11.1). */
export const api = {
  devices: () => request<DeviceSummary[]>("GET", "/devices"),
  device: (id: string) => request<DeviceSummary>("GET", `/devices/${enc(id)}`),
  createDevice: (kind: string, id: string | undefined, config: Record<string, unknown>) =>
    request<DeviceSummary>("POST", "/devices", id ? { kind, id, config } : { kind, config }),
  removeDevice: (id: string) => request<undefined>("DELETE", `/devices/${enc(id)}`),
  resetDevice: (id: string) => request<DeviceSummary>("POST", `/devices/${enc(id)}/reset`),
  closeDrawer: (id: string) => request<DeviceSummary>("POST", `/devices/${enc(id)}/close`),
  scan: (id: string, code: string) =>
    request<DeviceSummary>("POST", `/devices/${enc(id)}/scan`, { code }),
  jobs: (id: string, limit = 30) =>
    request<Job[]>("GET", `/devices/${enc(id)}/jobs?limit=${String(limit)}`),
  setFault: (id: string, fault: string, active: boolean, options?: Record<string, number>) =>
    request<DeviceSummary>(
      "PUT",
      `/devices/${enc(id)}/faults/${enc(fault)}`,
      options ? { active, options } : { active },
    ),
};

/** URL of a job rendered by the server as an HTML document, an SVG or a PNG (FR-11). */
export function renderUrl(
  deviceId: string,
  jobId: number,
  width: 58 | 80,
  showUnknown: boolean,
  format: "html" | "svg" | "png" = "html",
): string {
  const unknown = showUnknown ? "" : "&unknown=0";
  return withToken(
    `/api/v1/devices/${enc(deviceId)}/jobs/${String(jobId)}/render?format=${format}&width=${String(width)}${unknown}`,
  );
}

/**
 * Subscribes to the event bus (`/api/v1/stream`) and reconnects with backoff
 * (250 ms up to 5 s). Returns a function that stops it.
 */
export function subscribe(handlers: {
  onEvent: (event: BusEvent) => void;
  onOpen: () => void;
  onClose: () => void;
  onLagged: () => void;
}): () => void {
  let socket: WebSocket | null = null;
  let delay = 250;
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const connect = () => {
    const scheme = window.location.protocol === "https:" ? "wss" : "ws";
    socket = new WebSocket(withToken(`${scheme}://${window.location.host}/api/v1/stream`));
    socket.onopen = () => {
      delay = 250;
      handlers.onOpen();
    };
    socket.onmessage = (message: MessageEvent<string>) => {
      const data = JSON.parse(message.data) as BusEvent | { type: "lagged" };
      if (data.type === "lagged") handlers.onLagged();
      else handlers.onEvent(data);
    };
    socket.onclose = () => {
      handlers.onClose();
      if (stopped) return;
      timer = setTimeout(connect, delay);
      delay = Math.min(delay * 2, 5000);
    };
  };
  connect();
  return () => {
    stopped = true;
    clearTimeout(timer);
    socket?.close();
  };
}
