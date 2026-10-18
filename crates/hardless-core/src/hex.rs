//! Lowercase hex encoding for byte payloads on the wire (`bytesHex`, `dataHex`).

use bytes::Bytes;
use serde::{de, Deserialize, Deserializer, Serializer};

const DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Encodes bytes as lowercase hex, two digits per byte.
pub(crate) fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 2);
    for &b in data {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    out
}

/// Decodes hex (either case). Whitespace is not allowed.
pub(crate) fn decode(text: &str) -> Result<Vec<u8>, &'static str> {
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

/// `#[serde(with = "crate::hex::serde_bytes")]` for [`Bytes`] fields.
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
    fn round_trips() {
        let data = [0x00, 0x1b, 0x40, 0xff];
        assert_eq!(encode(&data), "001b40ff");
        assert_eq!(decode("001B40ff").unwrap(), data);
        assert!(decode("abc").is_err());
        assert!(decode("zz").is_err());
    }
}
