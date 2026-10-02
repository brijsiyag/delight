//! .env ⇄ JSON, one tool: a JSON object's values as environment variables, one per line,
//! and a .env file's variables as a JSON object.
//!
//! * this file: which inputs fit, which way they go, and the conversions.
//! * `view`: the tool.
//!
//! No crate for .env is used: `dotenvy`, the common one, also expands `$VARIABLES` (from
//! the process's environment, empty in a plugin), which a conversion must leave as written.

mod view;

use delight_ui::code::Language;
use delight_ui::conversion::Conversion;
use serde_json::{Map, Value};

use crate::json::Shape;

pub use view::EnvView;

/// Lines detection looks at.
const DETECT_LINES: usize = 400;

/// How surely the tool fits `text`: as variables, or as a JSON object to turn into them
/// (below the JSON tool and YAML). `json` is what the input is as JSON.
pub fn detect(text: &str, json: Option<Shape>) -> Option<f32> {
    match json {
        Some(Shape::Object) => return Some(0.4),
        Some(Shape::Large { object: true }) => return Some(0.35),
        Some(_) => return None,
        None => {}
    }
    let lines: Vec<&str> =
        text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')).take(DETECT_LINES).collect();
    let assignments: Vec<(&str, &str)> = lines.iter().filter_map(|line| assignment(line)).collect();
    // Most lines are assignments (a quoted value can span lines).
    if assignments.is_empty() || (assignments.len() as f32) < 0.8 * lines.len() as f32 {
        return None;
    }
    match assignments[..] {
        [_, _, ..] => Some(0.85),
        // One `name=value` is often something else (`a=1`, padded Base64): only an
        // upper-case name with a value, as variables are written.
        [(name, value)] => (is_upper_case(name) && !value.trim().is_empty()).then_some(0.6),
        [] => None,
    }
}

/// Which way `text` goes: JSON (`{` or `[` first) to variables, anything else from
/// variables to JSON.
pub fn to_env(text: &str) -> bool {
    matches!(text.trim_start().as_bytes().first(), Some(b'{' | b'['))
}

/// `text` converted the way it goes (see [`to_env`]); `prefix` is put before every name.
pub fn convert(text: &str, prefix: &str) -> Conversion {
    if to_env(text) { json_to_env(text, prefix) } else { env_to_json(text) }
}

/// `NAME=value`, with or without `export`: its name and its value as written.
fn assignment(line: &str) -> Option<(&str, &str)> {
    let line = line.strip_prefix("export ").map_or(line, str::trim_start);
    let (name, value) = line.split_once('=')?;
    let name = name.trim_end();
    is_name(name).then_some((name, value))
}

/// A variable's name: letters, digits, `_`, `.` and `-`, not starting with a digit.
fn is_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

fn is_upper_case(name: &str) -> bool {
    name.chars().any(|c| c.is_ascii_uppercase()) && !name.chars().any(|c| c.is_ascii_lowercase())
}

// ---------------------------------------------------------------------------
// JSON → .env
// ---------------------------------------------------------------------------

/// A JSON object as variables, one per line: nested keys joined by `_` (`database.host`
/// → `DATABASE_HOST`), array items by their index, each name after `prefix`.
pub fn json_to_env(text: &str, prefix: &str) -> Conversion {
    if text.is_empty() {
        return Conversion::Empty;
    }
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(e) => {
            let message = format!("Invalid JSON: line {}, column {}: {e}", e.line(), e.column());
            return Conversion::failed(message, Some((e.line(), e.column())));
        }
    };
    let Value::Object(map) = value else {
        return Conversion::failed("Not a JSON object: its keys name the variables", None);
    };
    let prefix = name_part(prefix);
    let mut lines = Vec::new();
    for (key, value) in &map {
        flatten(&join(&prefix, &name_part(key)), value, &mut lines);
    }
    Conversion::text(Language::Env, lines.join("\n"))
}

