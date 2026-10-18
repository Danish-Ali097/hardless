/**
 * A tiny ESC/POS receipt builder, the kind of code a POS app has. It knows nothing about
 * Hardless: it produces the same bytes a real receipt printer expects.
 */

const ESC = 0x1b;
const GS = 0x1d;
const DLE = 0x10;
const LF = 0x0a;

export interface Line {
  name: string;
  quantity: number;
  /** Price per item in the smallest currency unit. */
  price: number;
}

export interface Sale {
  lines: Line[];
  payment: "cash" | "card";
}

/** Formats an amount the way this café prints it: 1250 -> "1,250". */
export function money(amount: number): string {
  return amount.toLocaleString("en-US");
}

export function total(lines: Line[]): number {
  return lines.reduce((sum, line) => sum + line.quantity * line.price, 0);
}

/** Pads `left` and `right` to fill a 32-column (58 mm) or 48-column (80 mm) line. */
function columns(left: string, right: string, width: number): string {
  const space = Math.max(1, width - left.length - right.length);
  return left + " ".repeat(space) + right;
}

class Bytes {
  private readonly parts: number[] = [];
  push(...bytes: number[]): this {
    this.parts.push(...bytes);
    return this;
  }
  text(value: string): this {
    // The café's receipts are ASCII; real apps would encode for the printer's code page.
    for (const ch of value) this.parts.push(ch.charCodeAt(0) & 0x7f);
    return this;
  }
  line(value = ""): this {
    return this.text(value).push(LF);
  }
  toArray(): Uint8Array {
    return Uint8Array.from(this.parts);
  }
}

/** The receipt for a sale. Cash sales kick the drawer (pin 2, 50 ms) before the cut. */
export function receipt(sale: Sale, width = 32): Uint8Array {
  const b = new Bytes()
    .push(ESC, 0x40) // initialize
    .push(ESC, 0x61, 1) // center
    .push(GS, 0x21, 0x11) // double width and height
    .line("CORNER CAFE")
    .push(GS, 0x21, 0x00)
    .line("12 Market Street")
    .line()
    .push(ESC, 0x61, 0); // left
  for (const line of sale.lines) {
    b.line(
      columns(`${String(line.quantity)} x ${line.name}`, money(line.quantity * line.price), width),
    );
  }
  b.line("-".repeat(width))
    .push(ESC, 0x45, 1) // bold
    .line(columns("Total:", money(total(sale.lines)), width))
    .push(ESC, 0x45, 0)
    .line(`Paid by ${sale.payment}`)
    .line()
    .push(ESC, 0x61, 1)
    .line("Thank you!")
    .push(ESC, 0x64, 3); // feed 3 lines
  if (sale.payment === "cash") b.push(ESC, 0x70, 0, 25, 250); // drawer kick
  return b.push(GS, 0x56, 0x42, 0).toArray(); // feed and partial cut
}

/** `ESC p 0 25 250`: open the cash drawer on pin 2. */
export const DRAWER_KICK = Uint8Array.from([ESC, 0x70, 0, 25, 250]);

/** `DLE EOT n`: real-time status requests. */
export const STATUS = {
  printer: Uint8Array.from([DLE, 0x04, 1]),
  offlineCause: Uint8Array.from([DLE, 0x04, 2]),
  paper: Uint8Array.from([DLE, 0x04, 4]),
};

export interface PrinterStatus {
  online: boolean;
  drawerOpen: boolean;
  coverOpen: boolean;
  paperOut: boolean;
}

/** Decodes the three status bytes, per the ESC/POS command reference. */
export function decodeStatus(printer: number, offlineCause: number, paper: number): PrinterStatus {
  return {
    online: (printer & 0x08) === 0,
    drawerOpen: (printer & 0x04) !== 0,
    coverOpen: (offlineCause & 0x04) !== 0,
    paperOut: (paper & 0x60) === 0x60 || (offlineCause & 0x20) !== 0,
  };
}
