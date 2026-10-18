#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod ast;
mod codepage;
mod hex;
mod parser;

pub use ast::{Align, Font, HriPosition, Node, QrErrorCorrection, StatusKind, Symbology};
pub use codepage::{CodePage, CodePages, TableCodePage, TABLE_PC437, TABLE_PC858};
pub use parser::{parse, Parser, MAX_BARCODE_A_BYTES, MAX_IMAGE_BYTES, MAX_UNKNOWN_BYTES};
