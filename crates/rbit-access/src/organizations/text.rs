//! Organisation profile text rules.

pub(crate) const MAX_DISPLAY_NAME_LEN: usize = 100;
pub(crate) const MAX_DESCRIPTION_LEN: usize = 350;
const MAX_WEBSITE_LEN: usize = 255;

/// Trims `value`; empty becomes `None`, too long is an error naming `field`.
pub(crate) fn optional_text(value: &str, field: &str, max: usize) -> Result<Option<String>, String> {
    let value = value.trim();
    if value.chars().count() > max {
        return Err(format!("{field} must be at most {max} characters"));
    }
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

/// An optional `http(s)://` URL. Other schemes (`javascript:`, `data:`) are refused, so
/// the value is always safe to render as a link.
pub(crate) fn website(value: &str) -> Result<Option<String>, String> {
    let Some(url) = optional_text(value, "website", MAX_WEBSITE_LEN)? else {
        return Ok(None);
    };
    let lower = url.to_ascii_lowercase();
    let host = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"));
    let plausible = host.is_some_and(|rest| {
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        !host.is_empty() && !host.starts_with('.')
    }) && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if plausible { Ok(Some(url)) } else { Err("website must be a link starting with https:// or http://".to_owned()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websites_must_be_http_links() {
        assert_eq!(website("").unwrap(), None);
        assert_eq!(website("  https://acme.example/about ").unwrap().as_deref(), Some("https://acme.example/about"));
        assert!(website("HTTP://acme.example").is_ok());
        for bad in ["acme.example", "javascript:alert(1)", "https://", "https:///path", "https://a b.example", "ftp://acme.example"] {
            assert!(website(bad).is_err(), "{bad:?}");
        }
        assert!(website(&format!("https://{}.example", "a".repeat(250))).is_err());
    }

    #[test]
    fn optional_text_trims_and_limits() {
        assert_eq!(optional_text("   ", "x", 3).unwrap(), None);
        assert_eq!(optional_text(" é é ", "x", 3).unwrap().as_deref(), Some("é é"));
        assert!(optional_text("abcd", "x", 3).is_err());
    }
}
