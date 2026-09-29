//! Plugins' secrets (an API key, a sign-in's tokens), kept encrypted.
//!
//! One random 256-bit master key lives in the login Keychain. A secret is AES-256-GCM
//! ciphertext in `plugin-secrets.json` (readable only by this user), with a fresh nonce
//! each time and bound to its plugin and key, so moved or modified ciphertext fails to
//! open. The Keychain is touched only when a plugin first reads or saves a secret, and
//! the key is made only if nothing is saved yet: a new key is never made over existing
//! ciphertext.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow, ensure};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use delight_protocol::validate_id;
use gpui::{App, Global};
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom as _, SystemRandom};
use security_framework::passwords::{PasswordOptions, generic_password, set_generic_password};

use crate::files::{read_json, write_json};

const KEYCHAIN_SERVICE: &str = "dev.delight.app";
const KEYCHAIN_ACCOUNT: &str = "encryption-key-v1";
const KEY_LEN: usize = 32;
const FORMAT: &str = "v1.";
/// `errSecItemNotFound`: no such Keychain item.
const NOT_FOUND: i32 = -25300;
/// The longest a secret's name is, in bytes.
const MAX_KEY_BYTES: usize = 256;

/// Plugin id, then secret name, then ciphertext.
type Saved = BTreeMap<String, BTreeMap<String, String>>;

struct Cipher {
    key: LessSafeKey,
    random: SystemRandom,
}

impl Cipher {
    fn new(key: &[u8]) -> Result<Self> {
        ensure!(key.len() == KEY_LEN, "the Delight encryption key has the wrong length");
        let key = UnboundKey::new(&AES_256_GCM, key).map_err(|_| anyhow!("the Delight encryption key is invalid"))?;
        Ok(Self { key: LessSafeKey::new(key), random: SystemRandom::new() })
    }

    fn encrypt(&self, scope: &[u8], value: &str) -> Result<String> {
        let mut nonce = [0_u8; NONCE_LEN];
        self.random.fill(&mut nonce).map_err(|_| anyhow!("could not make an encryption nonce"))?;
        let mut sealed = value.as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(scope), &mut sealed)
            .map_err(|_| anyhow!("could not encrypt the value"))?;
        let mut encoded = nonce.to_vec();
        encoded.extend_from_slice(&sealed);
        Ok(format!("{FORMAT}{}", URL_SAFE_NO_PAD.encode(encoded)))
    }

    fn decrypt(&self, scope: &[u8], value: &str) -> Result<String> {
        let encoded = value.strip_prefix(FORMAT).context("unknown encrypted value format")?;
        let mut sealed = URL_SAFE_NO_PAD.decode(encoded).context("invalid encrypted value")?;
        ensure!(sealed.len() >= NONCE_LEN + AES_256_GCM.tag_len(), "the encrypted value is too short");
        let nonce: [u8; NONCE_LEN] = sealed[..NONCE_LEN].try_into().context("the nonce's length")?;
        let plaintext = self
            .key
            .open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(scope), &mut sealed[NONCE_LEN..])
            .map_err(|_| anyhow!("the encrypted value is invalid or belongs to another plugin"))?;
        String::from_utf8(plaintext.to_vec()).context("the decrypted value is not text")
    }
}

/// The encrypted store, a GPUI global.
pub struct Secrets {
    path: PathBuf,
    saved: Saved,
    cipher: Option<Cipher>,
}

impl Global for Secrets {}

/// Load the store from Delight's folder; the Keychain isn't touched yet.
pub fn init(cx: &mut App) {
    cx.set_global(Secrets::open(crate::app_dir().join("plugin-secrets.json")));
}

pub fn get_mut(cx: &mut App) -> &mut Secrets {
    cx.global_mut::<Secrets>()
}

/// Forget everything a deleted plugin saved.
pub fn forget_plugin(plugin_id: &str, cx: &mut App) {
    if let Err(error) = get_mut(cx).forget_plugin(plugin_id) {
        log::error!("forgetting {plugin_id}'s secrets: {error:#}");
    }
}

impl Secrets {
    fn open(path: PathBuf) -> Self {
        let saved = read_json(&path).unwrap_or_default();
        Secrets { path, saved, cipher: None }
    }

    fn cipher(&mut self, create_key: bool) -> Result<&Cipher> {
        if self.cipher.is_none() {
            self.cipher = Some(Cipher::new(&master_key(create_key)?)?);
        }
        self.cipher.as_ref().context("the cipher was just made")
    }

    /// A saved secret, or `None` when there's none.
    pub fn get(&mut self, plugin_id: &str, key: &str) -> Result<Option<String>> {
        check(plugin_id, key)?;
        let Some(value) = self.saved.get(plugin_id).and_then(|values| values.get(key)).cloned() else {
            return Ok(None);
        };
        let scope = scope(plugin_id, key);
        self.cipher(false)?.decrypt(scope.as_bytes(), &value).map(Some)
    }

    /// Encrypt and save a secret; an empty value deletes it.
    pub fn set(&mut self, plugin_id: &str, key: &str, value: &str) -> Result<()> {
        check(plugin_id, key)?;
        if value.is_empty() {
            let removed = self.saved.get_mut(plugin_id).is_some_and(|values| values.remove(key).is_some());
            if self.saved.get(plugin_id).is_some_and(BTreeMap::is_empty) {
                self.saved.remove(plugin_id);
            }
            return if removed { self.save() } else { Ok(()) };
        }
        let scope = scope(plugin_id, key);
        let create_key = self.saved.is_empty();
        let encrypted = self.cipher(create_key)?.encrypt(scope.as_bytes(), value)?;
        self.saved.entry(plugin_id.into()).or_default().insert(key.into(), encrypted);
        self.save()
    }

