//! Bytes that cross between a plugin and the app: HTTP bodies, a file to save.

use std::ops::Deref;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use embedded_gpui::serde::{Deserialize, Deserializer, Serialize, Serializer};
use embedded_gpui::{Describe, TypeSchema};

/// Bytes, crossing as base64 text: payloads are JSON, where they would otherwise be a
/// list of numbers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bytes(pub Vec<u8>);

impl Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(bytes: Vec<u8>) -> Self {
        Bytes(bytes)
    }
}

impl From<&[u8]> for Bytes {
    fn from(bytes: &[u8]) -> Self {
        Bytes(bytes.to_vec())
    }
}

impl Serialize for Bytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD.decode(text).map(Bytes).map_err(embedded_gpui::serde::de::Error::custom)
    }
}

impl Describe for Bytes {
    fn describe() -> TypeSchema {
        TypeSchema::String
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_cross_as_base64_text() {
        let bytes = Bytes(b"hi\x00\xff".to_vec());
        let payload = embedded_gpui::encode(&bytes).unwrap();
        assert_eq!(payload.bytes, br#""aGkA/w==""#, "a third larger than the bytes, not a list of numbers");
        assert_eq!(embedded_gpui::decode::<Bytes>(&payload).unwrap(), bytes);
    }
}
