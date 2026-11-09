//! Profile field validation. Values are rendered as text or, for the website, as a link.

use super::{AccountProfile, ProfileRejection, ProfileUpdate};

const MAX_DISPLAY_NAME_LEN: usize = 80;
const MAX_BIO_LEN: usize = 300;
const MAX_WEBSITE_LEN: usize = 200;
const MAX_LOCATION_LEN: usize = 80;

pub(super) fn validate(input: &ProfileUpdate) -> Result<AccountProfile, ProfileRejection> {
    Ok(AccountProfile {
        display_name: single_line("display name", &input.display_name, MAX_DISPLAY_NAME_LEN)?,
        bio: optional_text("bio", input.bio.trim(), MAX_BIO_LEN)?,
        website: website(&input.website)?,
        location: single_line("location", &input.location, MAX_LOCATION_LEN)?,
    })
}

/// A trimmed single-line value; blank becomes `None`.
fn single_line(field: &'static str, value: &str, max: usize) -> Result<Option<String>, ProfileRejection> {
    let value = value.trim();
    if value.chars().any(char::is_control) {
        return Err(ProfileRejection::NotSingleLine { field });
    }
    optional_text(field, value, max)
}

/// `value` (already trimmed) as an optional field of at most `max` characters.
fn optional_text(field: &'static str, value: &str, max: usize) -> Result<Option<String>, ProfileRejection> {
    if value.chars().count() > max {
        return Err(ProfileRejection::TooLong { field, max });
    }
    if value.chars().any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t') {
        return Err(ProfileRejection::InvalidCharacters { field });
    }
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

/// An `http(s)://` URL with a host, or `None` when blank. Anything else (notably
/// `javascript:` and `data:` URLs) is rejected, because the value is rendered as a link.
fn website(value: &str) -> Result<Option<String>, ProfileRejection> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > MAX_WEBSITE_LEN {
        return Err(ProfileRejection::TooLong { field: "website", max: MAX_WEBSITE_LEN });
    }
    let lower = value.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"));
    let has_host = rest.is_some_and(|rest| rest.split(['/', '?', '#']).next().is_some_and(|host| !host.is_empty()));
    if !has_host || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(ProfileRejection::WebsiteNotHttp);
    }
    Ok(Some(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn website_accepts_http_urls_only() {
        assert_eq!(website("  ").unwrap(), None);
        assert_eq!(website(" https://example.com/me ").unwrap().as_deref(), Some("https://example.com/me"));
        assert_eq!(website("HTTP://Example.com").unwrap().as_deref(), Some("HTTP://Example.com"));
        for bad in ["example.com", "javascript:alert(1)", "https://", "https:///path", "ftp://example.com", "https://exa mple.com"] {
            assert!(website(bad).is_err(), "{bad:?} should be rejected");
        }
        assert!(website(&format!("https://{}", "a".repeat(MAX_WEBSITE_LEN))).is_err());
    }

    #[test]
    fn single_line_fields_reject_line_breaks() {
        assert_eq!(single_line("location", "  Berlin ", 80).unwrap().as_deref(), Some("Berlin"));
        assert_eq!(single_line("location", "", 80).unwrap(), None);
        assert!(single_line("location", "Ber\nlin", 80).is_err());
        assert!(single_line("location", &"é".repeat(81), 80).is_err());
        assert!(single_line("location", &"é".repeat(80), 80).is_ok());
    }

    #[test]
    fn multi_line_text_keeps_line_breaks() {
        assert_eq!(optional_text("bio", "line one\nline two", 300).unwrap().as_deref(), Some("line one\nline two"));
        assert!(optional_text("bio", "bell\u{7}", 300).is_err());
    }

    #[test]
    fn rejection_text_matches_the_legacy_messages() {
        let long = ProfileUpdate { location: "x".repeat(81), ..ProfileUpdate::default() };
        assert_eq!(validate(&long).unwrap_err().to_string(), "location must be at most 80 characters");
        let broken = ProfileUpdate { display_name: "a\nb".into(), ..ProfileUpdate::default() };
        assert_eq!(validate(&broken).unwrap_err().to_string(), "display name must be a single line of text");
        let bell = ProfileUpdate { bio: "bell\u{7}".into(), ..ProfileUpdate::default() };
        assert_eq!(validate(&bell).unwrap_err().to_string(), "bio contains invalid characters");
        let site = ProfileUpdate { website: "ftp://example.com".into(), ..ProfileUpdate::default() };
        assert_eq!(validate(&site).unwrap_err().to_string(), "website must be a URL starting with http:// or https://");
        let site = ProfileUpdate { website: format!("https://{}", "a".repeat(200)), ..ProfileUpdate::default() };
        assert_eq!(validate(&site).unwrap_err().to_string(), "website must be at most 200 characters");
    }
}
