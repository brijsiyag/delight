//! JWT: a token's header and claims, its signature verified with a secret, and JSON signed
//! as a token. Signatures are HMAC (HS256, HS384, HS512), the ones a secret makes; a token
//! signed otherwise (RS256, ES256…) is read, but not verified.
//!
//! * this file: which inputs fit, decoding, signing and verifying, and the claims as rows.
//! * `view`: the two tools: the token read (and verified), and JSON signed.

mod view;

use base64::Engine as _;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use chrono::{DateTime, FixedOffset};
use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use serde_json::{Map, Value};
use sha2::{Sha256, Sha384, Sha512};

use crate::json::Shape;

pub use view::{JwtView, SignView};

/// Which tool fits `text` (signing JSON: `true`), and how surely. `json` is what the input
/// is as JSON: an object can be signed (offered low, below the other JSON tools).
pub fn detect(text: &str, json: Option<Shape>) -> Option<(bool, f32)> {
    if json == Some(Shape::Object) {
        return Some((true, 0.3));
    }
    // A header starts `{"`, which is `eyJ` in Base64.
    let looks = text.starts_with("eyJ") && text.matches('.').count() == 2 && !text.contains(char::is_whitespace);
    let decoded = looks.then(|| decode(text).ok()).flatten();
    decoded.filter(|jwt| jwt.header.contains_key("alg")).map(|_| (false, 0.95))
}

/// The algorithms a secret signs with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Algorithm {
    #[default]
    HS256,
    HS384,
    HS512,
}

impl Algorithm {
    pub const ALL: [Algorithm; 3] = [Algorithm::HS256, Algorithm::HS384, Algorithm::HS512];

    pub fn name(self) -> &'static str {
        match self {
            Algorithm::HS256 => "HS256",
            Algorithm::HS384 => "HS384",
            Algorithm::HS512 => "HS512",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|algorithm| algorithm.name() == name)
    }

    fn sign(self, secret: &[u8], data: &[u8]) -> Vec<u8> {
        match self {
            Algorithm::HS256 => keyed::<Hmac<Sha256>>(secret, data).finalize().into_bytes().to_vec(),
            Algorithm::HS384 => keyed::<Hmac<Sha384>>(secret, data).finalize().into_bytes().to_vec(),
            Algorithm::HS512 => keyed::<Hmac<Sha512>>(secret, data).finalize().into_bytes().to_vec(),
        }
    }

    /// Whether `signature` is `data`'s with `secret`, compared in constant time.
    fn verify(self, secret: &[u8], data: &[u8], signature: &[u8]) -> bool {
        match self {
            Algorithm::HS256 => keyed::<Hmac<Sha256>>(secret, data).verify_slice(signature).is_ok(),
            Algorithm::HS384 => keyed::<Hmac<Sha384>>(secret, data).verify_slice(signature).is_ok(),
            Algorithm::HS512 => keyed::<Hmac<Sha512>>(secret, data).verify_slice(signature).is_ok(),
        }
    }
}

fn keyed<M: Mac + KeyInit>(secret: &[u8], data: &[u8]) -> M {
    let mut mac = <M as KeyInit>::new_from_slice(secret).expect("HMAC takes a key of any length");
    Mac::update(&mut mac, data);
    mac
}

/// A token, decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct Jwt {
    pub header: Map<String, Value>,
    pub payload: Value,
    /// `header.payload`, as it was signed.
    signed: String,
    signature: Vec<u8>,
}

impl Jwt {
    /// The algorithm its header names.
    pub fn algorithm(&self) -> Option<&str> {
        self.header.get("alg")?.as_str()
    }

    /// The HMAC algorithm it's signed with: a secret verifies only those.
    pub fn hmac(&self) -> Option<Algorithm> {
        Algorithm::from_name(self.algorithm()?)
    }

    /// Whether `secret` made its signature; `None` if a secret can't tell (not HMAC).
    pub fn verify(&self, secret: &str) -> Option<bool> {
        Some(self.hmac()?.verify(secret.as_bytes(), self.signed.as_bytes(), &self.signature))
    }
}