fn flatten(name: &str, value: &Value, lines: &mut Vec<String>) {
    match value {
        Value::Object(map) if !map.is_empty() => map.iter().for_each(|(key, value)| flatten(&join(name, &name_part(key)), value, lines)),
        Value::Array(items) if !items.is_empty() => {
            items.iter().enumerate().for_each(|(index, value)| flatten(&join(name, &index.to_string()), value, lines))
        }
        // Nothing in it, or null: an empty value.
        Value::Object(_) | Value::Array(_) | Value::Null => lines.push(format!("{name}=")),
        Value::String(text) => lines.push(format!("{name}={}", quoted(text))),
        number_or_bool => lines.push(format!("{name}={number_or_bool}")),
    }
}

fn join(name: &str, part: &str) -> String {
    match (name.is_empty(), part.is_empty()) {
        (true, _) => part.to_string(),
        (_, true) => name.to_string(),
        _ => format!("{name}_{part}"),
    }
}

/// A key as part of a variable's name, upper case: `timeoutMs` → `TIMEOUT_MS`, `api-key`
/// → `API_KEY`.
fn name_part(key: &str) -> String {
    let mut part = String::new();
    let mut previous: Option<char> = None;
    for c in key.chars() {
        if c.is_ascii_alphanumeric() {
            let starts_word = c.is_ascii_uppercase() && previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit());
            if starts_word && !part.ends_with('_') {
                part.push('_');
            }
            part.push(c.to_ascii_uppercase());
        } else if !part.is_empty() && !part.ends_with('_') {
            part.push('_');
        }
        previous = Some(c);
    }
    part.trim_end_matches('_').to_string()
}

/// A value as written in a .env file: as it is, or double-quoted (with `\` escapes) when it
/// has spaces, quotes, `#`, `$` or line breaks in it.
fn quoted(value: &str) -> String {
    let plain = !value.chars().any(|c| c.is_whitespace() || matches!(c, '#' | '"' | '\'' | '\\' | '$' | '`'));
    if plain {
        return value.to_string();
    }
    let mut out = String::from('"');
    for c in value.chars() {
        match c {
            '"' | '\\' | '$' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Variables as `export NAME=value` lines, for a shell.
pub fn exported(env: &str) -> String {
    env.lines().map(|line| format!("export {line}")).collect::<Vec<_>>().join("\n")
}

// ---------------------------------------------------------------------------
// .env → JSON
// ---------------------------------------------------------------------------

/// A .env file's variables as a JSON object of strings, in their order (a name set twice
/// keeps the later value).
pub fn env_to_json(text: &str) -> Conversion {
    if text.is_empty() {
        return Conversion::Empty;
    }
    match parse(text) {
        Ok(variables) => {
            let object: Map<String, Value> = variables.into_iter().map(|(name, value)| (name, Value::String(value))).collect();
            let pretty = serde_json::to_string_pretty(&Value::Object(object)).expect("serializing a Value can't fail");
            Conversion::text(Language::Json, pretty)
        }
        Err((line, message)) => Conversion::failed(format!("Line {line}: {message}"), Some((line, 1))),
    }
}

/// The variables in a .env file: blank lines and comments skipped, `export` dropped, and
/// quotes and escapes resolved. A quoted value can span lines. Errors give their line.
pub fn parse(text: &str) -> Result<Vec<(String, String)>, (usize, String)> {
    let mut variables = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        let number = index + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").map_or(line, str::trim_start);
        let Some((name, rest)) = line.split_once('=') else {
            return Err((number, "not NAME=value".into()));
        };
        let name = name.trim_end();
        if !is_name(name) {
            return Err((number, format!("{name:?} isn't a variable's name")));
        }
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(quote @ ('"' | '\'')) => {
                let mut body = rest[1..].to_string();
                loop {
                    // What follows the closing quote (a comment) is left out.
                    if let Some(end) = closing(&body, quote) {
                        break if quote == '"' { unescape(&body[..end]) } else { body[..end].to_string() };
                    }
                    match lines.next() {
                        Some((_, next)) => {
                            body.push('\n');
                            body.push_str(next);
                        }
                        None => return Err((number, format!("the value's {quote} is never closed"))),
                    }
                }
            }
            // A comment after an unquoted value starts with ` #`.
            _ => rest.split(" #").next().unwrap_or_default().trim_end().to_string(),
        };
        variables.push((name.to_string(), value));
    }
    Ok(variables)
}

/// Where `body`'s closing `quote` is; in double quotes, `\"` doesn't close.
fn closing(body: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (index, c) in body.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if quote == '"' => escaped = true,
            c if c == quote => return Some(index),
            _ => {}
        }
    }
    None
}

