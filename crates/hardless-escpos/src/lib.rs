#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod ast;

/// This crate's README, which holds the supported command table (`## Commands`). The MCP
/// server's `hardless://docs/escpos-commands` resource and the docs site read it, so the
/// reference has one source.
pub const REFERENCE: &str = include_str!("../README.md");

mod codepage;
mod hex;
mod parser;

pub use ast::{Align, Font, HriPosition, Node, QrErrorCorrection, StatusKind, Symbology};
pub use codepage::{CodePage, CodePages, TableCodePage, TABLE_PC437, TABLE_PC858};
pub use parser::{parse, Parser, MAX_BARCODE_A_BYTES, MAX_IMAGE_BYTES, MAX_UNKNOWN_BYTES};