/// `text`'s three parts: the header and the payload, JSON in Base64URL, and the signature.
pub fn decode(text: &str) -> Result<Jwt, String> {
    let token = text.trim();
    let parts: Vec<&str> = token.split('.').collect();
    let [header, payload, signature] = parts[..] else {
        return Err("Not a JWT: it has three parts, separated by dots".into());
    };
    let json = |part: &str, name: &str| -> Result<Value, String> {
        let bytes = base64url(part).ok_or_else(|| format!("The {name} isn't Base64URL"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("The {name} isn't JSON: {e}"))
    };
    let Value::Object(header_map) = json(header, "header")? else {
        return Err("The header isn't a JSON object".into());
    };
    Ok(Jwt {
        header: header_map,
        payload: json(payload, "payload")?,
        signed: format!("{header}.{payload}"),
        signature: base64url(signature).ok_or("The signature isn't Base64URL")?,
    })
}

fn base64url(part: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(part).or_else(|_| URL_SAFE.decode(part)).ok()
}

/// `payload` (a JSON object) as a token signed with `secret`; its header is `alg` and `typ`.
pub fn sign(payload: &str, algorithm: Algorithm, secret: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(payload).map_err(|e| format!("Invalid JSON: line {}, column {}: {e}", e.line(), e.column()))?;
    if !value.is_object() {
        return Err("Not a JSON object: a JWT's payload is one".into());
    }
    let header = serde_json::json!({ "alg": algorithm.name(), "typ": "JWT" });
    let signed = format!("{}.{}", URL_SAFE_NO_PAD.encode(header.to_string()), URL_SAFE_NO_PAD.encode(value.to_string()));
    let signature = URL_SAFE_NO_PAD.encode(algorithm.sign(secret.as_bytes(), signed.as_bytes()));
    Ok(format!("{signed}.{signature}"))
}

// ---------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------

/// How a claim's value reads: a time past (`exp`) or not yet come (`nbf`) is bad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Good,
    Bad,
}

/// A registered claim, read for people.
#[derive(Debug, Clone, PartialEq)]
pub struct Claim {
    pub label: &'static str,
    pub value: String,
    /// For times: how long ago, or how long until.
    pub hint: Option<String>,
    pub tone: Tone,
}

/// The registered claims, in the order they're read, and their labels.
const REGISTERED: [(&str, &str); 7] = [
    ("sub", "Subject"),
    ("iss", "Issuer"),
    ("aud", "Audience"),
    ("iat", "Issued"),
    ("nbf", "Not before"),
    ("exp", "Expires"),
    ("jti", "ID"),
];

/// The payload's registered claims as rows (times in the user's time zone, `offset` seconds
/// east of UTC, and against `now`), and the other claims.
pub fn claims(payload: &Map<String, Value>, now: i64, offset: i32) -> (Vec<Claim>, Map<String, Value>) {
    let mut rows = Vec::new();
    for (key, label) in REGISTERED {
        let Some(value) = payload.get(key) else { continue };
        let time = matches!(key, "iat" | "nbf" | "exp").then(|| value.as_f64().map(|seconds| seconds as i64)).flatten();
        let row = match time {
            Some(at) => {
                let tone = match key {
                    "exp" if at <= now => Tone::Bad,
                    "exp" => Tone::Good,
                    "nbf" if at > now => Tone::Bad,
                    _ => Tone::Plain,
                };
                Claim { label, value: date(at, offset), hint: Some(relative(at, now)), tone }
            }
            None => Claim { label, value: text(value), hint: None, tone: Tone::Plain },
        };
        rows.push(row);
    }
    let others = payload.iter().filter(|(key, _)| !REGISTERED.iter().any(|(k, _)| k == key)).map(|(k, v)| (k.clone(), v.clone())).collect();
    (rows, others)
}

/// A claim's value as text: a string as it is, a list of strings joined, anything else as JSON.
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) if items.iter().all(Value::is_string) => {
            items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")
        }
        other => other.to_string(),
    }
}