    /// Remove all of one plugin's secrets.
    pub fn forget_plugin(&mut self, plugin_id: &str) -> Result<()> {
        if self.saved.remove(plugin_id).is_some() { self.save() } else { Ok(()) }
    }

    fn save(&self) -> Result<()> {
        write_json(&self.path, &self.saved)?;
        let mut permissions = std::fs::metadata(&self.path)?.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(&self.path, permissions)?;
        Ok(())
    }
}

fn check(plugin_id: &str, key: &str) -> Result<()> {
    validate_id(plugin_id)?;
    ensure!(!key.is_empty() && key.len() <= MAX_KEY_BYTES, "a secret's name is 1 to {MAX_KEY_BYTES} bytes");
    Ok(())
}

/// What binds a ciphertext to its plugin and name.
fn scope(plugin_id: &str, key: &str) -> String {
    format!("delight-secret-v1\0{plugin_id}\0{key}")
}

fn master_key(create: bool) -> Result<[u8; KEY_LEN]> {
    let options = || PasswordOptions::new_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT);
    match generic_password(options()) {
        Ok(key) => key.try_into().map_err(|_| anyhow!("the Delight encryption key in the Keychain has the wrong length")),
        Err(error) if error.code() == NOT_FOUND && create => {
            let mut key = [0_u8; KEY_LEN];
            SystemRandom::new().fill(&mut key).map_err(|_| anyhow!("could not make the Delight encryption key"))?;
            set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, &key)
                .map_err(|error| anyhow!("saving the Delight encryption key in the Keychain: {error}"))?;
            Ok(key)
        }
        Err(error) if error.code() == NOT_FOUND => {
            Err(anyhow!("the Delight encryption key is missing from the Keychain: saved secrets can't be opened"))
        }
        Err(error) => Err(anyhow!("reading the Delight encryption key from the Keychain: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str, key: u8) -> Secrets {
        let path = std::env::temp_dir().join(format!("delight-secrets-{name}-{}.json", std::process::id()));
        std::fs::remove_file(&path).ok();
        let mut secrets = Secrets::open(path);
        secrets.cipher = Some(Cipher::new(&[key; KEY_LEN]).unwrap());
        secrets
    }

    #[test]
    fn a_secret_is_saved_as_ciphertext_and_read_back() {
        let mut secrets = store("saved", 7);
        secrets.set("acme.one", "token", "plain-secret").unwrap();
        let raw = std::fs::read_to_string(&secrets.path).unwrap();
        assert!(!raw.contains("plain-secret"));
        assert_eq!(std::fs::metadata(&secrets.path).unwrap().permissions().mode() & 0o777, 0o600);

        let mut reopened = Secrets::open(secrets.path.clone());
        reopened.cipher = Some(Cipher::new(&[7; KEY_LEN]).unwrap());
        assert_eq!(reopened.get("acme.one", "token").unwrap().as_deref(), Some("plain-secret"));
        assert_eq!(reopened.get("acme.one", "other").unwrap(), None);
        reopened.set("acme.one", "token", "").unwrap();
        assert_eq!(reopened.get("acme.one", "token").unwrap(), None);
        std::fs::remove_file(&reopened.path).ok();
    }

    #[test]
    fn ciphertext_is_bound_to_its_plugin_and_name() {
        let mut secrets = store("bound", 9);
        secrets.set("acme.one", "token", "x").unwrap();
        let moved = secrets.saved["acme.one"]["token"].clone();
        secrets.saved.entry("acme.two".into()).or_default().insert("token".into(), moved.clone());
        secrets.saved.get_mut("acme.one").unwrap().insert("renamed".into(), moved);
        assert!(secrets.get("acme.two", "token").is_err(), "another plugin's");
        assert!(secrets.get("acme.one", "renamed").is_err(), "another name");
        assert_eq!(secrets.get("acme.one", "token").unwrap().as_deref(), Some("x"));
        std::fs::remove_file(&secrets.path).ok();
    }

    #[test]
    fn a_nonce_is_fresh_each_time() {
        let cipher = Cipher::new(&[3; KEY_LEN]).unwrap();
        let (first, second) = (cipher.encrypt(b"s", "v").unwrap(), cipher.encrypt(b"s", "v").unwrap());
        assert_ne!(first, second);
        assert_eq!(cipher.decrypt(b"s", &first).unwrap(), "v");
        assert!(cipher.decrypt(b"other", &first).is_err());
    }

    #[test]
    fn forgetting_a_plugin_keeps_the_others() {
        let mut secrets = store("forget", 5);
        secrets.set("acme.gone", "token", "gone").unwrap();
        secrets.set("acme.kept", "token", "kept").unwrap();
        secrets.forget_plugin("acme.gone").unwrap();
        assert_eq!(secrets.get("acme.gone", "token").unwrap(), None);
        assert_eq!(secrets.get("acme.kept", "token").unwrap().as_deref(), Some("kept"));
        std::fs::remove_file(&secrets.path).ok();
    }

    #[test]
    fn names_are_checked() {
        let mut secrets = store("names", 1);
        assert!(secrets.set("acme.one", "", "x").is_err());
        assert!(secrets.set("acme.one", &"k".repeat(MAX_KEY_BYTES + 1), "x").is_err());
        assert!(secrets.set("../evil", "token", "x").is_err());
    }
}
