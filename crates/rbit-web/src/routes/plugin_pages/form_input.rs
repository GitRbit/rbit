//! Strict browser form transport; no raw input reaches compiled plugin callbacks.

use axum::http::{HeaderMap, header};

pub(super) const MAX_ENCODED_FORM_BYTES: usize = 32 * 1024;

pub(super) struct ParsedForm {
    pub proof: String,
    pub fields: Vec<(String, String)>,
}
impl std::fmt::Debug for ParsedForm {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ParsedForm").field("proof", &"<redacted>").field("field_count", &self.fields.len()).finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputError {
    Malformed,
    MissingProof,
    TooLarge,
}
impl InputError {
    pub fn status(self) -> axum::http::StatusCode {
        match self {
            Self::Malformed => axum::http::StatusCode::BAD_REQUEST,
            Self::MissingProof => axum::http::StatusCode::FORBIDDEN,
            Self::TooLarge => axum::http::StatusCode::PAYLOAD_TOO_LARGE,
        }
    }
}

pub(super) fn check_length(headers: &HeaderMap) -> Result<(), InputError> {
    if headers.get_all(header::CONTENT_LENGTH).iter().count() > 1 {
        return Err(InputError::Malformed);
    }
    if let Some(length) = headers.get(header::CONTENT_LENGTH) {
        let length = length.to_str().map_err(|_| InputError::Malformed)?;
        if length.is_empty() || !length.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(InputError::Malformed);
        }
        let length = length.parse::<usize>().map_err(|_| InputError::TooLarge)?;
        if length > MAX_ENCODED_FORM_BYTES {
            return Err(InputError::TooLarge);
        }
    }
    Ok(())
}

impl ParsedForm {
    pub fn decode(body: &[u8]) -> Result<Self, InputError> {
        if body.len() > MAX_ENCODED_FORM_BYTES {
            return Err(InputError::TooLarge);
        }
        let body = std::str::from_utf8(body).map_err(|_| InputError::Malformed)?;
        let mut fields = Vec::new();
        let mut proof = None;
        let mut names = std::collections::BTreeSet::new();
        for pair in body.split('&') {
            if names.len() >= 33 {
                return Err(InputError::TooLarge);
            }
            let (name, value) = pair.split_once('=').ok_or(InputError::Malformed)?;
            let name = decode_component(name)?;
            let value = decode_component(value)?;
            if name.is_empty() || name.len() > 64 || !names.insert(name.clone()) {
                return Err(InputError::Malformed);
            }
            if name == "_csrf" {
                if value.is_empty() || value.len() > 128 {
                    return Err(InputError::MissingProof);
                }
                proof = Some(value);
            } else {
                if matches!(name.as_str(), "_generation" | "_version") {
                    return Err(InputError::Malformed);
                }
                if value.len() > 4096 {
                    return Err(InputError::TooLarge);
                }
                fields.push((name, value));
            }
        }
        Ok(Self { proof: proof.ok_or(InputError::MissingProof)?, fields })
    }
}

fn decode_component(encoded: &str) -> Result<String, InputError> {
    let bytes = encoded.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'%' && !bytes.get(index + 1..index + 3).is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit)) {
            return Err(InputError::Malformed);
        }
    }
    percent_encoding::percent_decode_str(&encoded.replace('+', " "))
        .decode_utf8()
        .map(std::borrow::Cow::into_owned)
        .map_err(|_| InputError::Malformed)
}

/// Browser URLSearchParams may include UTF-8; no other media/charset/encoding is accepted.
pub(super) fn supported_content_type(headers: &HeaderMap) -> bool {
    if headers.contains_key(header::CONTENT_ENCODING) || headers.get_all(header::CONTENT_TYPE).iter().count() != 1 {
        return false;
    }
    let Some(value) = headers.get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()).filter(|value| value.len() <= 128) else {
        return false;
    };
    let mut parts = value.split(';').map(str::trim);
    if !parts.next().is_some_and(|media| media.eq_ignore_ascii_case("application/x-www-form-urlencoded")) {
        return false;
    }
    let charset = parts.next();
    parts.next().is_none() && charset.is_none_or(|value| value.eq_ignore_ascii_case("charset=utf-8"))
}

