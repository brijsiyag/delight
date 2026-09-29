//! YAML ⇄ JSON: text in, [`Conversion`] out.
//!
//! `yaml-rust2` parses and writes YAML (1.2, pure Rust); the tree is mapped
//! to and from `serde_json::Value` here, so the edge cases are explicit:
//! scalar keys become strings, `.inf`/`.nan` (no JSON number for them)
//! become strings, and several documents (`---`) become a JSON array.

use delight_ui::code::Language;
use delight_ui::conversion::Conversion;
use serde_json::{Map, Number, Value};
use yaml_rust2::yaml::Hash;
use yaml_rust2::{Yaml, YamlEmitter, YamlLoader};

/// A parse error with its 1-based line and column.
pub struct ParseError {
    message: String,
    position: (usize, usize),
}

/// The documents in `text`.
pub fn load(text: &str) -> Result<Vec<Yaml>, ParseError> {
    YamlLoader::load_from_str(text).map_err(|e| {
        let marker = e.marker();
        let position = (marker.line(), marker.col() + 1);
        ParseError { message: format!("line {}, column {}: {}", position.0, position.1, e.info()), position }
    })
}

pub fn yaml_to_json(text: &str) -> Conversion {
    if text.is_empty() {
        return Conversion::Empty;
    }
    let docs = match load(text) {
        Ok(docs) => docs,
        Err(e) => return Conversion::failed(format!("Invalid YAML: {}", e.message), Some(e.position)),
    };
    let values = match docs.iter().map(to_json).collect::<Result<Vec<_>, _>>() {
        Ok(values) => values,
        Err(e) => return Conversion::failed(format!("Can't convert to JSON: {e}"), None),
    };
    let count = values.len();
    let value = match count {
        0 => Value::Null,
        1 => values.into_iter().next().expect("one document"),
        _ => Value::Array(values),
    };
    let pretty = serde_json::to_string_pretty(&value).expect("serializing a Value can't fail");
    let json = Conversion::text("JSON", Language::Json, pretty);
    if count > 1 { json.with_note(format!("{count} YAML documents → a JSON array")) } else { json }
}

pub fn json_to_yaml(text: &str) -> Conversion {
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
    match emit(&to_yaml(&value)) {
        Ok(yaml) => Conversion::text("YAML", Language::Yaml, yaml),
        Err(e) => Conversion::failed(format!("Can't write YAML: {e}"), None),
    }
}

fn scalar_key(y: &Yaml) -> Result<String, String> {
    Ok(match y {
        Yaml::String(s) | Yaml::Real(s) => s.clone(),
        Yaml::Integer(i) => i.to_string(),
        Yaml::Boolean(b) => b.to_string(),
        Yaml::Null => "null".into(),
        _ => return Err("a mapping key that isn't a plain value (JSON keys are strings)".into()),
    })
}

fn to_json(y: &Yaml) -> Result<Value, String> {
    Ok(match y {
        Yaml::Null => Value::Null,
        Yaml::Boolean(b) => Value::Bool(*b),
        Yaml::Integer(i) => Value::from(*i),
        Yaml::Real(s) => match y.as_f64().and_then(Number::from_f64) {
            Some(n) => Value::Number(n),
            // `.inf`, `.nan`: no JSON number for them.
            None => Value::String(s.clone()),
        },
        Yaml::String(s) => Value::String(s.clone()),
        Yaml::Array(items) => Value::Array(items.iter().map(to_json).collect::<Result<_, _>>()?),
        Yaml::Hash(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                out.insert(scalar_key(k)?, to_json(v)?);
            }
            Value::Object(out)
        }
        Yaml::Alias(_) => return Err("an unresolved alias".into()),
        Yaml::BadValue => return Err("an invalid value".into()),
    })
}

fn to_yaml(v: &Value) -> Yaml {
    match v {
        Value::Null => Yaml::Null,
        Value::Bool(b) => Yaml::Boolean(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Yaml::Integer(i),
            None => Yaml::Real(n.to_string()),
        },
        Value::String(s) => Yaml::String(s.clone()),
        Value::Array(items) => Yaml::Array(items.iter().map(to_yaml).collect()),
        Value::Object(map) => {
            let mut out = Hash::new();
            for (k, v) in map {
                out.insert(Yaml::String(k.clone()), to_yaml(v));
            }
            Yaml::Hash(out)
        }
    }
}

fn emit(y: &Yaml) -> Result<String, String> {
    let mut out = String::new();
    let mut emitter = YamlEmitter::new(&mut out);
    emitter.multiline_strings(true);
    emitter.dump(y).map_err(|e| e.to_string())?;
    // The emitter starts every document with `---`.
    let body = out.strip_prefix("---\n").or_else(|| out.strip_prefix("---")).unwrap_or(&out);
    Ok(body.trim_start().to_string() + "\n")
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

    fn parse(yaml: &str) -> Value {
        to_json(&load(yaml).ok().unwrap()[0]).unwrap()
    }

    const K8S: &str = "apiVersion: v1\nkind: Service\nmetadata:\n  name: api\n  labels:\n    app: api\nspec:\n  ports:\n    - port: 80\n      targetPort: 8080\n";

    #[test]
    fn yaml_to_json_and_back() {
        let json = text(yaml_to_json(K8S));
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["spec"]["ports"][0]["targetPort"], 8080);
        assert_eq!(v["metadata"]["labels"]["app"], "api");
        // Key order is kept.
        assert!(json.find("apiVersion").unwrap() < json.find("spec").unwrap());

        let yaml = text(json_to_yaml(&json));
        assert_eq!(parse(&yaml), v);
        assert!(!yaml.starts_with("---"));
    }

    #[test]
    fn edge_cases() {
        // Several documents → an array, with a note.
        let conversion = yaml_to_json("a: 1\n---\nb: 2\n");
        assert!(matches!(&conversion, Conversion::Text { note: Some(note), .. } if note.contains("2 YAML documents")));
        assert_eq!(serde_json::from_str::<Value>(&text(conversion)).unwrap(), serde_json::json!([{"a": 1}, {"b": 2}]));
        // Non-string keys, specials, anchors.
        let v: Value =
            serde_json::from_str(&text(yaml_to_json("1: one\ntrue: yes\ninf: .inf\nbase: &b {x: 1}\nuse: *b\n"))).unwrap();
        assert_eq!(v["1"], "one");
        assert_eq!(v["true"], "yes");
        assert_eq!(v["inf"], ".inf");
        assert_eq!(v["use"]["x"], 1);
        // Errors point at the line.
        assert!(matches!(
            yaml_to_json("a: [1, 2\nb: 3"),
            Conversion::Failed { message, position: Some(_) } if message.contains("line")
        ));
        // Strings that look like other types stay strings in YAML.
        let yaml = text(json_to_yaml(r#"{"s": "true", "n": "123", "e": ""}"#));
        assert_eq!(parse(&yaml), serde_json::json!({"s": "true", "n": "123", "e": ""}));
    }
}
