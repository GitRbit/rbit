//! Request-scoped presentation locale; no preference writes or database reads.
//!
//! Production supports every language of `Locale::LANGUAGES` with an English default. The session adapter
//! supplies the saved account choice through its existing lookup. Embedded tests may
//! inject an immutable `LocaleResolver` to enable diagnostics for selected features.

use std::convert::Infallible;

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, HeaderValue, header, request::Parts},
    response::Response,
};
use rbit_identity_models::AccountLocale;
use rbit_localization::{Locale, LocalePreferences, LocaleResolver, ResolvedLocale};

#[path = "localization/pipeline.rs"]
mod pipeline;
pub use pipeline::page_locale;
pub(crate) const CATALOG_HEADER: &str = "rbit-catalog-key";
#[cfg(test)]
#[path = "localization/pipeline_tests.rs"]
mod pipeline_tests;

/// Trusted typed preferences supplied by a locale-setting or identity adapter.
/// Incoming arbitrary headers never populate this extension.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocaleChoices {
    pub explicit: Option<Locale>,
    pub account: Option<AccountLocale>,
}

/// The supported presentation locale and provenance, resolved once per request.
#[derive(Clone, Copy, Debug)]
pub struct RequestLocale(pub ResolvedLocale);

impl<S: Send + Sync> FromRequestParts<S> for RequestLocale {
    type Rejection = Infallible;

    fn from_request_parts(parts: &mut Parts, _state: &S) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        if let Some(locale) = parts.extensions.get::<Self>() {
            return std::future::ready(Ok(*locale));
        }
        let resolver = parts.extensions.get::<LocaleResolver>().copied().unwrap_or_default();
        let choices = parts.extensions.get::<LocaleChoices>().copied().unwrap_or_default();
        let locale = Self(resolver.resolve(LocalePreferences {
            explicit: choices.explicit.map(Locale::tag),
            account: choices.account.map(AccountLocale::tag),
            anonymous: preference_cookie(&parts.headers),
            accept_language: single_header(&parts.headers, "accept-language"),
        }));
        parts.extensions.insert(locale);
        std::future::ready(Ok(locale))
    }
}

impl RequestLocale {
    /// Diagnostics remain feature-owned opt-ins, never a global shell default.
    pub const fn document_locale(self) -> Locale {
        match self.0.locale {
            Locale::PseudoExpanded | Locale::PseudoRtl => Locale::English,
            locale => locale,
        }
    }

    /// Adds language/cache dimensions without replacing privacy or existing Vary.
    pub fn response(self, response: Response) -> Response {
        response_for(self.0.locale, response)
    }
}

fn response_for(locale: Locale, mut response: Response) -> Response {
    let first = !response.headers().contains_key(CATALOG_HEADER);
    response.headers_mut().insert(header::CONTENT_LANGUAGE, HeaderValue::from_static(locale.tag()));
    response.headers_mut().insert(CATALOG_HEADER, HeaderValue::from_static(locale.catalog_key()));
    if first {
        const DIMENSIONS: &str = "Accept-Language, Cookie, HX-Target, HX-History-Restore-Request, HX-Boosted, Rbit-Document-Locale";
        // Outer partial/cache layers read one Vary value. Preserve every pre-existing
        // dimension in that value rather than append a second field they could lose.
        let value = if response.headers().contains_key(header::VARY) {
            let mut combined = String::new();
            for existing in response.headers().get_all(header::VARY) {
                if let Ok(existing) = existing.to_str() {
                    combined.push_str(existing);
                    combined.push_str(", ");
                }
            }
            combined.push_str(DIMENSIONS);
            HeaderValue::from_str(&combined).expect("existing HTTP header values remain valid")
        } else {
            HeaderValue::from_static(DIMENSIONS)
        };
        response.headers_mut().insert(header::VARY, value);
    }
    response
}

