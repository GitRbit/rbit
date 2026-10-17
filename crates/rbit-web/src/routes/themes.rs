//! `/themes`: the theme gallery and the endpoint every theme picker posts to.
//!
//! Frontend workstream T owns this file. Choosing a theme sets the `rbit-theme` cookie
//! (anonymous visitors included) and, for signed-in users, stores it in their preferences
//! so sign-in on another device restores it.

use axum::{
    Form, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::{
    AppState,
    client_ip::Https,
    error::{WebError, WebResult},
    extract::Viewer,
    themes::Theme,
    urls::safe_redirect_target,
    views::{
        layout::Page,
        themes::{GALLERY_URL, ThemesPage, VariantFilter},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new().route(GALLERY_URL, get(gallery).post(choose))
}

/// `?variant=dark&q=gruvbox`
#[derive(Debug, Deserialize)]
pub struct GalleryQuery {
    pub variant: Option<String>,
    pub q: Option<String>,
}

/// `GET /themes`
pub async fn gallery(page: Page, Query(query): Query<GalleryQuery>) -> ThemesPage {
    ThemesPage::new(page, VariantFilter::parse(query.variant.as_deref()), query.q.as_deref())
}

#[derive(Debug, Deserialize)]
pub struct ChooseForm {
    #[serde(default)]
    theme: String,
    next: Option<String>,
    /// Sent by `theme-picker.js`, which has already applied the theme: answer `204`.
    js: Option<String>,
}

/// `POST /themes`: remembers the chosen theme and returns to where it was chosen.
pub async fn choose(
    State(state): State<AppState>,
    viewer: Viewer,
    jar: CookieJar,
    Https(https): Https,
    Form(form): Form<ChooseForm>,
) -> WebResult<Response> {
    let theme = Theme::parse(&form.theme).ok_or_else(|| WebError::BadRequest("That theme does not exist.".to_owned()))?;
    if let Some(user) = viewer.user() {
        state.services.preferences.set_theme(user, theme.slug()).await?;
    }
    let jar = jar.add(theme.cookie(https));
    if form.js.is_some() {
        return Ok((StatusCode::NO_CONTENT, jar).into_response());
    }
    let next = safe_redirect_target(form.next.as_deref().or(Some(GALLERY_URL)));
    Ok((jar, Redirect::to(next)).into_response())
}
