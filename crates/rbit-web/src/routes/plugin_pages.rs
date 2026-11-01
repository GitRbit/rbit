//! Runtime page and form dispatch inside the normal host browser security stack.

mod browser_preference;
mod form_input;
mod forms;
mod query_input;
mod upload_input;

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
        plugin_pages::{AdminPluginPage, GlobalPluginPage, RepositoryPluginPage},
    },
};

/// Document factories construct their own full view. Their host authorization
/// needs repository grants, not an extra Git ref/header/count expansion.
enum AuthorizedRepository {
    Document(Box<rbit_services::RepoContext>),
    Shell(Box<RepoPage>),
}
impl AuthorizedRepository {
    async fn load(
        state: &AppState,
        viewer: &Viewer,
        owner: &str,
        repo: &str,
        presentation: rbit_services::plugin_pages::PagePresentation,
    ) -> WebResult<Self> {
        if presentation == rbit_services::plugin_pages::PagePresentation::Document {
            Ok(Self::Document(Box::new(state.services.repositories.open(viewer.user(), owner, repo).await?)))
        } else {
            Ok(Self::Shell(Box::new(RepoPage::load(state, viewer, owner, repo).await?)))
        }
    }
    fn ctx(&self) -> &rbit_services::RepoContext {
        match self {
            Self::Document(ctx) => ctx,
            Self::Shell(page) => &page.ctx,
        }
    }
    fn into_shell(self) -> WebResult<RepoPage> {
        match self {
            Self::Shell(page) => Ok(*page),
            Self::Document(_) => Err(WebError::NotFound),
        }
    }
}

struct ShellPolicy {
    native_forms: bool,
    administrator: bool,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/-/plugins/{plugin}/{*page}", get(global).post(forms::global))
        .route("/{owner}/{repo}/-/plugins/{plugin}/{*page}", get(repository).post(forms::repository))
        .route("/-/plugins", get(absent))
        .route("/-/plugins/{plugin}", get(absent))
        .route("/{owner}/{repo}/-/plugins", get(absent))
        .route("/{owner}/{repo}/-/plugins/{plugin}", get(absent))
        .route_layer(from_fn(runtime_page_policy))
}

pub(super) fn matches_route(pattern: &str) -> bool {
    matches!(
        pattern,
        "/-/plugins/{plugin}/{*page}"
            | "/{owner}/{repo}/-/plugins/{plugin}/{*page}"
            | "/-/plugins"
            | "/-/plugins/{plugin}"
            | "/{owner}/{repo}/-/plugins"
            | "/{owner}/{repo}/-/plugins/{plugin}"
    )
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
    crate::client_ip::Https(https): crate::client_ip::Https,
    request: Request,
) -> WebResult<Response> {
    let Path(path) = path.map_err(|_| WebError::NotFound)?;
    page.path = uri.path_and_query().map_or(uri.path(), |value| value.as_str()).to_owned();
    let route = lookup(&state, &uri, &path.plugin, &path.page, PageScope::Global)?;
    match route.descriptor().audience() {
        PageAudience::SignedIn | PageAudience::SiteAdmin if viewer.user().is_none() => return Ok(sign_in(&uri)),
        PageAudience::SiteAdmin if !viewer.user().is_some_and(|user| user.is_admin) => return Err(WebError::NotFound),
        PageAudience::Public | PageAudience::SignedIn | PageAudience::SiteAdmin => {}
        PageAudience::RepositoryReader | PageAudience::RepositoryAdmin => return Err(WebError::NotFound),
    }
    let session = request.extensions().get::<crate::extract::CurrentSession>().cloned();
    let browser = if forms::needs_browser(&route, &state) {
        browser_preference::BrowserForms::new(&request, https)
    } else {
        browser_preference::BrowserForms::empty(https)
    };
    let query = query_input::parse(uri.query(), route.descriptor().query())?;
    render(route, page, &viewer, session.as_ref(), None, (&state, browser, query)).await
}

