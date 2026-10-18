// Writes the golden ESC/POS inputs in fixtures/escpos/*.bin from readable definitions.
// Run `node scripts/gen-escpos-fixtures.mjs` after changing a definition, then review the
// snapshot changes (`INSTA_UPDATE=always cargo test -p hardless-escpos`).
//
// Byte sequences follow the publicly available ESC/POS command reference.
import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";

const ESC = 0x1b;
const GS = 0x1d;
const DLE = 0x10;
const LF = 0x0a;

/** Concatenates numbers, strings (as Latin-1 bytes) and arrays into one byte array. */
function bytes(...parts) {
  const out = [];
  for (const part of parts) {
    if (typeof part === "number") out.push(part);
    else if (typeof part === "string") for (const ch of part) out.push(ch.charCodeAt(0) & 0xff);
    else out.push(...bytes(...part));
  }
  return out;
}

/** `GS ( k pL pH cn fn ...params`. */
function gsParenK(cn, fn, params = []) {
  const body = bytes(cn, fn, params);
  return bytes(GS, "(k", body.length & 0xff, body.length >> 8, body);
}

const qrStore = (data) => gsParenK(49, 80, bytes(48, data));

const fixtures = {
  "01-init-text": bytes(ESC, "@", "Hello, receipt", LF),
  "02-align": bytes(
    ESC,
    "a",
    0,
    "left",
    LF,
    ESC,
    "a",
    1,
    "center",
    LF,
    ESC,
    "a",
    2,
    "right",
    LF,
    ESC,
    "a",
    "1",
    "ascii center",
    LF,
  ),
  "03-bold": bytes(ESC, "E", 1, "Bold", ESC, "E", 0, " plain", LF, ESC, "E", 0xff, "odd n", LF),
  "04-underline": bytes(ESC, "-", 1, "thin", ESC, "-", 2, " thick", ESC, "-", 0, " off", LF),
  "05-size": bytes(
    GS,
    "!",
    0x11,
    "2x2",
    GS,
    "!",
    0x00,
    LF,
    GS,
    "!",
    0x70,
    "8x1",
    GS,
    "!",
    0x07,
    LF,
  ),
  "06-feed": bytes("a", ESC, "d", 3, "b", LF),
  "07-codepage-cp437": bytes(
    ESC,
    "t",
    0,
    [...Array(32).keys()].map((i) => 0x80 + i),
    LF,
  ),
  "08-codepage-cp858-euro": bytes(ESC, "t", 19, "Total ", 0xd5, " 12,50", LF),
  "09-codepage-unknown": bytes(ESC, "t", 17, 0x80, 0x81, LF),
  "10-cut-full": bytes("bye", LF, GS, "V", 0),
  "11-cut-partial-ascii": bytes(GS, "V", "1"),
  "12-cut-with-feed": bytes(GS, "V", 65, 3, GS, "V", 66, 5, GS, "V", 97, 0),
  "13-drawer-kick-pin2": bytes(ESC, "p", 0, 25, 250),
  "14-drawer-kick-pin5-ascii": bytes(ESC, "p", "1", 50, 50),
  "15-qr-full-sequence": bytes(
    ESC,
    "a",
    1,
    gsParenK(49, 65, [50, 0]),
    gsParenK(49, 67, [6]),
    gsParenK(49, 69, [49]),
    qrStore("https://example.com/r/1250"),
    gsParenK(49, 81, [48]),
    LF,
  ),
  "16-qr-binary-data": bytes(qrStore(bytes(0x00, 0xff, 0x10, 0x04, 0x01)), gsParenK(49, 81, [48])),
  "17-barcode-code128": bytes(GS, "h", 80, GS, "w", 2, GS, "H", 2, GS, "k", 73, 9, "{BABC-123", LF),
  "18-barcode-ean13-function-a": bytes(GS, "k", 2, "400638133393", 0),
  "19-barcode-ean13-function-b": bytes(GS, "k", 67, 13, "4006381333931"),
  "20-barcode-code39": bytes(GS, "k", 4, "HARDLESS", 0, GS, "k", 69, 4, "M1-2"),
  "21-barcode-unsupported-upca": bytes(GS, "k", 65, 11, "01234567890", "after", LF),
  "22-raster-small": bytes(
    GS,
    "v0",
    0,
    2,
    0,
    8,
    0,
    [...Array(16).keys()].map((i) => (i % 2 ? 0x0f : 0xf0)),
  ),
  "23-raster-double": bytes(GS, "v0", 3, 1, 0, 2, 0, 0xaa, 0x55),
  "24-status-requests": bytes(DLE, 0x04, 1, DLE, 0x04, 2, DLE, 0x04, 3, DLE, 0x04, 4),
  "25-unknown-known-length-esc3": bytes(ESC, "3", 24, "spaced", LF),
  "26-unknown-gs-paren-l": bytes(GS, "(L", 6, 0, 48, 69, 32, 32, 1, 1, "after", LF),
  "27-truly-unknown-command": bytes(ESC, 0xf0, "after", LF),
  "28-control-chars": bytes("a", 0x0d, LF, "b", 0x09, "c", 0x07, LF),
  "29-print-mode": bytes(ESC, "!", 0x38, "BIG", ESC, "!", 0x00, LF, ESC, "!", 0x81, "small u", LF),
  "30-font": bytes(ESC, "M", 1, "font B", ESC, "M", "0", " font A", LF),
  "31-legacy-cuts": bytes(ESC, "i", ESC, "m"),
  "32-realtime-pulse": bytes(DLE, 0x14, 1, 0, 3),
  "33-full-receipt": bytes(
    ESC,
    "@",
    ESC,
    "a",
    1,
    GS,
    "!",
    0x11,
    ESC,
    "E",
    1,
    "HARDLESS CAFE",
    LF,
    GS,
    "!",
    0,
    ESC,
    "E",
    0,
    "1 Test Street",
    LF,
    LF,
    ESC,
    "a",
    0,
    "2 x Flat white          900",
    LF,
    "1 x Croissant           350",
    LF,
    "--------------------------------",
    LF,
    ESC,
    "E",
    1,
    "Total:                 1,250",
    ESC,
    "E",
    0,
    LF,
    LF,
    ESC,
    "a",
    1,
    GS,
    "h",
    60,
    GS,
    "H",
    2,
    GS,
    "k",
    73,
    8,
    "{C",
    0x0c,
    0x22,
    0x38,
    0x4e,
    0x5a,
    0x0a,
    LF,
    gsParenK(49, 65, [50, 0]),
    gsParenK(49, 67, [4]),
    qrStore("receipt:1250"),
    gsParenK(49, 81, [48]),
    LF,
    "Thank you!",
    LF,
    ESC,
    "d",
    3,
    ESC,
    "p",
    0,
    25,
    250,
    GS,
    "V",
    66,
    0,
  ),
  "34-truncated-command-at-end": bytes("ok", LF, GS, "(k", 4, 0, 49),
  "35-unknown-gs-paren-k-pdf417": bytes(gsParenK(48, 80, bytes(48, "pdf")), "after", LF),
  "36-raster-declared-too-large": bytes(GS, "v0", 0, 0xff, 0xff, 0xff, 0xff, 1, 2, 3),
  "37-high-bytes-text-cp437": bytes("Caf", 0x82, " ", 0x9c, "5", LF),
  "38-unknown-bit-image-esc-star": bytes(ESC, "*", 33, 2, 0, [1, 2, 3, 4, 5, 6], "after", LF),
  "39-unknown-tab-positions-esc-d": bytes(ESC, "D", 8, 16, 24, 0, "after", LF),
  "40-init-resets-code-page": bytes(ESC, "t", 19, 0xd5, ESC, "@", 0xd5, LF),
};

const dir = new URL("../fixtures/escpos/", import.meta.url);
mkdirSync(dir, { recursive: true });
for (const file of readdirSync(dir)) if (file.endsWith(".bin")) rmSync(new URL(file, dir));
for (const [name, data] of Object.entries(fixtures)) {
  writeFileSync(new URL(`${name}.bin`, dir), Uint8Array.from(data));
}
console.log(`wrote ${Object.keys(fixtures).length} fixtures to fixtures/escpos`);
