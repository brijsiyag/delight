//! Formats: JSON, YAML, Base64, .env and JWT: format, convert, encode and decode.
//!
//! * this file: the plugin, its tools, and which inputs each one fits.
//! * `json`: format, minify, escape and unescape JSON.
//! * `yaml`: YAML ⇄ JSON, either way.
//! * `b64`: Base64, encoded and decoded.
//! * `env`: .env ⇄ JSON, either way.
//! * `jwt`: a JWT decoded and its signature verified; JSON signed as one.
//! * `field`: the one text field a tool shows in place of its result (a prefix, a secret).

mod b64;
mod env;
mod field;
mod json;
mod jwt;
mod yaml;

use delight_plugin_api::{AnyTool, Detection, Input, Operations, Plugin, plugin};
use gpui::{App, AppContext as _, AssetSource};

#[plugin(
    id = "delight_formats",
    name = "Formats",
    description = "JSON, YAML, Base64, .env and JWT: format, convert, encode and decode.",
    author = "Delight",
    icon = "assets/icon.svg",
    tags = ["json", "yaml", "base64", "env", "jwt", "format", "convert", "encode", "decode"],
    tips = ["Paste JSON to format it", "Paste a JWT to read its claims", "Paste YAML to get JSON"],
)]
struct Formats;

#[derive(Operations, Clone, Copy, PartialEq, Debug)]
enum FormatsOperation {
    #[operation(
        id = "json",
        title = "JSON",
        description = "Format, minify, escape or unescape JSON",
        icon = "assets/json.svg",
        tags = ["json", "format", "pretty", "minify", "escape", "unescape", "validate"],
    )]
    Json,
    #[operation(
        id = "yaml",
        title = "YAML ⇄ JSON",
        description = "Convert YAML to JSON, or JSON to YAML",
        icon = "assets/yaml.svg",
        tags = ["yaml", "yml", "json", "convert"],
    )]
    Yaml,
    #[operation(
        id = "base64",
        title = "Base64",
        description = "Encode text as Base64, or decode it",
        icon = "assets/base64.svg",
        tags = ["base64", "encode", "decode"],
    )]
    Base64,
    #[operation(
        id = "env",
        title = ".env ⇄ JSON",
        description = "Turn a JSON object into environment variables, or variables into JSON",
        icon = "assets/env.svg",
        tags = ["env", "dotenv", "json", "environment", "variables", "convert"],
    )]
    Env,
    #[operation(
        id = "jwt",
        title = "JWT",
        description = "Read a JWT's claims and verify its signature",
        icon = "assets/jwt.svg",
        tags = ["jwt", "token", "decode", "verify", "claims"],
    )]
    Jwt,
    #[operation(
        id = "json_to_jwt",
        title = "JSON → JWT",
        description = "Sign a JSON object as a JWT",
        icon = "assets/jwt.svg",
        tags = ["jwt", "token", "encode", "sign", "json"],
    )]
    JsonToJwt,
}

impl Plugin for Formats {
    type Operation = FormatsOperation;

    fn new(cx: &mut App) -> Self {
        delight_ui::init_plugin(cx);
        Formats
    }

    fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection<FormatsOperation>> {
        detect(&input.text).into_iter().map(|(operation, confidence)| Detection::new(operation, confidence)).collect()
    }

    fn open_tool(&mut self, operation: FormatsOperation, cx: &mut App) -> AnyTool {
        use FormatsOperation::*;
        match operation {
            Json => cx.new(json::JsonView::new).into(),
            Yaml => cx.new(|_| yaml::YamlView::default()).into(),
            Base64 => cx.new(b64::Base64View::new).into(),
            Env => cx.new(|_| env::EnvView::default()).into(),
            Jwt => cx.new(|_| jwt::JwtView::default()).into(),
            JsonToJwt => cx.new(jwt::SignView::new).into(),
        }
    }

    fn assets() -> Option<Box<dyn AssetSource>> {
        Some(Box::new(delight_ui::Assets))
    }
}

/// Every tool that fits `text`, and how surely. The input is parsed as JSON once, here,
/// for the tools that take JSON.
fn detect(text: &str) -> Vec<(FormatsOperation, f32)> {
    use FormatsOperation::*;
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let shape = json::shape(text);
    let mut found = Vec::new();
    if let Some(shape) = shape {
        found.push((Json, json::confidence(shape)));
    }
    found.extend(yaml::detect(text, shape).map(|confidence| (Yaml, confidence)));
    found.extend(env::detect(text, shape).map(|confidence| (Env, confidence)));
    found.extend(jwt::detect(text, shape).map(|(sign, confidence)| (if sign { JsonToJwt } else { Jwt }, confidence)));
    found.push((Base64, b64::confidence(text)));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use FormatsOperation::*;

    fn tools(text: &str) -> Vec<FormatsOperation> {
        let mut found = detect(text);
        found.sort_by(|a, b| b.1.total_cmp(&a.1));
        found.into_iter().map(|(operation, _)| operation).collect()
    }

    #[test]
    fn each_input_gets_its_tools_best_first() {
        assert_eq!(tools(r#"{"a": 1}"#), [Json, Yaml, Env, JsonToJwt, Base64]);
        assert_eq!(tools("[1, 2]"), [Json, Yaml, Base64]);
        assert_eq!(tools("name: api\nport: 80"), [Yaml, Base64]);
        assert_eq!(tools("DB_HOST=localhost\nDB_PORT=5432"), [Env, Base64]);
        assert_eq!(tools("aGVsbG8gd29ybGQ="), [Base64]);
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxIn0.c2ln";
        assert_eq!(tools(jwt)[0], Jwt);
        assert!(detect("").is_empty());
    }
}
