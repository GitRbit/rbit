//! Error page rendering and moved-repository redirects.

use axum::{
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{
    AppState,
    error::ErrorInfo,
    extract::Viewer,
    themes::Theme,
    views::{errors::ErrorPage, layout::Page},
};

/// Replaces bare error responses produced by [`crate::error::WebError`] with the full
/// error page, rendered with the current user's navigation.
///
/// A "not found" under `/{owner}/{repo}/…` where `owner/repo` is an old name of a
/// repository the viewer may read (it was renamed or transferred) becomes a redirect to
/// the same path under the current name instead; see [`moved_repository_location`].
pub async fn error_pages(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let viewer = request.extensions().get::<Viewer>().cloned().unwrap_or_default();
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let query = request.uri().query().map(str::to_owned);
    let theme = Theme::from_headers(request.headers());
    let response = next.run(request).await;

    let Some(info) = response.extensions().get::<ErrorInfo>().cloned() else {
        return response;
    };
    if info.status == StatusCode::NOT_FOUND
        && let Some(location) = moved_repository_location(&state, &viewer, &path, query.as_deref()).await
    {
        return moved_permanently(&method, &location);
    }
    let page = Page::new(&state, viewer.user(), &path).with_theme(theme).titled(info.localized_title());
    render_error_response(response, ErrorPage { page, info })
}

/// Replaces the body without discarding protocol headers such as `Retry-After`.
pub(super) fn render_error_response(response: Response, page: impl IntoResponse) -> Response {
    let (mut parts, _) = response.into_parts();
    let rendered = page.into_response();
    if let Some(content_type) = rendered.headers().get(header::CONTENT_TYPE) {
        parts.headers.insert(header::CONTENT_TYPE, content_type.clone());
    }
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, rendered.into_body())
}

/// `path` (and `query`) rewritten to the current name of the repository its first two
/// segments used to name, if the viewer may read that repository.
async fn moved_repository_location(state: &AppState, viewer: &Viewer, path: &str, query: Option<&str>) -> Option<String> {
    let profile = path.strip_prefix('/')?.trim_end_matches('/');
    if !profile.is_empty() && !profile.contains('/') {
        let moved = state.services.accounts.moved_to(profile).await.ok()??;
        let slash = if path.ends_with('/') { "/" } else { "" };
        let mut location = format!("/{}{slash}", moved.name);
        if let Some(query) = query {
            location.push('?');
            location.push_str(query);
        }
        return Some(location);
    }
    let (owner, repo, rest) = split_repo_path(path)?;
    let moved = match state.services.repositories.moved_to(viewer.user(), owner, repo).await {
        Ok(moved) => moved?,
        Err(error) => {
            tracing::error!(%error, "repository redirect lookup failed");
            return None;
        }
    };
    let mut location = format!("/{}{rest}", moved.full_name());
    if let Some(query) = query {
        location.push('?');
        location.push_str(query);
    }
    Some(location)
}

/// `/{owner}/{repo}{rest}` → `(owner, repo, rest)`, where `rest` is empty or starts with `/`.
pub(super) fn split_repo_path(path: &str) -> Option<(&str, &str, &str)> {
    let (owner, after_owner) = path.strip_prefix('/')?.split_once('/')?;
    let (repo, rest) = after_owner.find('/').map_or((after_owner, ""), |slash| after_owner.split_at(slash));
    (!owner.is_empty() && !repo.is_empty()).then_some((owner, repo, rest))
}

/// `301` for reads, `308` (which keeps the method and body) for everything else. Not
/// cacheable: a new repository can take the old name at any time, and then wins.
pub(super) fn moved_permanently(method: &Method, location: &str) -> Response {
    let status = if matches!(*method, Method::GET | Method::HEAD) { StatusCode::MOVED_PERMANENTLY } else { StatusCode::PERMANENT_REDIRECT };
    let Ok(location) = HeaderValue::from_str(location) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    (status, [(header::LOCATION, location), (header::CACHE_CONTROL, HeaderValue::from_static("no-store"))]).into_response()
}
