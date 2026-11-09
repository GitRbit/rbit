//! The wire contract of one webhook delivery: its signature, headers, delivery id shape
//! and how much of a response is kept. Receivers depend on every byte of it.
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// Response bytes kept in the delivery log.
pub const MAX_RESPONSE_BODY: usize = 64 * 1024;
/// Response headers kept in the delivery log.
pub const MAX_RESPONSE_HEADERS: usize = 64;

/// Sent as `User-Agent`. Shaped like GitHub's `GitHub-Hookshot/<id>`: the product name as it is
/// written (rbit, lower case) and `Hookshot`. Receivers may match on it, so it stays as
/// released rather than being lower-cased.
pub const USER_AGENT: &str = concat!("rbit-Hookshot/", env!("CARGO_PKG_VERSION"));
/// The `User-Agent` when the server does not report its version.
pub const ANONYMOUS_USER_AGENT: &str = "rbit-Hookshot";

/// The `User-Agent` deliveries send.
pub const fn user_agent(report_version: bool) -> &'static str {
    if report_version { USER_AGENT } else { ANONYMOUS_USER_AGENT }
}

/// The lower-case hex HMAC-SHA256 of `body` keyed with `secret`, as sent in
/// `X-Rbit-Signature-256: sha256=<hex>` (and GitHub's `X-Hub-Signature-256`).
pub fn signature(secret: &str, body: &[u8]) -> String {
    // HMAC accepts keys of any length; this cannot fail.
    let mut mac =
        <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).unwrap_or_else(|_| unreachable!("HMAC takes any key length"));
    mac.update(body);
    hex(&mac.finalize().into_bytes())
}

/// Lower-case hexadecimal.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

/// A delivery id (`X-Rbit-Delivery`) from 16 random bytes, formatted like a UUID v4.
pub fn format_guid(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex(&bytes);
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// The headers of a delivery request, in the order they are sent. Header names use HTTP's
/// usual Title-Case (`X-Rbit-Event`, like `X-GitHub-Event`); that is header style, not a
/// spelling of the project's name, and receivers already read these names.
pub fn request_headers(event: &str, guid: &str, hook_id: i64, signature: Option<&str>, report_version: bool) -> Vec<(String, String)> {
    let mut headers = vec![
        ("Content-Type".to_owned(), "application/json".to_owned()),
        ("User-Agent".to_owned(), user_agent(report_version).to_owned()),
        ("X-Rbit-Event".to_owned(), event.to_owned()),
        ("X-Rbit-Delivery".to_owned(), guid.to_owned()),
        ("X-Rbit-Hook-Id".to_owned(), hook_id.to_string()),
        ("X-GitHub-Event".to_owned(), event.to_owned()),
        ("X-GitHub-Delivery".to_owned(), guid.to_owned()),
        ("X-GitHub-Hook-ID".to_owned(), hook_id.to_string()),
    ];
    if let Some(signature) = signature {
        headers.push(("X-Rbit-Signature-256".to_owned(), format!("sha256={signature}")));
        headers.push(("X-Hub-Signature-256".to_owned(), format!("sha256={signature}")));
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_like_github() {
        // The example from GitHub's "Validating webhook deliveries" documentation.
        assert_eq!(
            signature("It's a Secret to Everybody", b"Hello, World!"),
            "757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17"
        );
        assert_eq!(signature("", b"").len(), 64);
    }

    #[test]
    fn signature_headers_only_with_a_secret() {
        let unsigned = request_headers("push", "g", 3, None, true);
        assert!(unsigned.iter().all(|(name, _)| !name.contains("Signature")));
        assert!(unsigned.contains(&("X-GitHub-Event".to_owned(), "push".to_owned())));
        assert!(unsigned.contains(&("X-Rbit-Delivery".to_owned(), "g".to_owned())));
        let signed = request_headers("push", "g", 3, Some("abc"), true);
        assert!(signed.contains(&("X-Rbit-Signature-256".to_owned(), "sha256=abc".to_owned())));
        assert!(signed.contains(&("X-Hub-Signature-256".to_owned(), "sha256=abc".to_owned())));
        assert!(USER_AGENT.starts_with("rbit-Hookshot/"));
        assert_eq!(request_headers("push", "g", 3, None, false)[1].1, "rbit-Hookshot");
    }

    #[test]
    fn guids_have_the_version_and_variant_bits() {
        let guid = format_guid([0xff; 16]);
        assert_eq!(guid, "ffffffff-ffff-4fff-bfff-ffffffffffff");
        assert_eq!(format_guid([0; 16]), "00000000-0000-4000-8000-000000000000");
    }
}
