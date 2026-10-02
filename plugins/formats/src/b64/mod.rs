//! Base64: encode text, or decode it back.
//!
//! * this file: which inputs are Base64, and the encoding and decoding.
//! * `view`: the tool: the Encode and Decode tabs, the result and its actions.

mod view;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use delight_ui::code::Language;
use delight_ui::conversion::Conversion;

pub use view::Base64View;

/// Shorter text is never taken for Base64: too many words are.
const MIN_LENGTH: usize = 8;

/// Encode or decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Encode,
    Decode,
}

/// What decoding found.
#[derive(Debug, Clone, PartialEq)]
pub enum Decoded {
    Text(String),
    /// Bytes that aren't UTF-8 text: their count.
    Binary(usize),
}

/// `text` decoded, in either alphabet (standard or URL-safe), padded or not; line breaks
/// are ignored, as encoders wrap long output. `None`: it isn't Base64.
pub fn decode(text: &str) -> Option<Decoded> {
    let compact: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if compact.is_empty() {
        return None;
    }
    let bytes = [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD].iter().find_map(|engine| engine.decode(&compact).ok())?;
    Some(match String::from_utf8(bytes) {
        Ok(text) if is_readable(&text) => Decoded::Text(text),
        Ok(text) => Decoded::Binary(text.len()),
        Err(error) => Decoded::Binary(error.into_bytes().len()),
    })
}

/// No control characters but line breaks and tabs: text, not data.
fn is_readable(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
}

/// `text` as standard Base64, padded.
pub fn encode(text: &str) -> String {
    STANDARD.encode(text)
}

/// `text` as URL-safe Base64, unpadded (as in URLs and JWTs).
pub fn encode_url_safe(text: &str) -> String {
    URL_SAFE_NO_PAD.encode(text)
}

/// A conversion, and what to copy.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub conversion: Conversion,
    /// The result's size, beside the tabs.
    pub info: Option<String>,
    /// The result as it is (↵).
    pub copy: Option<String>,
    /// Encoded: URL-safe; decoded: the JSON in it, formatted (⌘↵).
    pub other: Option<String>,
}

/// `text` encoded or decoded. Decoded JSON is shown formatted, but copied as it was.
pub fn convert(text: &str, mode: Mode) -> Converted {
    let mut converted = Converted { conversion: Conversion::Empty, info: None, copy: None, other: None };
    if text.is_empty() {
        return converted;
    }
    match mode {
        Mode::Encode => {
            let encoded = encode(text);
            converted.info = Some(format!("{} bytes → {} characters", text.len(), encoded.len()));
            converted.conversion = Conversion::plain(encoded.clone());
            converted.copy = Some(encoded);
            converted.other = Some(encode_url_safe(text));
        }
        Mode::Decode => match decode(text) {
            Some(Decoded::Text(decoded)) => {
                converted.info = Some(format!("UTF-8 · {} bytes", decoded.len()));
                let json = serde_json::from_str::<serde_json::Value>(&decoded)
                    .ok()
                    .filter(|value| value.is_object() || value.is_array())
                    .map(|value| serde_json::to_string_pretty(&value).expect("serializing a Value can't fail"));
                converted.conversion = match &json {
                    Some(pretty) => Conversion::text(Language::Json, pretty.clone()),
                    None => Conversion::plain(decoded.clone()),
                };
                converted.copy = Some(decoded);
                converted.other = json;
            }
            Some(Decoded::Binary(bytes)) => {
                converted.conversion = Conversion::failed(format!("Not text: {bytes} bytes of binary data"), None)
            }
            None => {
                converted.conversion =
                    Conversion::failed("Not Base64: it has characters other than A–Z, a–z, 0–9, + / (or - _) and = at the end", None)
            }
        },
    }
    converted
}

/// Which tab fits `text`: Decode for Base64 that decodes to text (or, with a sign it's
/// Base64 rather than a word, to data); Encode for anything else.
pub fn mode(text: &str) -> Mode {
    if confidence(text) >= 0.45 { Mode::Decode } else { Mode::Encode }
}

/// How surely the tool fits: Base64 to decode is recommended; any other text can be
/// encoded, so the tool is always offered, low.
pub fn confidence(text: &str) -> f32 {
    let encode = 0.2;
    // Most inputs have a character Base64 doesn't: stop there, before copying them.
    let alphabet = |b: u8| b.is_ascii_alphanumeric() || b.is_ascii_whitespace() || matches!(b, b'+' | b'/' | b'-' | b'_' | b'=');
    if !text.bytes().all(alphabet) {
        return encode;
    }
    let compact: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if compact.len() < MIN_LENGTH {
        return encode;
    }
    match decode(text) {
        Some(Decoded::Text(_)) => 0.85,
        // Data is taken for Base64 only with a sign it's no word: padding, `+` or `/`, or
        // a long run of mixed case and digits.
        Some(Decoded::Binary(_)) => {
            let has = |f: fn(&char) -> bool| compact.chars().any(|c| f(&c));
            let signs = compact.ends_with('=')
                || has(|c| matches!(c, '+' | '/'))
                || (compact.len() >= 24 && has(char::is_ascii_digit) && has(char::is_ascii_uppercase) && has(char::is_ascii_lowercase));
            if signs { 0.45 } else { encode }
        }
        None => encode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_either_alphabet_padded_or_not() {
        assert_eq!(decode("aGVsbG8gd29ybGQ="), Some(Decoded::Text("hello world".into())));
        assert_eq!(decode("aGVsbG8gd29ybGQ"), Some(Decoded::Text("hello world".into())));
        assert_eq!(decode("aGVsbG8g\nd29ybGQ="), Some(Decoded::Text("hello world".into())));
        assert_eq!(decode("-_-_"), Some(Decoded::Binary(3)));
        assert_eq!(decode("not base64!"), None);
        assert_eq!(encode("hello world"), "aGVsbG8gd29ybGQ=");
        assert_eq!(encode_url_safe("??>"), "Pz8-");
    }

    #[test]
    fn converts_and_says_what_to_copy() {
        let encoded = convert("hello world", Mode::Encode);
        assert_eq!(encoded.copy.as_deref(), Some("aGVsbG8gd29ybGQ="));
        assert_eq!(encoded.other.as_deref(), Some("aGVsbG8gd29ybGQ"));
        assert_eq!(encoded.info.as_deref(), Some("11 bytes → 16 characters"));
        // JSON inside is formatted to show, and copied as it was.
        let decoded = convert(&encode(r#"{"a":1}"#), Mode::Decode);
        assert_eq!(decoded.conversion, Conversion::text(Language::Json, "{\n  \"a\": 1\n}".into()));
        assert_eq!(decoded.copy.as_deref(), Some(r#"{"a":1}"#));
        assert!(matches!(convert("%%%", Mode::Decode).conversion, Conversion::Failed { .. }));
        assert_eq!(convert("", Mode::Encode).conversion, Conversion::Empty);
    }

    #[test]
    fn decodes_what_looks_encoded_and_encodes_the_rest() {
        assert_eq!(mode("aGVsbG8gd29ybGQ="), Mode::Decode);
        assert_eq!(confidence("aGVsbG8gd29ybGQ="), 0.85);
        // Words are valid Base64 too, but decode to nothing readable.
        assert_eq!(mode("deployment"), Mode::Encode);
        assert_eq!(mode("hello"), Mode::Encode);
        assert_eq!(mode("{\"a\": 1}"), Mode::Encode);
        // Data, with its padding.
        assert_eq!(mode("/9j/4AAQSkZJRgABAQ=="), Mode::Decode);
    }
}
