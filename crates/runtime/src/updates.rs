//! Getting plugins from where they are published (`delight_manifest::Release`): reading what a link
//! points at (a list of plugins, or one plugin's manifest), then downloading a plugin's `.wasm` and
//! checking it, all without running any of it. The app installs plugins from a link this way, and
//! updates an installed one from the location its manifest names. These block on the network: call
//! them off the main thread.

use std::io::Read as _;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use delight_manifest::{MAX_MANIFEST_BYTES, MAX_PLUGIN_BYTES, Manifest, manifest_url, read_manifest, read_published, wasm_url};
pub use delight_manifest::{Link, PluginList, Release, is_newer};
use sha2::{Digest as _, Sha256};

/// Longest to wait for a server to answer at all.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest a location's manifest may take, and a plugin's file.
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(30);
const PLUGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// A plugin's file, checked: it is the one the location's manifest describes.
pub struct Downloaded {
    pub wasm: Vec<u8>,
    pub manifest: Manifest,
}

/// Read the manifest of the plugin `id` at `location`: which version is published there.
pub fn fetch_release(location: &str, id: &str) -> Result<Release> {
    let url = manifest_url(location, id);
    let release = Release::parse(&get_text(&url)?).with_context(|| format!("reading {url}"))?;
    if release.id != id {
        bail!("{url} is the manifest of {}, not {id}", release.id);
    }
    Ok(release)
}

/// The plugins a link points at: every one in the list it is, or the one plugin whose manifest it
/// is. The file says which.
pub fn fetch_link(link: &Link) -> Result<Vec<Release>> {
    read_published(&get_text(&link.url)?).with_context(|| format!("reading {}", link.url))
}

/// GET a manifest or a list: text, at most [`MAX_MANIFEST_BYTES`].
fn get_text(url: &str) -> Result<String> {
    let bytes = get(url, MAX_MANIFEST_BYTES, MANIFEST_TIMEOUT, &mut |_, _| {})?;
    String::from_utf8(bytes).with_context(|| format!("{url} isn't text"))
}

/// Download the plugin `release` describes from `location`, and check it ([`check`]).
pub fn download(location: &str, release: &Release, installed: Option<&Manifest>) -> Result<Downloaded> {
    download_with_progress(location, release, installed, |_, _| {})
}