/// `3 Oct 2026, 13:30`, in the zone `offset` seconds east of UTC.
fn date(seconds: i64, offset: i32) -> String {
    let zone = FixedOffset::east_opt(offset).unwrap_or(FixedOffset::east_opt(0).expect("UTC"));
    match DateTime::from_timestamp(seconds, 0) {
        Some(at) => at.with_timezone(&zone).format("%-d %b %Y, %H:%M").to_string(),
        None => seconds.to_string(),
    }
}

/// `in 20 h 48 m`, `3 h ago`.
fn relative(at: i64, now: i64) -> String {
    let span = span(at.abs_diff(now));
    if at > now { format!("in {span}") } else { format!("{span} ago") }
}

fn span(seconds: u64) -> String {
    let (days, hours, minutes) = (seconds / 86_400, seconds % 86_400 / 3600, seconds % 3600 / 60);
    match (days, hours, minutes) {
        (0, 0, 0) => "under a minute".into(),
        (0, 0, m) => format!("{m} m"),
        (0, h, 0) => format!("{h} h"),
        (0, h, m) => format!("{h} h {m} m"),
        (d, 0, _) => format!("{d} d"),
        (d, h, _) if d < 7 => format!("{d} d {h} h"),
        (d, _, _) => format!("{d} d"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &str = r#"{"sub":"user_8812","name":"Ada","iat":1790928000,"exp":1791014400,"roles":["admin"]}"#;

    #[test]
    fn signs_decodes_and_verifies() {
        let token = sign(PAYLOAD, Algorithm::HS256, "secret").unwrap();
        let jwt = decode(&token).unwrap();
        assert_eq!(jwt.algorithm(), Some("HS256"));
        assert_eq!(jwt.payload["sub"], "user_8812");
        assert_eq!(jwt.verify("secret"), Some(true));
        assert_eq!(jwt.verify("other"), Some(false));
        let jwt = decode(&sign(PAYLOAD, Algorithm::HS512, "k").unwrap()).unwrap();
        assert_eq!((jwt.hmac(), jwt.verify("k")), (Some(Algorithm::HS512), Some(true)));
        assert!(sign("[1]", Algorithm::HS256, "k").is_err());
    }

    #[test]
    fn verifies_a_known_token() {
        // jwt.io's example, signed with "your-256-bit-secret".
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        assert_eq!(decode(token).unwrap().verify("your-256-bit-secret"), Some(true));
        assert_eq!(detect(token, None), Some((false, 0.95)));
    }

    #[test]
    fn reads_tokens_it_cant_verify_and_refuses_what_isnt_one() {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
        let jwt = decode(&format!("{header}.{}.c2ln", URL_SAFE_NO_PAD.encode("{}"))).unwrap();
        assert_eq!((jwt.hmac(), jwt.verify("k")), (None, None));
        assert!(decode("a.b").is_err());
        assert!(decode("eyJ.eyJ.x").is_err());
        assert_eq!(detect("hello.world.again", None), None);
        assert_eq!(detect(r#"{"a":1}"#, Some(Shape::Object)), Some((true, 0.3)));
    }

    #[test]
    fn claims_read_for_people() {
        let Value::Object(payload) = serde_json::from_str(PAYLOAD).unwrap() else { panic!() };
        // 3 h 12 m after it was issued, in India (UTC+5:30).
        let now = 1790928000 + 3 * 3600 + 12 * 60;
        let (rows, others) = claims(&payload, now, 5 * 3600 + 1800);
        assert_eq!(rows[0], Claim { label: "Subject", value: "user_8812".into(), hint: None, tone: Tone::Plain });
        assert_eq!(rows[1].value, "2 Oct 2026, 13:30");
        assert_eq!(rows[1].hint.as_deref(), Some("3 h 12 m ago"));
        assert_eq!((rows[2].label, rows[2].tone, rows[2].hint.as_deref()), ("Expires", Tone::Good, Some("in 20 h 48 m")));
        assert_eq!(others.keys().collect::<Vec<_>>(), ["name", "roles"]);
        let (rows, _) = claims(&payload, 1791014400 + 2 * 86_400, 0);
        assert_eq!((rows[2].tone, rows[2].hint.as_deref()), (Tone::Bad, Some("2 d ago")));
        assert_eq!(span(30), "under a minute");
        assert_eq!(span(9 * 86_400 + 3600), "9 d");
    }
}
