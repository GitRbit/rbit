//! Language changes are native settings writes, never redirects to caller URLs. A signed-in
//! user's choice is saved to the account; a visitor's goes into the `rbit-locale` cookie,
//! which negotiation honours before Accept-Language. The header's language dialog posts the
//! same form.
use super::SavedQuery;
use crate::{
    AppState,
    client_ip::Https,
    error::WebResult,
    extract::Viewer,
    localization::{AccountLanguage, PREFERENCE_COOKIE, RequestLocale},
    views::{
        layout::Page,
        settings::{LanguageDialog, LanguagePage, VisitorLanguagePage},
    },
};
use axum::{
    Form,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use rbit_identity_models::AccountLocale;
use rbit_localization::{Locale, PreferenceSource};
use serde::Deserialize;

/// How long a visitor's language cookie lasts, like the theme cookie.
const VISITOR_COOKIE_DAYS: i64 = 365;

/// `GET /settings/language`: the account's language settings, or for a visitor who is not
/// signed in the same form saved in a cookie (the header button's no-script destination).
pub async fn show(
    viewer: Viewer,
    page: Page,
    AccountLanguage(selected): AccountLanguage,
    RequestLocale(resolved): RequestLocale,
    Query(query): Query<SavedQuery>,
) -> Response {
    let saved = query.saved.is_some();
    if viewer.user().is_some() {
        LanguagePage::new(page, selected, saved).into_response()
    } else {
        VisitorLanguagePage::new(page, visitor_choice(resolved.source, resolved.locale), saved).into_response()
    }
}

/// `GET /settings/language/dialog`: the header's language dialog, fetched by `language-picker.js`.
/// Like Explore, it opts into a diagnostic locale when the resolver enables one, so it opens
/// in the direction of the page that asked for it.
pub async fn dialog(page: Page, AccountLanguage(account): AccountLanguage, RequestLocale(resolved): RequestLocale) -> LanguageDialog {
    LanguageDialog::new(page.with_locale(resolved.locale), account.is_some() || resolved.source == PreferenceSource::Anonymous)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageForm {
    #[serde(default)]
    locale: String,
}

#[expect(clippy::too_many_arguments, reason = "the request, the viewer and both preference sources")]
pub async fn save(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    AccountLanguage(current): AccountLanguage,
    RequestLocale(resolved): RequestLocale,
    Https(https): Https,
    jar: CookieJar,
    headers: HeaderMap,
    Form(form): Form<LanguageForm>,
) -> WebResult<Response> {
    let choice = if form.locale.is_empty() { Some(None) } else { AccountLocale::parse(&form.locale).map(Some) };
    let Some(choice) = choice else {
        let response = if viewer.user().is_some() {
            let mut view = LanguagePage::new(page, current, false);
            view.invalid = true;
            view.into_response()
        } else {
            let mut view = VisitorLanguagePage::new(page, visitor_choice(resolved.source, resolved.locale), false);
            view.invalid = true;
            view.into_response()
        };
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, response).into_response());
    };
    let removed = Cookie::build((PREFERENCE_COOKIE, "")).path("/").http_only(true).same_site(SameSite::Lax).build();
    let jar = match (viewer.user(), choice) {
        (Some(user), _) => {
            state.services.preferences.set_locale(user, choice).await?;
            // Account choice takes precedence. Removing the old anonymous preference makes
            // clearing the saved choice actually follow browser/default negotiation again.
            jar.remove(removed)
        }
        (None, Some(locale)) => jar.add(visitor_cookie(locale, https)),
        (None, None) => jar.remove(removed),
    };
    let mut response = if crate::localization::single_header(&headers, "hx-request") == Some("true") {
        let mut response = (StatusCode::OK, jar).into_response();
        response.headers_mut().insert("hx-redirect", HeaderValue::from_static("/settings/language?saved=1"));
        response
    } else {
        (jar, Redirect::to("/settings/language?saved=1")).into_response()
    };
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// The language a visitor's cookie chose, shown as selected on their language page.
fn visitor_choice(source: PreferenceSource, locale: Locale) -> Option<Locale> {
    (source == PreferenceSource::Anonymous).then_some(locale)
}

/// A visitor's language preference: only the canonical tag, nothing that identifies them.
/// Sent to this site alone (`SameSite=Lax`), never to scripts, kept for a year.
fn visitor_cookie(locale: AccountLocale, https: bool) -> Cookie<'static> {
    Cookie::build((PREFERENCE_COOKIE, locale.tag()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(https)
        .max_age(time::Duration::days(VISITOR_COOKIE_DAYS))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visitor_cookie_holds_only_the_tag_for_this_site() {
        let cookie = visitor_cookie(AccountLocale::PortugueseBrazil, true).to_string();
        for part in ["rbit-locale=pt-BR", "Path=/", "HttpOnly", "SameSite=Lax", "Secure", "Max-Age=31536000"] {
            assert!(cookie.contains(part), "{cookie} has {part}");
        }
        assert!(!visitor_cookie(AccountLocale::German, false).to_string().contains("Secure"));
    }

    #[test]
    fn only_a_cookie_choice_is_shown_as_the_visitors_selection() {
        assert_eq!(visitor_choice(PreferenceSource::Anonymous, Locale::Japanese), Some(Locale::Japanese));
        for source in [PreferenceSource::AcceptLanguage, PreferenceSource::Default, PreferenceSource::Account] {
            assert_eq!(visitor_choice(source, Locale::Japanese), None);
        }
    }
}
