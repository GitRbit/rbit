//! Starring repositories: `/{owner}/{repo}/star`, `/{owner}/{repo}/stargazers`.
//!
//! Workstream D owns this file. Its routes are merged into the page router, so they get
//! sessions, CSRF protection, security headers and error pages automatically.

use axum::{
    Form, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    urls::{RepoUrls, encode_path, safe_redirect_target},
    views::{
        layout::Page,
        repo_settings::{StarButton, StarButtonFragment, StargazerRow, StargazersPage},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/{owner}/{repo}/star", post(toggle)).route("/{owner}/{repo}/stargazers", get(stargazers))
}

#[derive(Debug, Deserialize)]
pub struct StarFormInput {
    /// `star` or `unstar`; anything else (or nothing) toggles the current state.
    action: Option<String>,
    /// Where to return to without JavaScript.
    next: Option<String>,
}

/// An htmx request that wants a fragment: sent by `hx-post`, not by a boosted form.
fn wants_fragment(headers: &HeaderMap) -> bool {
    headers.contains_key("hx-request") && !headers.contains_key("hx-boosted")
}

/// `POST /{owner}/{repo}/star`: stars or unstars, then returns the updated button
/// (htmx) or redirects back (plain form post). Anonymous visitors are sent to sign in.
pub async fn toggle(
    State(state): State<AppState>,
    viewer: Viewer,
    headers: HeaderMap,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<StarFormInput>,
) -> WebResult<Response> {
    let ctx = state.services.repositories.open(viewer.user(), &owner, &repo).await?;
    let urls = RepoUrls::new(ctx.repo.owner.name.as_str(), ctx.repo.repo.name.as_str());
    let return_to = form.next.as_deref().map_or_else(|| urls.home(), |next| safe_redirect_target(Some(next)).to_owned());

    let Some(user) = viewer.user() else {
        let login = format!("/login?next={}", encode_path(&return_to).replace('&', "%26"));
        return Ok(Redirect::to(&login).into_response());
    };

    let stars = &state.services.stars;
    let star = match form.action.as_deref() {
        Some("star") => true,
        Some("unstar") => false,
        _ => !stars.summary(ctx.repo.repo.id, Some(user)).await?.starred_by_viewer,
    };
    let summary = if star { stars.star(user, &ctx).await? } else { stars.unstar(user, &ctx).await? };

    if wants_fragment(&headers) {
        let fragment = StarButtonFragment { star: StarButton::new(&urls.home(), summary, true, &return_to) };
        return Ok(([(header::VARY, HeaderValue::from_static("HX-Request"))], fragment).into_response());
    }
    Ok(Redirect::to(&return_to).into_response())
}

/// `GET /{owner}/{repo}/stargazers`
pub async fn stargazers(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<StargazersPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let stargazers = state.services.stars.stargazers(&loaded.ctx).await?;
    Ok(StargazersPage {
        page: page.titled(format!("Stargazers · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "stargazers"),
        stargazers: stargazers.iter().map(StargazerRow::from).collect(),
    })
}
