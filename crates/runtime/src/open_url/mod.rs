//! TEMPORARY(open_url): opening a URL for a plugin, until embedded_gpui forwards
//! GPUI's own `cx.open_url` from plugins (today its plugin platform drops it).
//! docs/development.md, "Temporary host APIs".

use anyhow::{Result, bail};

/// `url` if a plugin may open it: any URL macOS can hand to an app (a web page to
/// the browser, `mailto:` to the mail app, another app's own link such as `zoommtg:`),
/// but not `file:`, which opens files (and apps) on the Mac directly.
pub fn openable(url: &str) -> Result<&str> {
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        bail!("{url:?} has spaces or control characters");
    }
    let Some((scheme, rest)) = url.split_once(':') else {
        bail!("{url:?} has no scheme, such as https:");
    };
    let valid = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid {
        bail!("{url:?} has no scheme, such as https:");
    }
    let scheme = scheme.to_ascii_lowercase();
    if scheme == "file" {
        bail!("{url:?} is a file: a plugin can't open files");
    }
    if rest.is_empty() || rest.starts_with('?') {
        bail!("{url:?} says where to go only by its scheme");
    }
    if matches!(scheme.as_str(), "http" | "https") {
        let host = rest.strip_prefix("//").unwrap_or("");
        if host.is_empty() || host.starts_with('/') {
            bail!("{url:?} has no host");
        }
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_apps_url_opens_but_not_a_file() {
        assert!(openable("https://accounts.google.com/o/oauth2/v2/auth?client_id=1").is_ok());
        assert!(openable("HTTP://localhost:8080/").is_ok());
        assert!(openable("mailto:someone@example.com?subject=Hi").is_ok());
        assert!(openable("zoommtg://zoom.us/join?confno=123").is_ok());
        assert!(openable("x-apple.systempreferences:com.apple.preference.security").is_ok());
        assert!(openable("file:///etc/passwd").is_err());
        assert!(openable("FILE:///Applications/Calculator.app").is_err());
        assert!(openable("example.com").is_err(), "no scheme");
        assert!(openable("mailto:").is_err());
        assert!(openable("https://").is_err());
        assert!(openable("https:///path").is_err());
        assert!(openable("https://example.com/a b").is_err());
    }
}
