//! TEMPORARY(open_url): opening a web page in the browser for a plugin (to sign in,
//! say), until embedded_gpui forwards GPUI's own `cx.open_url` from plugins (today its
//! plugin platform drops it). README, "Temporary host APIs".

use anyhow::{Result, bail};

/// `url` if it's a web page a plugin may open: `http` or `https`, with somewhere to
/// go. Nothing else (`file:`, other apps' own schemes), so a plugin can't start
/// things on the Mac through it.
pub fn web_page(url: &str) -> Result<&str> {
    let lower = url.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")) else {
        bail!("{url:?} isn't a web page: a plugin opens only http and https URLs");
    };
    if rest.is_empty() || rest.starts_with('/') {
        bail!("{url:?} has no host");
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        bail!("{url:?} has spaces or control characters");
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_pages_open() {
        assert!(web_page("https://accounts.google.com/o/oauth2/v2/auth?client_id=1").is_ok());
        assert!(web_page("HTTP://localhost:8080/").is_ok());
        assert!(web_page("file:///etc/passwd").is_err());
        assert!(web_page("x-apple.systempreferences:com.apple.preference").is_err());
        assert!(web_page("https://").is_err());
        assert!(web_page("https:///path").is_err());
        assert!(web_page("https://example.com/a b").is_err());
    }
}
