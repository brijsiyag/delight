//! YAML ⇄ JSON: paste YAML to get JSON, paste JSON to get YAML.
//!
//! * this file: the plugin, and which inputs are YAML (or JSON to turn into YAML).
//! * `view`: the tool: the converted text and its action.
//! * `convert`: the conversions, and the parsing detection uses.

mod convert;
mod view;

use delight_plugin_api::{AnyTool, Detection, Input, Operations, Plugin, plugin};
use gpui::{App, AppContext as _, AssetSource};
use serde_json::Value;
use yaml_rust2::Yaml;

#[plugin(
    id = "delight.yaml",
    name = "YAML",
    description = "Convert YAML to JSON and JSON to YAML.",
    author = "Delight",
    icon = "assets/icon.svg",
    tags = ["yaml", "yml", "json", "convert"],
    tips = ["Paste YAML to get JSON, or JSON to get YAML"],
)]
struct YamlPlugin;

#[derive(Operations, Clone, Copy, PartialEq)]
enum YamlOperation {
    #[operation(
        id = "yaml_to_json",
        title = "YAML → JSON",
        description = "Convert YAML to JSON",
        tags = ["yaml", "json", "convert"],
    )]
    YamlToJson,
    #[operation(
        id = "json_to_yaml",
        title = "JSON → YAML",
        description = "Convert JSON to YAML",
        tags = ["json", "yaml", "convert"],
    )]
    JsonToYaml,
}

impl Plugin for YamlPlugin {
    type Operation = YamlOperation;

    fn new(cx: &mut App) -> Self {
        delight_ui::init_plugin(cx);
        YamlPlugin
    }

    fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection<YamlOperation>> {
        detect(&input.text).map(|(operation, confidence)| Detection::new(operation, confidence)).into_iter().collect()
    }

    fn open_tool(&mut self, operation: YamlOperation, cx: &mut App) -> AnyTool {
        let to_json = operation == YamlOperation::YamlToJson;
        cx.new(|_| view::YamlView::new(to_json)).into()
    }

    fn assets() -> Option<Box<dyn AssetSource>> {
        Some(Box::new(delight_ui::Assets))
    }
}

/// Lines the cheap shape check looks at.
const DETECT_LINES: usize = 400;
/// Inputs up to this size are parsed to confirm a detection; larger ones rely on the
/// shape check (the view parses them).
const DETECT_PARSE_LIMIT: usize = 256 * 1024;

/// Which direction fits `text`, and how surely.
fn detect(text: &str) -> Option<(YamlOperation, f32)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    // JSON is also valid YAML: offer the JSON → YAML direction, below the JSON tool.
    if matches!(text.as_bytes().first(), Some(b'{' | b'[')) {
        let is_json = text.len() > DETECT_PARSE_LIMIT
            || matches!(serde_json::from_str::<Value>(text), Ok(Value::Object(_) | Value::Array(_)));
        return is_json.then_some((YamlOperation::JsonToYaml, 0.5));
    }
    // Cheap shape check first: plain text is valid YAML too (a scalar).
    let (structured, total) = yaml_shape(text);
    if structured == 0 || (structured as f32) < 0.8 * total as f32 {
        return None;
    }
    // One `key: value` line could be prose ("Note: call me later"): recommended only
    // when its value is one word or quoted.
    let confidence = match structured {
        2.. => 0.85,
        _ if single_value(text) => 0.6,
        _ => 0.35,
    };
    if text.len() > DETECT_PARSE_LIMIT {
        return Some((YamlOperation::YamlToJson, confidence * 0.9));
    }
    match convert::load(text) {
        Ok(docs) if matches!(docs.first(), Some(Yaml::Hash(_) | Yaml::Array(_))) => {
            Some((YamlOperation::YamlToJson, confidence))
        }
        _ => None,
    }
}

/// Whether a one-line `key: value`'s value is a single word (`api`, `8080`, `true`) or
/// quoted, not a sentence.
fn single_value(line: &str) -> bool {
    let value = line.split_once(':').map_or("", |(_, value)| value.trim());
    let quoted = value.len() >= 2
        && [('"', '"'), ('\'', '\''), ('[', ']'), ('{', '}')]
            .iter()
            .any(|&(open, close)| value.starts_with(open) && value.ends_with(close));
    quoted || !value.contains(char::is_whitespace)
}

/// How many of the first lines are YAML structure (`key: value`, `key:`, `- item`,
/// `---`), out of the meaningful ones.
fn yaml_shape(text: &str) -> (usize, usize) {
    let (mut structured, mut total) = (0, 0);
    for line in text.lines().take(DETECT_LINES) {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        total += 1;
        let is_key = l.split_once(':').is_some_and(|(key, rest)| {
            let key = key.trim_matches(|c| c == '"' || c == '\'');
            !key.is_empty()
                && !key.contains(char::is_whitespace)
                && (rest.is_empty() || rest.starts_with(' '))
                && !key.contains("//")
        });
        let is_item = l == "-" || l.starts_with("- ");
        // Continues a block scalar or a nested value.
        let is_nested = line.starts_with(' ') && total > 1;
        if is_key || is_item || l == "---" || is_nested {
            structured += 1;
        }
    }
    (structured, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    const K8S: &str = "apiVersion: v1\nkind: Service\nmetadata:\n  name: api\n";

    #[test]
    fn detects_yaml_but_not_prose_json_or_env() {
        let (operation, confidence) = detect(K8S).unwrap();
        assert!(operation == YamlOperation::YamlToJson && confidence > 0.8);
        assert!(detect(r#"{"a": 1}"#) == Some((YamlOperation::JsonToYaml, 0.5)));
        assert!(detect("hello there").is_none());
        assert!(detect("DB_HOST=localhost\nDB_PORT=5432").is_none());
        assert!(detect("Note: this is a sentence, not config.\nIt goes on for a while here.").is_none());
        assert!(detect("curl https://x/y").is_none());
        assert!(detect("{\"broken\": ").is_none());
        // A single key line: recommended when its value is a word or quoted; offered,
        // but low, when it reads like a sentence.
        let confidence = |text| detect(text).unwrap().1;
        assert!(confidence("name: api") >= 0.5);
        assert!(confidence("key: Val") >= 0.5);
        assert!(confidence("title: \"Hello there\"") >= 0.5);
        assert!(confidence("Note: call me later") < 0.5);
    }
}
