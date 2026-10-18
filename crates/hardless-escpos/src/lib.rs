//! Streaming ESC/POS parser: byte stream to command AST.
//!
//! Sans-IO: no I/O and no clock. Implemented in milestone M1 (FR-06 to FR-09).
#![forbid(unsafe_code)]
