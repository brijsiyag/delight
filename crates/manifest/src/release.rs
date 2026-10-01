//! Where plugins are published: a location (a URL, such as a GitHub release's download address)
//! holding, for each plugin, `<id>.wasm`, the plugin, and `<id>.xml`, its manifest there: which
//! plugin and version the file is, what it is called and does, and the file's SHA-256. Beside them,
//! a list ([`PluginList`]) may name several plugins there, in the same form, under any file name. A
//! plugin names its location ([`PluginProperties::update`]) and updates from its own manifest there;
//! people install from a link to a list or to one plugin's manifest ([`Link`]), and which one it is
//! is read from the file ([`read_published`]), not from its name.
//!
//! ```xml
//! <plugin id="local.logs" version="0.0.3">
//!   <name>Logs</name>
//!   <description>Search logs with KQL on your Elasticsearch hosts.</description>
//!   <sha256>…64 hex digits…</sha256>
//! </plugin>
//! ```

use std::collections::HashSet;

use anyhow::{Context as _, Result, bail};
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

use crate::{PluginProperties, validate_id, validate_url};

/// The most a manifest or a list at a location is: text about a few plugins.
pub const MAX_MANIFEST_BYTES: u64 = 1 << 20;
/// The most a plugin's file is.
pub const MAX_PLUGIN_BYTES: u64 = 64 << 20;

/// The URL of the manifest of the plugin `id` at `location`.
pub fn manifest_url(location: &str, id: &str) -> String {
    format!("{}/{id}.xml", location.trim_end_matches('/'))
}

/// The URL of the file of the plugin `id` at `location`.
pub fn wasm_url(location: &str, id: &str) -> String {
    format!("{}/{id}.wasm", location.trim_end_matches('/'))
}

/// A link someone installs from: an XML file at a location, a list of plugins there or one plugin's
/// manifest. Which it is, the file says ([`read_published`]); its name can be anything.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub url: String,
    /// The folder the file is in, where the plugins' `<id>.wasm` files are.
    pub location: String,
}

impl Link {
    /// Read a link as someone pastes it.
    pub fn parse(link: &str) -> Result<Link> {
        let url = link.trim();
        validate_url(url)?;
        let (location, file) = url
            .rsplit_once('/')
            .filter(|(location, file)| !location.ends_with('/') && !file.is_empty())
            .with_context(|| format!("{url} isn't a link to a file"))?;
        if file.ends_with(".wasm") {
            bail!("{url} is a plugin's file: paste the link to its .xml manifest, or to a list of plugins");
        }
        validate_url(location)?;
        Ok(Link { url: url.to_string(), location: location.to_string() })
    }
}

/// The plugins an XML file at a location describes: every one in a list of plugins (`<plugins>`),
/// or the one whose manifest it is (`<plugin>`).
pub fn read_published(xml: &str) -> Result<Vec<Release>> {
    match root_element(xml)?.as_str() {
        "plugins" => Ok(PluginList::parse(xml)?.plugins),
        "plugin" => Ok(vec![Release::parse(xml)?]),
        other => bail!("it is <{other}>, not a list of plugins (<plugins>) or a plugin's manifest (<plugin>)"),
    }
}

/// The name of the first element in `xml`.
fn root_element(xml: &str) -> Result<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        match reader.read_event().context("it isn't XML")? {
            Event::Start(element) | Event::Empty(element) => return Ok(element.name().0.to_string()),
            Event::Eof => bail!("it isn't XML"),
            _ => {}
        }
    }
}

/// A plugin published at a location, as its manifest there says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename = "plugin")]
pub struct Release {
    #[serde(rename = "@id")]
    pub id: String,
    /// SemVer, as the plugin's own manifest has it.
    #[serde(rename = "@version")]
    pub version: String,
    /// What it's called and what it does, from its manifest: shown when picking what to install.
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The `.wasm`'s SHA-256, lowercase hex: a download that doesn't match is refused.
    pub sha256: String,
}

impl Release {
    /// Read a plugin's manifest at its location, and check it.
    pub fn parse(xml: &str) -> Result<Release> {
        let release: Release = quick_xml::de::from_str(xml).context("it isn't a plugin's manifest")?;
        release.validate()?;
        Ok(release)
    }

    /// The manifest as XML, one element per line.
    pub fn to_xml(&self) -> Result<String> {
        self.validate()?;
        to_xml(self)
    }