/// [`download`], telling `progress` how many bytes have come so far, and how many there are when
/// the server says.
pub fn download_with_progress(
    location: &str,
    release: &Release,
    installed: Option<&Manifest>,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<Downloaded> {
    let wasm = get(&wasm_url(location, &release.id), MAX_PLUGIN_BYTES, PLUGIN_TIMEOUT, &mut progress)?;
    check(release, installed, wasm)
}

/// Whether `wasm` is the plugin `release` describes: its SHA-256 is the manifest's, and its own
/// manifest (read without running it, on a protocol this app runs) has the id and version the
/// location's manifest said. To update `installed`, it must also be that plugin, in a newer version.
pub fn check(release: &Release, installed: Option<&Manifest>, wasm: Vec<u8>) -> Result<Downloaded> {
    let sha256 = hex(&Sha256::digest(&wasm));
    if sha256 != release.sha256 {
        bail!("the downloaded file's SHA-256 is {sha256}, not the manifest's {}: refused", release.sha256);
    }
    let manifest = read_manifest(&wasm).context("the downloaded file")?;
    let new = &manifest.plugin;
    if new.id != release.id || new.version != release.version {
        bail!("the downloaded file is {} {}, but the manifest says {} {}", new.id, new.version, release.id, release.version);
    }
    if let Some(installed) = installed.map(|installed| &installed.plugin) {
        if new.id != installed.id {
            bail!("the downloaded file is the plugin {}, not {}", new.id, installed.id);
        }
        if !is_newer(&new.version, &installed.version) {
            bail!("the downloaded file is version {}, not newer than the installed {}", new.version, installed.version);
        }
    }
    Ok(Downloaded { wasm, manifest })
}

/// Whether `new` asks for anything `installed` wasn't granted: a permission it didn't have, or
/// the same permission with other data (other programs to run). A reason that changed asks for
/// nothing more.
pub fn asks_for_more(installed: &Manifest, new: &Manifest) -> bool {
    let granted = &installed.plugin.permissions;
    new.plugin.permissions.iter().any(|request| !granted.iter().any(|had| had.permission == request.permission))
}

/// GET `url`, at most `limit` bytes, within `timeout`, telling `progress` the bytes so far and the
/// size the server gives. Certificates are checked by macOS, as the rest of Delight's network does,
/// so a company's TLS proxy works.
fn get(url: &str, limit: u64, timeout: Duration, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<Vec<u8>> {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(timeout))
        .build()
        .into();
    let response = agent.get(url).call().with_context(|| format!("fetching {url}"))?;
    let total = response.headers().get("content-length").and_then(|value| value.to_str().ok()?.parse().ok());
    let mut body = response.into_body().into_with_config().limit(limit).reader();
    let (mut bytes, mut chunk) = (Vec::new(), vec![0; 64 << 10]);
    loop {
        let read = body.read(&mut chunk).with_context(|| format!("reading {url} (at most {} KB)", limit >> 10))?;
        if read == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&chunk[..read]);
        progress(bytes.len() as u64, total);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::net::TcpListener;

    use delight_manifest::{Operation, Permission, PermissionRequest, PluginProperties, SECTION, encode_operations, encode_properties};
    use wasm_encoder::{CustomSection, Module};

    use super::*;

    fn manifest(id: &str, version: &str, permissions: Vec<Permission>) -> Manifest {
        Manifest {
            plugin: PluginProperties {
                id: id.into(),
                name: "Logs".into(),
                version: version.into(),
                description: String::new(),
                author: String::new(),
                icon: "<svg/>".into(),
                tags: Vec::new(),
                permissions: permissions.into_iter().map(|permission| PermissionRequest { permission, reason: "Why".into() }).collect(),
                tips: Vec::new(),
                update: Some("http://127.0.0.1/plugins".into()),
            },
            operations: vec![Operation { id: "search".into(), title: "Search".into(), description: String::new(), icon: None, tags: Vec::new() }],
        }
    }

    fn logs(version: &str) -> Manifest {
        manifest("com.example.logs", version, Vec::new())
    }

    /// A `.wasm` carrying `manifest`, as a plugin's does.
    fn wasm(manifest: &Manifest) -> Vec<u8> {
        let mut section = encode_properties(&manifest.plugin);
        section.extend(encode_operations(&manifest.operations));
        let mut module = Module::new();
        module.section(&CustomSection { name: Cow::Borrowed(SECTION), data: Cow::Borrowed(&section) });
        module.finish()
    }

    /// What a location's manifest says about `wasm`.
    fn release(manifest: &Manifest, wasm: &[u8]) -> Release {
        Release {
            id: manifest.plugin.id.clone(),
            version: manifest.plugin.version.clone(),
            name: manifest.plugin.name.clone(),
            description: String::new(),
            sha256: hex(&Sha256::digest(wasm)),
        }
    }

    /// Serve each of `bodies` once, in order, to whatever asks: `(status, body)`. The URL of the
    /// server.
    fn serve(bodies: Vec<(u16, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for (status, body) in bodies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 2 {
                    line.clear();
                }
                write!(stream, "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        url
    }

    #[test]
    fn a_newer_version_is_found_downloaded_and_checked() {
        let installed = logs("1.3.0");
        let newer = logs("1.4.0");
        let file = wasm(&newer);
        let location = serve(vec![(200, release(&newer, &file).to_xml().unwrap().into_bytes()), (200, file.clone())]);

        let found = fetch_release(&location, "com.example.logs").unwrap();
        assert!(found.newer_than(&installed.plugin));
        let mut told = Vec::new();
        let downloaded = download_with_progress(&location, &found, Some(&installed), |done, total| told.push((done, total))).unwrap();
        assert_eq!(told.last(), Some(&(file.len() as u64, Some(file.len() as u64))), "the progress ends at the whole file");
        assert_eq!(downloaded.manifest, newer);
        assert_eq!(downloaded.wasm, file);
    }

    #[test]
    fn a_link_gives_every_plugin_in_its_list_or_its_one_plugin_whatever_the_files_name() {
        let (logs_file, process_file) = (wasm(&logs("1.0.0")), wasm(&manifest("local.process", "2.0.0", Vec::new())));
        let list = PluginList {
            plugins: vec![release(&logs("1.0.0"), &logs_file), release(&manifest("local.process", "2.0.0", Vec::new()), &process_file)],
        };
        let location = serve(vec![(200, list.to_xml().unwrap().into_bytes())]);
        let link = Link::parse(&format!("{location}/team-tools.xml")).unwrap();
        assert_eq!(link.location, location);
        assert_eq!(fetch_link(&link).unwrap(), list.plugins);
        let location = serve(vec![(200, release(&logs("1.0.0"), &logs_file).to_xml().unwrap().into_bytes())]);
        let one = fetch_link(&Link::parse(&format!("{location}/plugins.xml")).unwrap()).unwrap();
        assert_eq!(one.iter().map(|release| release.id.as_str()).collect::<Vec<_>>(), ["com.example.logs"]);
        let location = serve(vec![(200, b"<html>Not found</html>".to_vec())]);
        let error = format!("{:#}", fetch_link(&Link::parse(&format!("{location}/plugins.xml")).unwrap()).unwrap_err());
        assert!(error.contains("/plugins.xml") && error.contains("<html>"), "{error}");
    }

    #[test]
    fn a_plugin_is_installed_from_a_location_with_nothing_installed() {
        let file = wasm(&logs("1.0.0"));
        let found = release(&logs("1.0.0"), &file);
        assert_eq!(check(&found, None, file).unwrap().manifest.plugin.id, "com.example.logs");
    }

    #[test]
    fn a_missing_manifest_is_an_error_saying_where() {
        let location = serve(vec![(404, b"not here".to_vec())]);
        let error = format!("{:#}", fetch_release(&location, "com.example.logs").unwrap_err());
        assert!(error.contains("/com.example.logs.xml") && error.contains("404"), "{error}");
        let file = wasm(&logs("1.0.0"));
        let location = serve(vec![(200, release(&logs("1.0.0"), &file).to_xml().unwrap().into_bytes())]);
        let error = format!("{:#}", fetch_release(&location, "com.example.other").unwrap_err());
        assert!(error.contains("not com.example.other"), "another plugin's manifest under this name: {error}");
    }

    #[test]
    fn a_file_that_isnt_what_the_manifest_says_is_refused() {
        let installed = logs("1.3.0");
        let file = wasm(&logs("1.4.0"));
        let refused = |release: Release, file: &[u8]| format!("{:#}", check(&release, Some(&installed), file.to_vec()).err().expect("refused"));

        let mut tampered = file.clone();
        tampered.push(0);
        assert!(refused(release(&logs("1.4.0"), &file), &tampered).contains("SHA-256"));
        assert!(refused(Release { version: "1.5.0".into(), ..release(&logs("1.4.0"), &file) }, &file).contains("manifest says"));
        let other = manifest("com.example.other", "1.4.0", Vec::new());
        let other_file = wasm(&other);
        assert!(refused(release(&other, &other_file), &other_file).contains("not com.example.logs"));
        let older = wasm(&logs("1.2.0"));
        assert!(refused(release(&logs("1.2.0"), &older), &older).contains("not newer"));
        let text = Release { sha256: hex(&Sha256::digest(b"text")), ..release(&logs("1.4.0"), &file) };
        assert!(refused(text, b"text").contains("downloaded file"));
    }

    #[test]
    fn an_update_asks_for_more_only_with_a_new_permission_or_other_programs() {
        let id = "com.example.logs";
        let installed = manifest(id, "1.0.0", vec![Permission::network(), Permission::commands(["/bin/ps"])]);
        assert!(!asks_for_more(&installed, &manifest(id, "1.1.0", vec![Permission::network()])), "fewer");
        let mut reworded = installed.clone();
        reworded.plugin.permissions[0].reason = "Another reason".into();
        assert!(!asks_for_more(&installed, &reworded), "a new reason");
        assert!(asks_for_more(&manifest(id, "1.0.0", vec![Permission::network()]), &installed), "a new permission");
        let more_programs = manifest(id, "1.1.0", vec![Permission::network(), Permission::commands(["/bin/ps", "/bin/kill"])]);
        assert!(asks_for_more(&installed, &more_programs), "other programs");
    }
}
