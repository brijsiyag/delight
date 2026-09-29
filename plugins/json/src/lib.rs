//! JSON: format, minify, escape and unescape.
//!
//! * this file: the plugin, and which inputs are JSON.
//! * `view`: the tool: the mode tabs, the formatting buttons, the result and its
//!   actions.
//! * `convert`: the conversions themselves.

mod convert;
mod view;

use delight_plugin_api::{AnyTool, Detection, Input, Operations, Plugin, plugin};
use gpui::{App, AppContext as _, AssetSource};
use serde_json::Value;

#[plugin(
    id = "delight.json",
    name = "JSON",
    description = "Format, minify, escape and unescape JSON.",
    author = "Delight",
    icon = "assets/icon.svg",
    tags = ["json", "format", "pretty print"],
    tips = ["Paste JSON to format it"],
)]
struct Json;

#[derive(Operations)]
enum JsonOperation {
    #[operation(
        id = "json",
        title = "JSON",
        description = "Format, minify, escape or unescape JSON",
        tags = ["json", "format", "pretty", "minify", "escape", "unescape", "validate"],
    )]
    Json,
}

impl Plugin for Json {
    type Operation = JsonOperation;

    fn new(cx: &mut App) -> Self {
        delight_ui::init_plugin(cx);
        Json
    }

    fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection<JsonOperation>> {
        confidence(&input.text).map(|confidence| Detection::new(JsonOperation::Json, confidence)).into_iter().collect()
    }

    fn open_tool(&mut self, operation: JsonOperation, cx: &mut App) -> AnyTool {
        match operation {
            JsonOperation::Json => cx.new(view::JsonView::new).into(),
        }
    }

    fn assets() -> Option<Box<dyn AssetSource>> {
        Some(Box::new(delight_ui::Assets))
    }
}

/// Larger inputs are detected by their first character only (parsing them on every
/// keystroke would be slow); the view still parses them.
const DETECT_PARSE_LIMIT: usize = 256 * 1024;

/// How surely `text` is JSON worth this tool.
fn confidence(text: &str) -> Option<f32> {
    let text = text.trim();
    let starts_container = matches!(text.as_bytes().first(), Some(b'{' | b'['));
    if !starts_container && !text.starts_with('"') {
        return None;
    }
    if text.len() > DETECT_PARSE_LIMIT {
        return starts_container.then_some(0.8);
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(_) | Value::Array(_)) => Some(0.92),
        // A JSON string holding JSON (often copied from logs).
        Ok(Value::String(s)) if convert::is_container(&s) => Some(0.9),
        Ok(_) => None,
        // Looks like JSON but doesn't parse: show where it breaks.
        Err(_) => starts_container.then_some(0.6),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_json_and_escaped_json() {
        assert_eq!(confidence(r#"{"a": 1}"#), Some(0.92));
        assert_eq!(confidence(r#""{\"a\":1}""#), Some(0.9));
        assert_eq!(confidence("{\"a\": "), Some(0.6));
        assert_eq!(confidence("hello"), None);
        assert_eq!(confidence(r#""just a string""#), None);
    }
}