async fn repository(
    State(state): State<AppState>,
    viewer: Viewer,
    mut page: Page,
    OriginalUri(uri): OriginalUri,
    path: Result<Path<RepositoryPath>, PathRejection>,
    crate::client_ip::Https(https): crate::client_ip::Https,
    request: Request,
) -> WebResult<Response> {
    let Path(path) = path.map_err(|_| WebError::NotFound)?;
    page.path = uri.path_and_query().map_or(uri.path(), |value| value.as_str()).to_owned();
    let route = lookup(&state, &uri, &path.plugin, &path.page, PageScope::Repository)?;
    let loaded = AuthorizedRepository::load(&state, &viewer, &path.owner, &path.repo, route.descriptor().presentation()).await?;
    match route.descriptor().audience() {
        PageAudience::RepositoryReader => {}
        PageAudience::RepositoryAdmin if loaded.ctx().access.can_admin() => {}
        _ => return Err(WebError::NotFound),
    }
    let session = request.extensions().get::<crate::extract::CurrentSession>().cloned();
    let browser = if forms::needs_browser(&route, &state) {
        browser_preference::BrowserForms::new(&request, https)
    } else {
        browser_preference::BrowserForms::empty(https)
    };
    let query = query_input::parse(uri.query(), route.descriptor().query())?;
    render(route, page, &viewer, session.as_ref(), Some(loaded), (&state, browser, query)).await
}

async fn render(
    route: PageRouteHandle,
    page: Page,
    viewer: &Viewer,
    session: Option<&crate::extract::CurrentSession>,
    repository: Option<AuthorizedRepository>,
    (state, mut browser, query): (&AppState, browser_preference::BrowserForms, rbit_services::plugin_pages::PageQuery),
) -> WebResult<Response> {
    let context = PageContext {
        actor: viewer.user().map(|user| PageActor { account_id: user.id().0, is_admin: user.is_admin }),
        repository: repository.as_ref().map(|loaded| PageRepository {
            repository_id: loaded.ctx().repo.repo.id.0,
            owner: loaded.ctx().repo.owner.name.to_string(),
            name: loaded.ctx().repo.repo.name.to_string(),
            can_admin: loaded.ctx().access.can_admin(),
        }),
        locale: page.locale.clone(),
    };
    let native_forms = !route.descriptor().forms().is_empty();
    let actions = forms::actions(&route, viewer, session, repository.as_ref(), state, &mut browser);
    let Ok(mut lease) = route.admit() else { return Ok(unavailable()) };
    let body = match lease.render_with_request(context, actions, query).await {
        Ok(body) => body,
        Err(PageError::Forbidden) => return Err(WebError::NotFound),
        Err(PageError::Invalid(_)) => return Err(WebError::BadRequest("Invalid plugin page request.".into())),
        Err(_) => return Ok(unavailable()),
    };
    let owner = route.owner().to_string();
    let path = route.descriptor().path().to_owned();
    let policy = ShellPolicy { native_forms, administrator: route.descriptor().audience() == PageAudience::SiteAdmin };
    let response = if route.descriptor().presentation() == rbit_services::plugin_pages::PagePresentation::Document {
        document(body)
    } else {
        let repository = repository.map(AuthorizedRepository::into_shell).transpose()?;
        shell(page, repository, owner, path, body, policy, state)
    };
    let (guard, cancellation) = lease.into_parts();
    let mut response = response;
    response.extensions_mut().insert(crate::middleware::PageResponseGuard::new(guard, cancellation));
    Ok(browser.apply(response))
}

fn document(body: rbit_services::plugin_pages::PageBody) -> Response {
    axum::response::Html(body.into_html()).into_response()
}

fn shell(
    page: Page,
    repository: Option<RepoPage>,
    owner: String,
    path: String,
    body: rbit_services::plugin_pages::PageBody,
    ShellPolicy { native_forms, administrator }: ShellPolicy,
    state: &AppState,
) -> Response {
    let page = page.titled(body.title());
    if let Some(repository) = repository {
        RepositoryPluginPage { page, header: repository.header(state, ""), owner, route: path, body, native_forms }.into_response()
    } else if administrator {
        AdminPluginPage { page, admin_nav: crate::views::admin::AdminSection::Plugins.nav(), owner, route: path, body, native_forms }
            .into_response()
    } else {
        GlobalPluginPage { page, owner, route: path, body, native_forms }.into_response()
    }
}

fn unavailable() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, [(header::CACHE_CONTROL, "no-store")], "Plugin page temporarily unavailable.").into_response()
}