/// A double-quoted value's escapes: `\n`, `\r`, `\t`, and `\` before any other character.
fn unescape(body: &str) -> String {
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(conversion: Conversion) -> String {
        match conversion {
            Conversion::Text { text, .. } => text,
            Conversion::Failed { message, .. } => panic!("unexpected error: {message}"),
            Conversion::Empty => panic!("unexpected empty result"),
        }
    }

    #[test]
    fn detects_variables_but_not_a_single_word_or_base64() {
        let detect = |text: &str| detect(text, crate::json::shape(text));
        assert_eq!(detect("DB_HOST=localhost\n# the port\nexport DB_PORT=5432"), Some(0.85));
        assert_eq!(detect("PATH=/usr/bin"), Some(0.6));
        assert_eq!(detect(r#"{"a": 1}"#), Some(0.4));
        assert_eq!(detect("[1]"), None);
        assert_eq!(detect("a=1"), None);
        assert_eq!(detect("aGVsbG8gd29ybGQ="), None);
        assert_eq!(detect("name: api\nport: 80"), None);
        assert_eq!(detect("hello there"), None);
    }

    #[test]
    fn json_becomes_one_variable_per_line() {
        let json = r#"{"database": {"host": "localhost", "port": 5432, "password": "s3cr3t pass"},
            "timeoutMs": 300, "debug": false, "features": ["search", "export"], "note": null, "empty": {}}"#;
        assert_eq!(
            text(json_to_env(json, "")),
            "DATABASE_HOST=localhost\nDATABASE_PORT=5432\nDATABASE_PASSWORD=\"s3cr3t pass\"\nTIMEOUT_MS=300\nDEBUG=false\n\
             FEATURES_0=search\nFEATURES_1=export\nNOTE=\nEMPTY="
        );
        assert_eq!(text(json_to_env(r#"{"url": "redis://h:6379"}"#, "app")), "APP_URL=redis://h:6379");
        assert_eq!(text(json_to_env(r#"{"a": 1}"#, "APP_")), "APP_A=1");
        assert_eq!(text(json_to_env(r#"{"s": "say \"$HOME\"\n"}"#, "")), r#"S="say \"\$HOME\"\n""#);
        assert!(matches!(json_to_env("[1]", ""), Conversion::Failed { .. }));
        // One tool, both ways.
        assert_eq!(text(convert(r#"{"a": 1}"#, "")), "A=1");
        assert_eq!(text(convert("A=1", "")), "{\n  \"A\": \"1\"\n}");
        assert_eq!(exported("A=1\nB=2"), "export A=1\nexport B=2");
    }

    #[test]
    fn names_from_keys() {
        assert_eq!(name_part("timeoutMs"), "TIMEOUT_MS");
        assert_eq!(name_part("api-key"), "API_KEY");
        assert_eq!(name_part("ipv4Address"), "IPV4_ADDRESS");
        assert_eq!(name_part("a__b"), "A_B");
    }

    #[test]
    fn env_becomes_an_object_of_strings() {
        let env = "# comment\nexport HOST=localhost\nPORT = 5432\nNAME=\"Ada \\\"L\\\"\" # who\nRAW='$HOME \\n'\nURL=https://x?a=1 # note\nKEY=\"line 1\nline 2\"\nHOST=override\n";
        let json: Value = serde_json::from_str(&text(env_to_json(env))).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"HOST": "override", "PORT": "5432", "NAME": "Ada \"L\"", "RAW": "$HOME \\n",
                "URL": "https://x?a=1", "KEY": "line 1\nline 2"})
        );
        assert!(matches!(env_to_json("HOST localhost"), Conversion::Failed { position: Some((1, 1)), .. }));
        assert!(matches!(env_to_json("A=\"open"), Conversion::Failed { .. }));
    }
}
