//! Lowercase hex for byte payloads in the JSON AST.

use serde::{de, Deserialize, Deserializer, Serializer};

const DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Encodes bytes as lowercase hex.
pub(crate) fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for &b in data {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    out
}

fn decode(text: &str) -> Result<Vec<u8>, &'static str> {
    fn nibble(c: u8) -> Result<u8, &'static str> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err("invalid hex digit"),
        }
    }
    let (pairs, rest) = text.as_bytes().as_chunks::<2>();
    if !rest.is_empty() {
        return Err("hex string has an odd length");
    }
    pairs
        .iter()
        .map(|&[hi, lo]| Ok((nibble(hi)? << 4) | nibble(lo)?))
        .collect()
}

/// `#[serde(with = "crate::hex::serde_vec")]` for `Vec<u8>` fields.
pub(crate) mod serde_vec {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(data: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&encode(data))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        decode(&text).map_err(de::Error::custom)
    }
}
