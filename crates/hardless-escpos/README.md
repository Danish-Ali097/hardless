# hardless-escpos

Streaming ESC/POS parser: turns a byte stream, in chunks of any size, into a command AST. Unknown commands become `unknown` nodes; the parser never panics. Sans-IO.

> Status: skeleton (milestone M0). The parser and golden fixtures land in milestone M1 (FR-06 to FR-09).

Part of [Hardless](https://github.com/Danish-Ali097/hardless), an open-source POS hardware
emulator. Licensed under Apache-2.0.