    pub fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        semver::Version::parse(&self.version).with_context(|| format!("version {:?} isn't SemVer", self.version))?;
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')) {
            bail!("the SHA-256 {:?} isn't 64 lowercase hex digits", self.sha256);
        }
        Ok(())
    }

    /// The name to show: its own, or its id.
    pub fn title(&self) -> &str {
        if self.name.trim().is_empty() { &self.id } else { &self.name }
    }

    /// Whether this is `installed`'s plugin, in a later version.
    pub fn newer_than(&self, installed: &PluginProperties) -> bool {
        self.id == installed.id && is_newer(&self.version, &installed.version)
    }
}

/// Several plugins at a location, each as its own manifest says it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename = "plugins")]
pub struct PluginList {
    #[serde(rename = "plugin", default)]
    pub plugins: Vec<Release>,
}

impl PluginList {
    /// Read a location's list, and check it.
    pub fn parse(xml: &str) -> Result<PluginList> {
        let list: PluginList = quick_xml::de::from_str(xml).context("it isn't a list of plugins")?;
        list.validate()?;
        Ok(list)
    }

    /// The list as XML, one element per line.
    pub fn to_xml(&self) -> Result<String> {
        self.validate()?;
        to_xml(self)
    }

    /// Each plugin is valid, and none is listed twice.
    pub fn validate(&self) -> Result<()> {
        let mut ids = HashSet::new();
        for release in &self.plugins {
            release.validate().with_context(|| format!("the list's entry for {:?}", release.id))?;
            if !ids.insert(release.id.as_str()) {
                bail!("the list has {} twice", release.id);
            }
        }
        Ok(())
    }

    /// The list with `release` in it: in place of its plugin's entry, or added; sorted by id.
    pub fn with(mut self, release: Release) -> PluginList {
        self.plugins.retain(|listed| listed.id != release.id);
        self.plugins.push(release);
        self.plugins.sort_by(|a, b| a.id.cmp(&b.id));
        self
    }
}

fn to_xml(value: &impl Serialize) -> Result<String> {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let mut serializer = quick_xml::se::Serializer::new(&mut xml);
    serializer.indent(' ', 2);
    value.serialize(serializer).context("writing the XML")?;
    xml.push('\n');
    Ok(xml)
}

