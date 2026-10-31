//! Language preferences are host-owned and survive optional provider disablement.

use askama::Template;
use askama_web::WebTemplate;
use axum::{
    Form, Router,
    extract::{Extension, State},
    http::header,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::Deserialize;

use crate::{
    AppState,
    client_ip::Https,
    error::{WebError, WebResult},
    extract::Viewer,
    middleware::LOCALE_COOKIE,
    views::layout::Page,
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/language", get(index).post(choose))
}

#[derive(Template, WebTemplate)]
#[template(path = "i18n/language.html")]
struct LanguagePage {
    page: Page,
    locales: Vec<String>,
    selected: String,
}

async fn index(
    State(state): State<AppState>,
    page: Page,
    jar: CookieJar,
    account: Option<Extension<crate::middleware::AccountLocale>>,
) -> WebResult<LanguagePage> {
    if !state.services.i18n_enabled() {
        return Err(WebError::NotFound);
    }
    let selected = account
        .and_then(|Extension(locale)| locale.0)
        .or_else(|| jar.get(LOCALE_COOKIE).map(|cookie| cookie.value().to_owned()))
        .unwrap_or_default();
    Ok(LanguagePage { selected, page: page.titled(crate::i18n::text("core", "locale-title")), locales: state.services.available_locales() })
}

#[derive(Deserialize)]
struct Choice {
    locale: String,
}

async fn choose(
    State(state): State<AppState>,
    viewer: Viewer,
    jar: CookieJar,
    Https(https): Https,
    Form(form): Form<Choice>,
) -> WebResult<Response> {
    if !state.services.i18n_enabled() {
        return Err(WebError::NotFound);
    }
    // Only negotiated, currently available choices are accepted from this form.
    // Existing unsupported stored preferences remain intact until explicitly changed.
    if !form.locale.is_empty() && !state.services.available_locales().contains(&form.locale) {
        return Err(WebError::BadRequest(crate::i18n::text("core", "locale-unavailable").into_owned()));
    }
    let locale = (!form.locale.is_empty()).then_some(form.locale.as_str());
    if let Some(user) = viewer.user() {
        state.services.set_account_locale(user, locale).await?;
    }
    let explicit = locale.is_some();
    let cookie = Cookie::build((LOCALE_COOKIE, form.locale)).path("/").same_site(SameSite::Lax).secure(https).http_only(true);
    let jar = if explicit { jar.add(cookie.max_age(time::Duration::days(365)).build()) } else { jar.remove(cookie.build()) };
    Ok((jar, [("hx-redirect", "/language"), (header::CACHE_CONTROL.as_str(), "no-store")], Redirect::to("/language")).into_response())
}
