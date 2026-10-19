# hardless-http-dialects

Translates **ePOS-Print-compatible** and **WebPRNT-compatible** XML print requests into ESC/POS
(FR-31), and writes their responses (FR-34). The printer device parses the ESC/POS into the same
command AST as a Web Serial print, so the renderer, the dashboard and test assertions work the
same whichever route printed a receipt. Sans-IO: no network code here.

Part of [Hardless](https://github.com/hardlessdev/hardless), an open-source POS hardware
emulator. Hardless is not affiliated with Seiko Epson or Star Micronics; both dialects are
implemented from their public manuals, and no vendor SDK code is used (NFR-15, NFR-16).

## Usage

```rust
use hardless_http_dialects::{epos, Outcome, PrinterStatus};

let request = epos::parse(r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body>
  <epos-print xmlns="http://www.epson-pos.com/schemas/2011/03/epos-print">
    <text em="true">Total: 1,250&#10;</text><cut type="feed"/>
  </epos-print></s:Body></s:Envelope>"#).unwrap();

// Feed `request.print.escpos` to an ESC/POS printer, then read its status back with
// DLE EOT 1, 2 and 4 and report it.
let status = PrinterStatus::from_dle_eot(0x12, 0x12, 0x12);
let outcome = Outcome::after_print(&status);
let xml = epos::response(&outcome, &status, request.print_job_id.as_deref());
assert!(xml.contains(r#"success="true""#));
```

## ePOS-Print-compatible (`epos`)

Request: a SOAP 1.1 envelope whose body holds an `epos-print` document (a bare `epos-print`
document is accepted too). `devid`, `timeout` and `printjobid` may come in the SOAP header.

| Element                                                             | Becomes                                               |
| ------------------------------------------------------------------- | ----------------------------------------------------- |
| `text` (`align`, `font`, `em`, `ul`, `dw`, `dh`, `width`, `height`) | text and `ESC a`, `ESC M`, `ESC E`, `ESC -`, `GS !`   |
| `feed` (`line`, `unit`)                                             | `ESC d` (units in 30-dot lines)                       |
| `cut` (`no_feed`, `feed`, `reserve`)                                | `GS V` partial cut                                    |
| `pulse` (`drawer_1`/`drawer_2`, `pulse_100`..`pulse_500`)           | `ESC p` on pin 2 or 5                                 |
| `barcode` (all types)                                               | `GS H`, `GS w`, `GS h`, `GS k`                        |
| `symbol` (`qrcode_model_1/2`)                                       | `GS ( k` QR Code; other symbol types print as unknown |
| `image` (`mono`, `gray16`)                                          | `GS v 0` raster                                       |
| `command`                                                           | the hexadecimal bytes as they are                     |
| `logo`                                                              | `GS ( L` (unknown: the emulator stores no logos)      |
| `sound`, `page`, `layout`, ...                                      | a warning; `page` content prints in standard mode     |

Response: `<response success code status battery>` with the manual's error codes
(`EPTR_REC_EMPTY`, `EPTR_COVER_OPEN`, `EX_TIMEOUT`, `SchemaError`, `DeviceNotFound`) and
status bits (print complete, drawer pin 3, offline, cover open, near end, paper end).

## WebPRNT-compatible (`webprnt`)

Request: a `<root>` document. `text` (`emphasis`, `underline`, `width`, `height`, `font`;
control characters as `\xNN`), `alignment`, `feed`, `cutpaper`, `barcode`, `qrcode`, `pdf417`,
`bitimage`, `peripheral`, `rawdata`, `initialization` and `logo` translate like the ePOS
elements; `sound`, `ruledline` and `holdprint` give warnings.

Response: `<root><success/><code/><status/></root>`, where `status` is the Star Automatic
Status as hex: header bytes, then status 1 (drawer switch, offline, cover open) and status 4
(paper near end, paper end), as in the public ASB manual.

## Safety

Documents over 8 MB and documents with a DTD are refused, so entity expansion and external
entities cannot reach the emulator. Both parsers are fuzzed nightly (`fuzz/`) and never panic.

## License

Apache-2.0
