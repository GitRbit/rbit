//! Read-only runtime dispatch inside the normal host browser security stack.

use axum::{
    Router,
    extract::{OriginalUri, Path, Request, State, rejection::PathRejection},
    http::{StatusCode, Uri, header},
    middleware::{Next, from_fn},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use rbit_plugin_api::PluginId;
use rbit_services::plugin_pages::{
    PageActor, PageAudience, PageContext, PageError, PageRepository, PageRouteHandle, PageScope, validate_page_path,
};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    views::{
        layout::Page,
        plugin_pages::{GlobalPluginPage, RepositoryPluginPage},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/-/plugins/{plugin}/{*page}", get(global))
        .route("/{owner}/{repo}/-/plugins/{plugin}/{*page}", get(repository))
        .route("/-/plugins", get(absent))
        .route("/-/plugins/{plugin}", get(absent))
        .route("/{owner}/{repo}/-/plugins", get(absent))
        .route("/{owner}/{repo}/-/plugins/{plugin}", get(absent))
        .route_layer(from_fn(runtime_page_policy))
}

async fn absent() -> WebError {
    WebError::NotFound
}

async fn runtime_page_policy(request: Request, next: Next) -> Response {
    // Read-only callbacks can still issue short-lived data. Preloads never invoke
    // them and runtime pages never retain an HTTP cache representation.
    if crate::middleware::is_preload(request.headers()) {
        return crate::middleware::decline_preload();
    }
    let mut response = next.run(request).await;
    response.headers_mut().insert(header::CACHE_CONTROL, axum::http::HeaderValue::from_static("no-store"));
    response
}

#[derive(Deserialize)]
struct GlobalPath {
    plugin: String,
    page: String,
}
#[derive(Deserialize)]
struct RepositoryPath {
    owner: String,
    repo: String,
    plugin: String,
    page: String,
}

fn lookup(state: &AppState, uri: &Uri, owner: &str, path: &str, scope: PageScope) -> WebResult<PageRouteHandle> {
    // Canonical literal routes have no percent-encoded aliases, including encoded
    // slash/dot/control bytes that a decoded wildcard could otherwise accept.
    if uri.path().contains('%') || !validate_page_path(path) {
        return Err(WebError::NotFound);
    }
    let owner: PluginId = owner.parse().map_err(|_| WebError::NotFound)?;
    state.services.page_registry().snapshot().find(&owner, scope, path).ok_or(WebError::NotFound)
}

fn sign_in(uri: &Uri) -> Response {
    let target = uri.path_and_query().map_or("/", |target| target.as_str());
    Redirect::to(&format!("/login?next={}", crate::urls::encode_path(target).replace('&', "%26"))).into_response()
}

async fn global(
    State(state): State<AppState>,
    viewer: Viewer,
    mut page: Page,
    OriginalUri(uri): OriginalUri,
    path: Result<Path<GlobalPath>, PathRejection>,
) -> WebResult<Response> {
    let Path(path) = path.map_err(|_| WebError::NotFound)?;
    page.path = uri.path().to_owned();
    let route = lookup(&state, &uri, &path.plugin, &path.page, PageScope::Global)?;
    match route.descriptor().audience() {
        PageAudience::SignedIn | PageAudience::SiteAdmin if viewer.user().is_none() => return Ok(sign_in(&uri)),
        PageAudience::SiteAdmin if !viewer.user().is_some_and(|user| user.is_admin) => return Err(WebError::NotFound),
        PageAudience::Public | PageAudience::SignedIn | PageAudience::SiteAdmin => {}
        PageAudience::RepositoryReader | PageAudience::RepositoryAdmin => return Err(WebError::NotFound),
    }
    render(route, page, &viewer, None, &state).await
}

async fn repository(
    State(state): State<AppState>,
    viewer: Viewer,
    mut page: Page,
    OriginalUri(uri): OriginalUri,
    path: Result<Path<RepositoryPath>, PathRejection>,
) -> WebResult<Response> {
    let Path(path) = path.map_err(|_| WebError::NotFound)?;
    page.path = uri.path().to_owned();
    let route = lookup(&state, &uri, &path.plugin, &path.page, PageScope::Repository)?;
    let loaded = RepoPage::load(&state, &viewer, &path.owner, &path.repo).await?;
    match route.descriptor().audience() {
        PageAudience::RepositoryReader => {}
        PageAudience::RepositoryAdmin if loaded.ctx.access.can_admin() => {}
        _ => return Err(WebError::NotFound),
    }
    render(route, page, &viewer, Some(loaded), &state).await
}

async fn render(
    route: PageRouteHandle,
    page: Page,
    viewer: &Viewer,
    repository: Option<RepoPage>,
    state: &AppState,
) -> WebResult<Response> {
    let context = PageContext {
        actor: viewer.user().map(|user| PageActor { account_id: user.id().0, is_admin: user.is_admin }),
        repository: repository.as_ref().map(|loaded| PageRepository {
            repository_id: loaded.ctx.repo.repo.id.0,
            owner: loaded.ctx.repo.owner.name.to_string(),
            name: loaded.ctx.repo.repo.name.to_string(),
            can_admin: loaded.ctx.access.can_admin(),
        }),
        locale: page.locale.clone(),
    };
    let Ok(mut lease) = route.admit() else { return Ok(unavailable()) };
    let body = match lease.render(context).await {
        Ok(body) => body,
        Err(PageError::Forbidden) => return Err(WebError::NotFound),
        Err(_) => return Ok(unavailable()),
    };
    let page = page.titled(body.title());
    let owner = route.owner().to_string();
    let path = route.descriptor().path().to_owned();
    let response = if let Some(repository) = repository {
        RepositoryPluginPage { page, header: repository.header(state, ""), owner, route: path, body }.into_response()
    } else {
        GlobalPluginPage { page, owner, route: path, body }.into_response()
    };
    let (guard, cancellation) = lease.into_parts();
    let mut response = response;
    response.extensions_mut().insert(crate::middleware::PageResponseGuard::new(guard, cancellation));
    Ok(response)
}

fn unavailable() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, [(header::CACHE_CONTROL, "no-store")], "Plugin page temporarily unavailable.").into_response()
}