/// Check both signals independently. A same-origin Fetch header cannot mask a bad Origin.
pub(super) fn acceptable_origin(headers: &HeaderMap, public_url: &str) -> bool {
    if headers.get_all("sec-fetch-site").iter().count() > 1 || headers.get_all(header::ORIGIN).iter().count() > 1 {
        return false;
    }
    if headers.get("sec-fetch-site").is_some_and(|site| !matches!(site.as_bytes(), b"same-origin" | b"none")) {
        return false;
    }
    let Some(origin) = headers.get(header::ORIGIN) else { return true };
    let Ok(origin) = origin.to_str() else { return false };
    if origin.len() > 256 || origin.contains(['#', ',']) || origin.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return false;
    }
    let (Ok(origin), Ok(expected)) = (origin.parse::<axum::http::Uri>(), public_url.parse::<axum::http::Uri>()) else { return false };
    let (Some(origin_scheme), Some(expected_scheme), Some(origin_authority), Some(expected_authority)) =
        (origin.scheme_str(), expected.scheme_str(), origin.authority(), expected.authority())
    else {
        return false;
    };
    if !matches!(origin_scheme, "http" | "https")
        || origin_authority.as_str().contains('@')
        || origin.query().is_some()
        || !matches!(origin.path(), "" | "/")
    {
        return false;
    }
    let (Some(origin_port), Some(expected_port)) = (port(origin_authority, origin_scheme), port(expected_authority, expected_scheme))
    else {
        return false;
    };
    origin_scheme == expected_scheme
        && origin_authority.host().eq_ignore_ascii_case(expected_authority.host())
        && origin_port == expected_port
}

fn port(authority: &axum::http::uri::Authority, scheme: &str) -> Option<u16> {
    let text = authority.as_str();
    let suffix = if text.starts_with('[') { &text[text.find(']')? + 1..] } else { text.strip_prefix(authority.host())? };
    if suffix.is_empty() {
        return Some(if scheme == "https" { 443 } else { 80 });
    }
    let digits = suffix.strip_prefix(':')?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_decoding_preserves_text_without_ambiguous_fields() {
        let form = ParsedForm::decode(b"_csrf=proof&title=Bonjour+%C3%A9&empty=").unwrap();
        assert_eq!(form.proof, "proof");
        assert_eq!(form.fields, vec![("title".into(), "Bonjour é".into()), ("empty".into(), String::new())]);
        for value in [
            "_csrf=a&%5Fcsrf=b",
            "_csrf=a&title=a&%74itle=b",
            "_csrf=a&title=%",
            "_csrf=a&title=%GG",
            "_csrf=a&title=%FF",
            "_csrf=a&&title=b",
            "_csrf=a&title",
            "_csrf=a&=b",
            "_csrf=a&_generation=1",
            "_csrf=a&_version=1",
        ] {
            assert!(ParsedForm::decode(value.as_bytes()).is_err(), "{value}");
        }
        assert_eq!(ParsedForm::decode(b"title=a").unwrap_err(), InputError::MissingProof);
        assert_eq!(ParsedForm::decode(&vec![b'a'; MAX_ENCODED_FORM_BYTES + 1]).unwrap_err(), InputError::TooLarge);
        assert!(ParsedForm::decode(format!("_csrf=a&title={}", "x".repeat(4097)).as_bytes()).is_err());
    }

    #[test]
    fn form_media_is_unambiguous_and_never_compressed() {
        for value in ["application/x-www-form-urlencoded", "application/x-www-form-urlencoded; charset=UTF-8"] {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, value.parse().unwrap());
            assert!(supported_content_type(&headers));
        }
        for value in [
            "text/plain",
            "application/json",
            "multipart/form-data; boundary=x",
            "application/x-www-form-urlencoded; charset=latin1",
            "application/x-www-form-urlencoded; charset=utf-8; charset=utf-8",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, value.parse().unwrap());
            assert!(!supported_content_type(&headers));
        }
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, "application/x-www-form-urlencoded".parse().unwrap());
        headers.insert(header::CONTENT_ENCODING, "gzip".parse().unwrap());
        assert!(!supported_content_type(&headers));
    }

    #[test]
    fn fetch_metadata_never_overrides_origin_and_missing_origin_still_needs_proof_elsewhere() {
        let expected = "https://forge.example";
        let mut headers = HeaderMap::new();
        assert!(acceptable_origin(&headers, expected));
        headers.insert("sec-fetch-site", "same-origin".parse().unwrap());
        for origin in [
            "https://evil.example",
            "http://forge.example",
            "https://forge.example:444",
            "https://forge.example:99999",
            "https://forge.example:invalid",
            "https://forge.example:",
            "null",
            "https://forge.example/path",
            "https://forge.example?x=1",
            "https://user@forge.example",
        ] {
            headers.insert(header::ORIGIN, origin.parse().unwrap());
            assert!(!acceptable_origin(&headers, expected), "{origin}");
        }
        headers.insert(header::ORIGIN, "https://forge.example:443".parse().unwrap());
        assert!(acceptable_origin(&headers, expected));
        headers.insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert!(!acceptable_origin(&headers, expected));
        headers.insert("sec-fetch-site", "same-origin".parse().unwrap());
        headers.append(header::ORIGIN, "https://forge.example".parse().unwrap());
        assert!(!acceptable_origin(&headers, expected));
        let mut ipv6 = HeaderMap::new();
        ipv6.insert(header::ORIGIN, "https://[::1]:443".parse().unwrap());
        assert!(acceptable_origin(&ipv6, "https://[::1]"));
    }
}
