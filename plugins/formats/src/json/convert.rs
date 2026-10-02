//! The JSON conversions: text in, [`Conversion`] out.

use delight_ui::code::Language;
use delight_ui::conversion::Conversion;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Format,
    Minify,
    Escape,
    Unescape,
}

/// Spaces per level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Indent {
    #[default]
    Two,
    Four,
}

impl Indent {
    fn bytes(self) -> &'static [u8] {
        match self {
            Indent::Two => b"  ",
            Indent::Four => b"    ",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub mode: Mode,
    pub indent: Indent,
    pub sort_keys: bool,
}

/// The result, and (for a formatted one) the JSON minified.
pub fn convert(input: &str, options: Options) -> (Conversion, Option<String>) {
    let text = input.trim();
    if text.is_empty() {
        return (Conversion::Empty, None);
    }
    let parse_error = |e: serde_json::Error| {
        let message = format!("Invalid JSON: line {}, column {}: {e}", e.line(), e.column());
        Conversion::failed(message, Some((e.line(), e.column())))
    };
    match options.mode {
        Mode::Format => {
            let value: Value = match serde_json::from_str(text) {
                Ok(value) => value,
                Err(e) => return (parse_error(e), None),
            };
            // A JSON string holding JSON: format what's inside it.
            let value = match value {
                Value::String(s) if is_container(&s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
                other => other,
            };
            let value = if options.sort_keys { sort_keys(value) } else { value };
            (Conversion::text(Language::Json, pretty(&value, options.indent)), Some(value.to_string()))
        }
        Mode::Minify => match serde_json::from_str::<Value>(text) {
            Ok(value) => (Conversion::text(Language::Json, value.to_string()), None),
            Err(e) => (parse_error(e), None),
        },
        // Valid JSON is compacted first; any other text is escaped as it is.
        Mode::Escape => {
            let raw = serde_json::from_str::<Value>(text).map(|v| v.to_string()).unwrap_or_else(|_| text.to_string());
            (Conversion::text(Language::Json, Value::String(raw).to_string()), None)
        }
        Mode::Unescape => match unescape(text) {
            Some(s) => {
                let out = match serde_json::from_str::<Value>(&s) {
                    Ok(value @ (Value::Object(_) | Value::Array(_))) => pretty(&value, options.indent),
                    _ => s,
                };
                (Conversion::text(Language::Json, out), None)
            }
            None => (Conversion::failed("Not a JSON string literal", None), None),
        },
    }
}

/// Whether `s` holds a JSON object or array.
pub fn is_container(s: &str) -> bool {
    matches!(s.trim_start().as_bytes().first(), Some(b'{' | b'['))
        && matches!(serde_json::from_str::<Value>(s), Ok(Value::Object(_) | Value::Array(_)))
}

fn sort_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().map(|(k, v)| (k, sort_keys(v))).collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

/// `value` as indented JSON.
pub fn pretty(value: &Value, indent: Indent) -> String {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.bytes());
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
    value.serialize(&mut serializer).expect("serializing a Value can't fail");
    String::from_utf8(out).expect("serde_json writes UTF-8")
}

/// The content of a JSON string literal; also accepts the literal without
/// its quotes (`{\"a\":1}`), as it's often copied from logs.
fn unescape(text: &str) -> Option<String> {
    serde_json::from_str::<String>(text).ok().or_else(|| serde_json::from_str::<String>(&format!("\"{text}\"")).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str, options: Options) -> String {
        match convert(input, options).0 {
            Conversion::Text { text, .. } => text,
            Conversion::Failed { message, .. } => format!("error: {message}"),
            Conversion::Empty => String::new(),
        }
    }

    fn mode(mode: Mode) -> Options {
        Options { mode, ..Options::default() }
    }

    #[test]
    fn formats_keeping_order_or_sorting() {
        assert_eq!(run(r#"{"b":1,"a":[true]}"#, Options::default()), "{\n  \"b\": 1,\n  \"a\": [\n    true\n  ]\n}");
        let sorted = Options { indent: Indent::Four, sort_keys: true, ..Options::default() };
        assert_eq!(run(r#"{"b":1,"a":2}"#, sorted), "{\n    \"a\": 2,\n    \"b\": 1\n}");
    }

    #[test]
    fn reports_where_it_breaks() {
        let (conversion, _) = convert("{\n  \"a\": 1,\n  \"b\" 2\n}", Options::default());
        let Conversion::Failed { message, position } = conversion else { panic!("expected an error") };
        assert!(message.contains("line 3"));
        assert_eq!(position.map(|(line, _)| line), Some(3));
    }

    #[test]
    fn modes() {
        assert_eq!(run(r#"{ "a" : [1, 2] }"#, mode(Mode::Minify)), r#"{"a":[1,2]}"#);
        assert_eq!(run(r#"{"a": 1}"#, mode(Mode::Escape)), r#""{\"a\":1}""#);
        assert_eq!(run("say \"hi\"", mode(Mode::Escape)), r#""say \"hi\"""#);
        assert_eq!(run(r#""{\"a\":1}""#, mode(Mode::Unescape)), "{\n  \"a\": 1\n}");
        assert_eq!(run(r#"{\"a\":1}"#, mode(Mode::Unescape)), "{\n  \"a\": 1\n}");
    }

    #[test]
    fn format_unwraps_escaped_json() {
        let (conversion, minified) = convert(r#""{\"a\":1}""#, Options::default());
        assert!(matches!(conversion, Conversion::Text { text, .. } if text == "{\n  \"a\": 1\n}"));
        assert_eq!(minified.as_deref(), Some(r#"{"a":1}"#));
    }
}
