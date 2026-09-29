//! Downloading a pinned file: its SHA-256 is known in advance, and anything else is refused.

use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::Path;

use anyhow::{Result, bail};
use sha2::{Digest as _, Sha256};

/// Download `url` to `file`, showing progress, and check its SHA-256 is `sha256`; the file is
/// removed if it isn't. Certificates are checked by macOS, so a company proxy's CA works.
pub fn fetch(url: &str, file: &Path, sha256: &str) -> Result<()> {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(tls).build().into();
    let response = agent.get(url).call()?;
    let total: Option<u64> =
        response.headers().get("content-length").and_then(|value| value.to_str().ok()).and_then(|value| value.parse().ok());
    let mut body = response.into_body().into_reader();
    let mut out = File::create(file)?;
    let (mut hasher, mut buffer, mut done, mut shown) = (Sha256::new(), vec![0; 1 << 20], 0_u64, 0_u64);
    loop {
        let read = body.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        out.write_all(&buffer[..read])?;
        done += read as u64;
        if done - shown >= 20 << 20 {
            shown = done;
            match total {
                Some(total) => print!("\rDownloading {} of {} MB", done >> 20, total >> 20),
                None => print!("\rDownloading {} MB", done >> 20),
            }
            std::io::stdout().flush().ok();
        }
    }
    println!("\rDownloaded {} MB          ", done >> 20);
    let found = hex(&hasher.finalize());
    if found != sha256 {
        std::fs::remove_file(file).ok();
        bail!("the download's SHA-256 is {found}, not the pinned {sha256}: refusing it");
    }
    Ok(())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_are_lowercase_hex() {
        assert_eq!(hex(&[0x00, 0xab, 0x0f]), "00ab0f");
        assert_eq!(hex(&Sha256::digest(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