/// Account choice from the session's existing SQL projection, not another lookup.
#[derive(Clone, Copy, Debug)]
pub struct AccountLanguage(pub Option<AccountLocale>);
impl<S: Send + Sync> FromRequestParts<S> for AccountLanguage {
    type Rejection = Infallible;
    fn from_request_parts(parts: &mut Parts, _state: &S) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(Ok(Self(parts.extensions.get::<LocaleChoices>().and_then(|choices| choices.account))))
    }
}

/// Ambiguous repeated shape/preference headers never enable an alternate response.
pub(crate) fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    Some(value)
}

fn preference_cookie(headers: &HeaderMap) -> Option<&str> {
    let cookie = single_header(headers, "cookie")?;
    if cookie.len() > 4096 {
        return None;
    }
    let mut choice = None;
    for (index, pair) in cookie.split(';').enumerate() {
        if index >= 32 {
            return None;
        }
        if let Some(("rbit-locale", value)) = pair.trim().split_once('=') {
            if choice.is_some() {
                return None;
            }
            choice = Some(value);
        }
    }
    choice
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use rbit_localization::PreferenceSource;

    #[test]
    fn locale_dimensions_preserve_all_existing_vary_fields_for_outer_layers() {
        let mut original = Response::new(Body::empty());
        original.headers_mut().append(header::VARY, HeaderValue::from_static("Cookie"));
        original.headers_mut().append(header::VARY, HeaderValue::from_static("Accept-Encoding"));
        let response = response_for(Locale::Spanish, original);
        assert_eq!(response.headers().get_all(header::VARY).iter().count(), 1);
        let vary = response.headers()[header::VARY].to_str().unwrap();
        for dimension in ["Cookie", "Accept-Encoding", "Accept-Language", "Rbit-Document-Locale", "HX-Target"] {
            assert!(vary.contains(dimension), "{dimension}");
        }
    }

    #[tokio::test]
    async fn production_defaults_ignore_diagnostic_and_untrusted_override_headers() {
        let (mut parts, _) = Request::builder()
            .header("accept-language", "ar-XB")
            .header("rbit-locale", "ar-XB")
            .header("cookie", "rbit-locale=en-XA")
            .body(Body::empty())
            .unwrap()
            .into_parts();
        let selected = RequestLocale::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(selected.0.locale, Locale::English);
        assert_eq!(selected.0.source, PreferenceSource::Default);
    }

    #[tokio::test]
    async fn typed_choices_cookie_and_header_share_one_request_result() {
        let (mut parts, _) = Request::builder()
            .header("accept-language", "ar-XB")
            .header("cookie", "rbit-locale=en-XA")
            .body(Body::empty())
            .unwrap()
            .into_parts();
        parts.extensions.insert(LocaleResolver::new(Locale::English, true).unwrap());
        let selected = RequestLocale::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(selected.0.locale, Locale::PseudoExpanded);
        parts.extensions.insert(LocaleChoices { explicit: Some(Locale::PseudoRtl), account: None });
        assert_eq!(RequestLocale::from_request_parts(&mut parts, &()).await.unwrap().0, selected.0, "request locale is stable");
    }

    #[tokio::test]
    async fn ambiguous_or_oversized_cookies_do_not_override_language() {
        for cookie in ["rbit-locale=en-XA; rbit-locale=ar-XB".to_owned(), format!("rbit-locale=en-XA; x={}", "x".repeat(4096))] {
            let (mut parts, _) =
                Request::builder().header("accept-language", "ar-XB").header("cookie", cookie).body(Body::empty()).unwrap().into_parts();
            parts.extensions.insert(LocaleResolver::new(Locale::English, true).unwrap());
            let selected = RequestLocale::from_request_parts(&mut parts, &()).await.unwrap();
            assert_eq!(selected.0.locale, Locale::PseudoRtl);
            assert_eq!(selected.0.source, PreferenceSource::AcceptLanguage);
        }
    }
}
