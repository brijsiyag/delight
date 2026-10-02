//! JSON: format, minify, escape and unescape.
//!
//! * this file: which inputs are JSON, for this tool and the others that take JSON.
//! * `view`: the tool: the mode tabs and the formatting buttons, the result and its
//!   actions.
//! * `convert`: the conversions themselves.

mod convert;
mod view;

use serde_json::Value;

pub use view::JsonView;

/// Larger inputs are judged by their first character only (parsing them on every
/// keystroke would be slow); the tools still parse them.
const DETECT_PARSE_LIMIT: usize = 256 * 1024;

/// What a (trimmed) input is as JSON.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Object,
    Array,
    /// A JSON string holding an object or an array (often copied from logs).
    Escaped,
    /// Too large to parse while typing, starting as an object (`{`) or an array does.
    Large { object: bool },
    /// Starts as an object or an array does, but doesn't parse: worth showing where it breaks.
    Broken,
}

/// `text` as JSON, if it looks like JSON this tool is for.
pub fn shape(text: &str) -> Option<Shape> {
    let first = *text.as_bytes().first()?;
    let container = matches!(first, b'{' | b'[');
    if !container && first != b'"' {
        return None;
    }
    if text.len() > DETECT_PARSE_LIMIT {
        return container.then_some(Shape::Large { object: first == b'{' });
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(_)) => Some(Shape::Object),
        Ok(Value::Array(_)) => Some(Shape::Array),
        Ok(Value::String(s)) if convert::is_container(&s) => Some(Shape::Escaped),
        Ok(_) => None,
        Err(_) => container.then_some(Shape::Broken),
    }
}

/// How surely the JSON tool fits an input of this shape.
pub fn confidence(shape: Shape) -> f32 {
    match shape {
        Shape::Object | Shape::Array => 0.92,
        Shape::Escaped => 0.9,
        Shape::Large { .. } => 0.8,
        Shape::Broken => 0.6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_json_and_escaped_json() {
        assert_eq!(shape(r#"{"a": 1}"#), Some(Shape::Object));
        assert_eq!(shape(r#""{\"a\":1}""#), Some(Shape::Escaped));
        assert_eq!(shape("{\"a\": "), Some(Shape::Broken));
        assert_eq!(shape("hello"), None);
        assert_eq!(shape(r#""just a string""#), None);
        assert_eq!(confidence(Shape::Object), 0.92);
        assert_eq!(confidence(Shape::Broken), 0.6);
    }
}
