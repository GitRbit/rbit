//! Language changes are account-owned native settings writes, never redirects to caller URLs.
use super::SavedQuery;
use crate::{
    AppState,
    error::WebResult,
    extract::SignedIn,
    localization::AccountLanguage,
    views::{layout::Page, settings::LanguagePage},
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
use serde::Deserialize;

pub async fn show(
    SignedIn(_): SignedIn,
    page: Page,
    AccountLanguage(selected): AccountLanguage,
    Query(query): Query<SavedQuery>,
) -> LanguagePage {
    LanguagePage::new(page, selected, query.saved.is_some())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageForm {
    #[serde(default)]
    locale: String,
}

pub async fn save(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    AccountLanguage(current): AccountLanguage,
    jar: CookieJar,
    headers: HeaderMap,
    Form(form): Form<LanguageForm>,
) -> WebResult<Response> {
    let choice = if form.locale.is_empty() { Some(None) } else { AccountLocale::parse(&form.locale).map(Some) };
    let Some(choice) = choice else {
        let mut view = LanguagePage::new(page, current, false);
        view.invalid = true;
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, view).into_response());
    };
    state.services.preferences.set_locale(&user, choice).await?;
    // Account choice takes precedence. Removing the old anonymous preference makes
    // clearing the saved choice actually follow browser/default negotiation again.
    let jar = jar.remove(Cookie::build(("rbit-locale", "")).path("/").http_only(true).same_site(SameSite::Lax).build());
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
