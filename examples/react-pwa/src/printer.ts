import { decodeStatus, STATUS, type PrinterStatus } from "./escpos.js";

/**
 * A receipt printer on Web Serial: the production code. With Hardless installed in
 * development, `navigator.serial` is the shim and the printer is emulated; nothing here
 * changes.
 */

interface SerialLike {
  requestPort(options?: object): Promise<PortLike>;
  getPorts(): Promise<PortLike[]>;
}

interface PortLike extends EventTarget {
  readonly readable: ReadableStream<Uint8Array> | null;
  readonly writable: WritableStream<Uint8Array> | null;
  open(options: { baudRate: number }): Promise<void>;
  close(): Promise<void>;
}

function serial(): SerialLike {
  const s = (navigator as unknown as { serial?: SerialLike }).serial;
  if (!s) throw new Error("This browser has no Web Serial. Use Chrome or Edge on a desktop.");
  return s;
}

export class ReceiptPrinter {
  private port: PortLike | null = null;
  private writer: WritableStreamDefaultWriter<Uint8Array> | null = null;
  private incoming: number[] = [];
  private waiting: ((byte: number) => void) | null = null;
  onDisconnect: (() => void) | null = null;

  get connected(): boolean {
    return this.port !== null;
  }

  /** Asks the user to pick the printer (needs a click) and opens it. */
  async connect(): Promise<void> {
    const port = await serial().requestPort();
    await this.open(port);
  }

  /** Reconnects to a printer picked before, without asking. */
  async reconnect(): Promise<boolean> {
    const [port] = await serial().getPorts();
    if (!port) return false;
    await this.open(port);
    return true;
  }

  private async open(port: PortLike): Promise<void> {
    await port.open({ baudRate: 9600 });
    this.port = port;
    this.writer = port.writable?.getWriter() ?? null;
    port.addEventListener("disconnect", () => {
      this.port = null;
      this.writer = null;
      this.onDisconnect?.();
    });
    void this.readLoop(port);
  }

  private async readLoop(port: PortLike): Promise<void> {
    const reader = port.readable?.getReader();
    if (!reader) return;
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        for (const byte of value) {
          if (this.waiting) {
            const resolve = this.waiting;
            this.waiting = null;
            resolve(byte);
          } else {
            this.incoming.push(byte);
          }
        }
      }
    } catch {
      // The printer was unplugged; the disconnect handler deals with it.
    } finally {
      reader.releaseLock();
    }
  }

  async write(bytes: Uint8Array): Promise<void> {
    if (!this.writer) throw new Error("The printer is not connected.");
    await this.writer.write(bytes);
  }

  /** Sends one `DLE EOT` request and waits up to a second for its byte. */
  private async ask(request: Uint8Array): Promise<number> {
    this.incoming = [];
    const reply = new Promise<number>((resolve, reject) => {
      this.waiting = resolve;
      setTimeout(() => {
        if (this.waiting === resolve) {
          this.waiting = null;
          reject(new Error("The printer did not answer a status request."));
        }
      }, 1000);
    });
    await this.write(request);
    return reply;
  }

  async status(): Promise<PrinterStatus> {
    const printer = await this.ask(STATUS.printer);
    const offlineCause = await this.ask(STATUS.offlineCause);
    const paper = await this.ask(STATUS.paper);
    return decodeStatus(printer, offlineCause, paper);
  }
}
