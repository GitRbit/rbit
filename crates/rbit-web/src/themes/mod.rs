//! Colour themes over HTTP: the theme a request's cookie chooses, the cookie that remembers
//! a choice and the content-hashed stylesheet URL of a tinted theme.
//!
//! The themes themselves (built-in and tinted catalog, slugs, variants, canvas colours, the
//! light/dark toggle and stylesheet paths) are [`rbit_presentation::themes`], re-exported
//! here so templates and handlers keep one import path.

use axum::http::HeaderMap;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
pub use rbit_presentation::themes::*;

use crate::assets;

/// The request and response side of a [`Theme`].
pub trait ThemeHttp: Sized {
    /// The theme chosen in a request's cookies; anything missing or unknown is the default.
    fn from_headers(headers: &HeaderMap) -> Self;

    /// The content-hashed stylesheet a tinted theme needs; built-in themes live in `app.css`.
    fn stylesheet(self) -> Option<String>;

    /// The cookie remembering this choice for a year, on every path. Not `HttpOnly`, so the
    /// picker can update it without a round trip; `Secure` whenever the site is on HTTPS.
    fn cookie(self, https: bool) -> Cookie<'static>;
}

impl ThemeHttp for Theme {
    fn from_headers(headers: &HeaderMap) -> Self {
        let jar = CookieJar::from_headers(headers);
        Self::from_cookies(jar.get(COOKIE).map(Cookie::value), jar.get(LEGACY_COOKIE).map(Cookie::value))
    }

    fn stylesheet(self) -> Option<String> {
        self.stylesheet_path().map(|path| assets::url(&path))
    }

    fn cookie(self, https: bool) -> Cookie<'static> {
        Cookie::build((COOKIE, self.slug()))
            .path("/")
            .same_site(SameSite::Lax)
            .secure(https)
            .http_only(false)
            .max_age(time::Duration::days(COOKIE_MAX_AGE_DAYS))
            .build()
    }
}

#[cfg(test)]
mod tests;
