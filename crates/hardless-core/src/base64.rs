//! Standard base64 (RFC 4648, with padding) for byte payloads in bridge messages.

use bytes::Bytes;
use serde::{de, Deserialize, Deserializer, Serializer};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes bytes as padded standard base64.
///
/// ```
/// assert_eq!(hardless_core::base64::encode(b"\x1b@"), "G0A=");
/// ```
pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 0x3f) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes standard base64; padding is optional, whitespace is not allowed.
///
/// ```
/// assert_eq!(hardless_core::base64::decode("G0A=").unwrap(), b"\x1b@");
/// assert!(hardless_core::base64::decode("G0A?").is_err());
/// ```
pub fn decode(text: &str) -> Result<Vec<u8>, &'static str> {
    fn value(c: u8) -> Result<u32, &'static str> {
        match c {
            b'A'..=b'Z' => Ok(u32::from(c - b'A')),
            b'a'..=b'z' => Ok(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Ok(u32::from(c - b'0') + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("invalid base64 character"),
        }
    }
    let trimmed = text.trim_end_matches('=');
    if text.len() - trimmed.len() > 2 {
        return Err("too much base64 padding");
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() % 4 == 1 {
        return Err("invalid base64 length");
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= value(c)? << (18 - 6 * i);
        }
        let [_, b0, b1, b2] = n.to_be_bytes();
        out.push(b0);
        if chunk.len() > 2 {
            out.push(b1);
        }
        if chunk.len() > 3 {
            out.push(b2);
        }
    }
    Ok(out)
}

/// `#[serde(with = "crate::base64::serde_bytes")]` for [`Bytes`] fields.
pub(crate) mod serde_bytes {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(data: &Bytes, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&encode(data))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Bytes, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        decode(&text).map(Bytes::from).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_length() {
        for len in 0..40usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 % 256) as u8).collect();
            assert_eq!(decode(&encode(&data)).unwrap(), data, "length {len}");
        }
        assert_eq!(encode(b"Man"), "TWFu");
        assert_eq!(encode(b"Ma"), "TWE=");
        assert_eq!(encode(b"M"), "TQ==");
        assert_eq!(decode("TQ").unwrap(), b"M");
        assert!(decode("T").is_err());
    }
}