/// Whether `candidate` is a later version than `installed`, by SemVer's order. A version that
/// isn't SemVer is never newer, nor older.
pub fn is_newer(candidate: &str, installed: &str) -> bool {
    match (semver::Version::parse(candidate), semver::Version::parse(installed)) {
        (Ok(candidate), Ok(installed)) => candidate > installed,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCATION: &str = "https://github.com/Meesho/delight-plugins/releases/latest/download";

    fn release(id: &str, version: &str) -> Release {
        Release {
            id: id.into(),
            version: version.into(),
            name: "Logs".into(),
            description: "Search logs with KQL on your Elasticsearch hosts.".into(),
            sha256: "ab".repeat(32),
        }
    }

    fn installed(id: &str, version: &str) -> PluginProperties {
        let mut properties = crate::manifest::sample().plugin;
        properties.id = id.into();
        properties.version = version.into();
        properties
    }

    #[test]
    fn a_manifest_round_trips_through_xml() {
        let xml = release("local.logs", "1.4.0").to_xml().unwrap();
        assert!(xml.starts_with("<?xml"), "{xml}");
        assert!(xml.contains(r#"<plugin id="local.logs" version="1.4.0">"#), "{xml}");
        assert!(xml.contains("<name>Logs</name>"), "{xml}");
        assert_eq!(Release::parse(&xml).unwrap(), release("local.logs", "1.4.0"));
        let bare = r#"<plugin id="local.logs" version="1.4.0"><sha256>0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef</sha256></plugin>"#;
        let bare = Release::parse(bare).unwrap();
        assert_eq!((bare.name.as_str(), bare.title()), ("", "local.logs"), "the name is optional: the id shows instead");
    }

    #[test]
    fn a_list_round_trips_and_takes_a_newer_entry_in_place_of_the_old() {
        let list = PluginList { plugins: vec![release("local.logs", "1.0.0"), release("local.process", "1.0.0")] };
        let xml = list.to_xml().unwrap();
        assert!(xml.contains("<plugins>"), "{xml}");
        assert_eq!(PluginList::parse(&xml).unwrap(), list);
        assert_eq!(PluginList::parse("<plugins/>").unwrap(), PluginList::default(), "an empty list");
        let updated = list.with(release("local.logs", "1.1.0")).with(release("local.calendar", "1.0.0"));
        let ids: Vec<(&str, &str)> = updated.plugins.iter().map(|release| (release.id.as_str(), release.version.as_str())).collect();
        assert_eq!(ids, [("local.calendar", "1.0.0"), ("local.logs", "1.1.0"), ("local.process", "1.0.0")]);
        let twice = PluginList { plugins: vec![release("local.logs", "1.0.0"), release("local.logs", "1.1.0")] };
        assert!(twice.validate().unwrap_err().to_string().contains("twice"));
    }

    #[test]
    fn a_bad_manifest_is_refused() {
        let bad = |change: fn(&mut Release)| {
            let mut release = release("local.logs", "1.4.0");
            change(&mut release);
            release.validate().is_err()
        };
        assert!(bad(|release| release.version = "1.4".into()), "not SemVer");
        assert!(bad(|release| release.sha256 = "AB".repeat(32)), "uppercase");
        assert!(bad(|release| release.sha256 = "ab".into()), "short");
        assert!(bad(|release| release.id = "../escape".into()));
        assert!(Release::parse("<plugin/>").is_err(), "fields missing");
        assert!(Release::parse("not xml").is_err());
    }

    #[test]
    fn only_a_later_version_of_the_same_plugin_is_newer() {
        let release = release("local.logs", "1.4.0");
        assert!(release.newer_than(&installed("local.logs", "1.3.9")));
        assert!(!release.newer_than(&installed("local.logs", "1.4.0")), "the same");
        assert!(!release.newer_than(&installed("local.logs", "1.10.0")), "numbers, not text");
        assert!(!release.newer_than(&installed("local.other", "0.1.0")), "another plugin");
        assert!(is_newer("1.0.0", "1.0.0-beta.2"), "a release is later than its pre-release");
        assert!(!is_newer("2.0.0", "not a version"));
    }

    #[test]
    fn a_plugins_files_are_named_by_its_id_at_the_location() {
        assert_eq!(wasm_url(LOCATION, "local.logs"), format!("{LOCATION}/local.logs.wasm"));
        assert_eq!(manifest_url(&format!("{LOCATION}/"), "local.logs"), format!("{LOCATION}/local.logs.xml"));
    }

    #[test]
    fn a_link_is_a_file_at_a_location_whatever_its_name() {
        let link = Link::parse(&format!(" {LOCATION}/anything.xml ")).unwrap();
        assert_eq!(link, Link { url: format!("{LOCATION}/anything.xml"), location: LOCATION.into() }, "spaces around it");
        assert!(Link::parse(LOCATION).is_ok(), "the file is the last part, whatever it is called");
        assert!(Link::parse(&format!("{LOCATION}/local.logs.wasm")).unwrap_err().to_string().contains(".xml manifest"), "the plugin itself");
        assert!(Link::parse(&format!("{LOCATION}/")).is_err(), "no file");
        assert!(Link::parse("https://example.com").is_err(), "no file");
        assert!(Link::parse("file:///tmp/plugins.xml").is_err());
        assert!(Link::parse("https:///plugins.xml").is_err(), "no host");
    }

    #[test]
    fn what_a_file_holds_is_read_from_it_not_from_its_name() {
        let list = PluginList { plugins: vec![release("local.logs", "1.0.0"), release("local.process", "1.0.0")] };
        assert_eq!(read_published(&list.to_xml().unwrap()).unwrap(), list.plugins, "a list");
        assert_eq!(read_published(&release("local.logs", "1.0.0").to_xml().unwrap()).unwrap(), [release("local.logs", "1.0.0")], "one plugin");
        assert_eq!(read_published("<plugins/>").unwrap(), [], "an empty list");
        let error = format!("{:#}", read_published("<feed><plugin/></feed>").unwrap_err());
        assert!(error.contains("<feed>"), "{error}");
        assert!(read_published("not xml").is_err());
        assert!(read_published("").is_err());
        assert!(read_published(r#"<plugin id="local.logs"/>"#).is_err(), "a manifest missing its fields");
    }
}
