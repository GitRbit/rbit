//! Request-scoped presentation locale; no preference writes or database reads.
//!
//! Production supports English. Embedded composition/tests may inject an immutable
//! `LocaleResolver` extension to enable diagnostics explicitly. Saved account choices
//! will be supplied by the identity adapter once that migration is implemented.

use std::convert::Infallible;

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, HeaderValue, header, request::Parts},
    response::Response,
};
use rbit_localization::{Locale, LocalePreferences, LocaleResolver, ResolvedLocale};

/// Trusted typed preferences supplied by a locale-setting or identity adapter.
/// Incoming arbitrary headers never populate this extension.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocaleChoices {
    pub explicit: Option<Locale>,
    pub account: Option<Locale>,
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
            account: choices.account.map(Locale::tag),
            anonymous: preference_cookie(&parts.headers),
            accept_language: single_header(&parts.headers, "accept-language"),
        }));
        parts.extensions.insert(locale);
        std::future::ready(Ok(locale))
    }
}

impl RequestLocale {
    /// Adds language/cache dimensions without replacing privacy or existing Vary.
    pub fn response(self, mut response: Response) -> Response {
        response.headers_mut().insert(header::CONTENT_LANGUAGE, HeaderValue::from_static(self.0.locale.tag()));
        response
            .headers_mut()
            .append(header::VARY, HeaderValue::from_static("Accept-Language, Cookie, HX-Target, HX-History-Restore-Request, HX-Boosted"));
        response
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
