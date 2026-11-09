//! Repository wikis: `/{owner}/{repo}/wiki` (`W`).
//!
//! Human-readable reads use the page path (`W/{*path}`); every other operation is
//! keyed by the stable page ID under the reserved `W/-/` namespace (a page path can
//! never start with `-`). Reads need repository read access (anonymous on public
//! repositories), changes need write access to a repository that is not archived, and
//! turning the wiki on or off needs administration. A turned-off wiki is a 404 for
//! everyone but administrators, who are offered to turn it on. Every check is repeated
//! by the service and, for changes, in the publishing SQL transaction itself.
//!
//! A path a page had before a rename answers `308 Permanent Redirect` to the page's
//! current path, marked `no-store`: a new page may take the old path later, and then
//! that page is shown instead. The target is always a current page's own path, which
//! never redirects, so there are no redirect chains or loops.
//!
//! Merged into the page router with a body limit sized for the largest page.
use axum::{
    Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rbit_localization::WikiMessage;
use rbit_services::{
    ServiceError,
    wikis::{
        errors::WikiError,
        pages::{WikiLookup, WikiPageView, WikiStatus},
    },
};
use rbit_wikis::{MAX_WIKI_CONTENT_BYTES, WikiPageId, WikiPath};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    localization::RequestLocale,
    render::render_markdown,
    views::{
        layout::Page,
        wiki::{WikiChrome, WikiEntryView, WikiText, WikiUrls, crumbs, dates},
        wiki_pages::{WikiCreateLink, WikiIndexTemplate, WikiPageTemplate, WikiRevisionNote, WikiStateTemplate},
    },
};

#[path = "wiki_deleted.rs"]
mod deleted;
#[path = "wiki_edit.rs"]
mod edit;
#[path = "wiki_history.rs"]
mod history;

/// The page every wiki opens on, when it exists.
const HOME_PATH: &str = "Home";

pub fn routes() -> Router<AppState> {
    // Form encoding can expand each byte sixfold (including line endings).
    let limit = MAX_WIKI_CONTENT_BYTES.saturating_mul(6).saturating_add(16 * 1024);
    Router::new()
        .route("/{owner}/{repo}/wiki", get(home))
        .route("/{owner}/{repo}/wiki/-/pages", get(index))
        .route("/{owner}/{repo}/wiki/-/search", get(search))
        .route("/{owner}/{repo}/wiki/-/deleted", get(deleted::list))
        .route("/{owner}/{repo}/wiki/-/page/{id}/undelete", post(deleted::undelete))
        .route("/{owner}/{repo}/wiki/-/settings", post(edit::settings))
        .route("/{owner}/{repo}/wiki/-/new", get(edit::new_form).post(edit::create))
        .route("/{owner}/{repo}/wiki/-/page/{id}/edit", get(edit::edit_form).post(edit::update))
        .route("/{owner}/{repo}/wiki/-/page/{id}/delete", get(edit::delete_form).post(edit::delete))
        .route("/{owner}/{repo}/wiki/-/page/{id}/history", get(history::history))
        .route("/{owner}/{repo}/wiki/-/page/{id}/compare", get(history::compare))
        .route("/{owner}/{repo}/wiki/-/page/{id}/revisions/{generation}", get(history::revision))
        .route("/{owner}/{repo}/wiki/-/page/{id}/revisions/{generation}/restore", post(edit::restore))
        .route("/{owner}/{repo}/wiki/{*path}", get(page))
        .layer(DefaultBodyLimit::max(limit))
}

/// Unknown or malformed IDs are simply missing pages.
fn page_id(id: &str) -> WebResult<WikiPageId> {
    id.parse::<i64>().ok().and_then(WikiPageId::new).ok_or(WebError::NotFound)
}

pub(super) fn web_error(error: WikiError) -> WebError {
    ServiceError::from(error).into()
}

/// A loaded repository page with the wiki's text and URLs.
struct Opened {
    loaded: RepoPage,
    text: WikiText,
    urls: WikiUrls,
    locale: RequestLocale,
}

impl Opened {
    async fn load(state: &AppState, viewer: &Viewer, owner: &str, repo: &str, locale: RequestLocale) -> WebResult<Self> {
        let loaded = RepoPage::load(state, viewer, owner, repo).await?;
        let urls = WikiUrls::new(&loaded.urls.home());
        Ok(Self { loaded, text: WikiText::new(locale.0.locale), urls, locale })
    }

    fn can_write(&self) -> bool {
        self.loaded.ctx.can_write_content()
    }

    /// The chrome with the first page of the page list (`current` highlighted).
    async fn chrome(&self, state: &AppState, viewer: &Viewer, current: Option<WikiPageId>) -> WebResult<WikiChrome> {
        let list = state.services.wikis.pages(&self.loaded.ctx, viewer.user(), None).await.map_err(web_error)?;
        Ok(self.chrome_with(list.entries.iter().map(|entry| WikiEntryView::new(entry, &self.urls, current)).collect(), list.next.is_some()))
    }

    fn chrome_with(&self, sidebar: Vec<WikiEntryView>, more_pages: bool) -> WikiChrome {
        WikiChrome {
            t: self.text,
            urls: self.urls.clone(),
            sidebar,
            more_pages,
            can_write: self.can_write(),
            can_admin: self.loaded.ctx.access.can_admin() && !self.loaded.ctx.repo.repo.is_archived(),
        }
    }

    fn page(&self, page: Page, title: &str) -> Page {
        page.with_locale(self.locale.0.locale).titled(format!("{title} · {}", self.loaded.ctx.repo.full_name()))
    }

    fn respond(&self, response: impl IntoResponse) -> Response {
        self.locale.response(response.into_response())
    }

    /// A page read at its current head, or at an older revision with `revision`.
    fn page_view(
        &self,
        page: Page,
        chrome: WikiChrome,
        view: &WikiPageView,
        revision: Option<WikiRevisionNote>,
        state: &AppState,
    ) -> WikiPageTemplate {
        let (updated, updated_iso) = dates(view.page.updated_at, self.text.locale());
        let id = view.page.id;
        WikiPageTemplate {
            page: self.page(page, view.page.title.as_str()),
            header: self.loaded.header(state, "wiki"),
            wiki: chrome,
            title: view.page.title.to_string(),
            crumbs: crumbs(&view.page.path),
            html: render_markdown(view.content.as_str(), None),
            updated,
            updated_iso,
            edit_url: self.urls.action(id, "edit"),
            history_url: self.urls.action(id, "history"),
            delete_url: self.urls.action(id, "delete"),
            revision,
        }
    }

    /// The wiki is off: administrators may turn it on, everyone else sees nothing.
    fn turned_off(&self, state: &AppState, page: Page) -> WebResult<Response> {
        if !self.loaded.ctx.access.can_admin() || self.loaded.ctx.repo.repo.is_archived() {
            return Err(WebError::NotFound);
        }
        let template = WikiStateTemplate {
            page: self.page(page, self.text.text(WikiMessage::Tab).as_str()),
            header: self.loaded.header(state, "wiki"),
            wiki: self.chrome_with(Vec::new(), false),
            message: self.text.text(WikiMessage::Disabled),
            about: Some(self.text.text(WikiMessage::About)),
            create: None,
            enable_url: Some(self.urls.settings()),
        };
        Ok(self.respond(template))
    }

    async fn status(&self, state: &AppState) -> WebResult<WikiStatus> {
        state.services.wikis.status(&self.loaded.ctx).await.map_err(web_error)
    }
}

/// `GET W`: the `Home` page, or the page list when there is none.
async fn home(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<Response> {
    let opened = Opened::load(&state, &viewer, &owner, &repo, locale).await?;
    if opened.status(&state).await? == WikiStatus::Off {
        return opened.turned_off(&state, page);
    }
    let path: WikiPath = HOME_PATH.parse().map_err(|_| WebError::NotFound)?;
    match state.services.wikis.page(&opened.loaded.ctx, viewer.user(), &path).await.map_err(web_error)? {
        WikiLookup::Page(view) => {
            let chrome = opened.chrome(&state, &viewer, Some(view.page.id)).await?;
            Ok(opened.respond(opened.page_view(page, chrome, &view, None, &state)))
        }
        WikiLookup::Moved(moved) => Ok(moved_to(&opened, &moved)),
        WikiLookup::Missing { empty: true } => missing(&state, &viewer, page, &opened, None).await,
        WikiLookup::Missing { empty: false } => index_page(&state, &viewer, page, &opened, None).await,
    }
}

/// `GET W/{*path}`.
async fn page(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo, path)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let opened = Opened::load(&state, &viewer, &owner, &repo, locale).await?;
    let path: WikiPath = path.parse().map_err(|_| WebError::NotFound)?;
    if opened.status(&state).await? == WikiStatus::Off {
        return opened.turned_off(&state, page);
    }
    match state.services.wikis.page(&opened.loaded.ctx, viewer.user(), &path).await.map_err(web_error)? {
        WikiLookup::Page(view) => {
            let chrome = opened.chrome(&state, &viewer, Some(view.page.id)).await?;
            Ok(opened.respond(opened.page_view(page, chrome, &view, None, &state)))
        }
        WikiLookup::Moved(moved) => Ok(moved_to(&opened, &moved)),
        WikiLookup::Missing { .. } => missing(&state, &viewer, page, &opened, Some(&path)).await,
    }
}

/// An old path of a renamed page: a permanent redirect that is never cached, because
/// a new page may take the path later.
fn moved_to(opened: &Opened, path: &WikiPath) -> Response {
    let location = opened.urls.page(path);
    let headers = [(header::LOCATION, location.as_str()), (header::CACHE_CONTROL, "no-store")];
    (StatusCode::PERMANENT_REDIRECT, headers).into_response()
}

/// An empty wiki, or a path without a page (404), with a create link for writers.
async fn missing(state: &AppState, viewer: &Viewer, page: Page, opened: &Opened, path: Option<&WikiPath>) -> WebResult<Response> {
    let chrome = opened.chrome(state, viewer, None).await?;
    let text = opened.text;
    let create = opened.can_write().then(|| match path {
        Some(path) => WikiCreateLink {
            url: format!("{}?path={}", opened.urls.new_page(), crate::urls::encode_path(path.as_str())),
            label: text.text(WikiMessage::CreatePath { path: path.as_str() }),
        },
        None => WikiCreateLink { url: format!("{}?path={HOME_PATH}", opened.urls.new_page()), label: text.text(WikiMessage::CreateFirst) },
    });
    let template = WikiStateTemplate {
        page: opened.page(page, text.text(WikiMessage::Tab).as_str()),
        header: opened.loaded.header(state, "wiki"),
        wiki: chrome,
        message: text.text(if path.is_some() { WikiMessage::Missing } else { WikiMessage::Empty }),
        about: None,
        create,
        enable_url: None,
    };
    let status = if path.is_some() { StatusCode::NOT_FOUND } else { StatusCode::OK };
    Ok(opened.respond((status, template)))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct IndexQuery {
    after: String,
}

/// `GET W/-/pages?after=`.
async fn index(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<IndexQuery>,
) -> WebResult<Response> {
    let opened = Opened::load(&state, &viewer, &owner, &repo, locale).await?;
    let after = if query.after.is_empty() { None } else { Some(query.after.parse::<WikiPath>().map_err(|_| WebError::NotFound)?) };
    index_page(&state, &viewer, page, &opened, after.as_ref()).await
}

async fn index_page(state: &AppState, viewer: &Viewer, page: Page, opened: &Opened, after: Option<&WikiPath>) -> WebResult<Response> {
    let chrome = opened.chrome(state, viewer, None).await?;
    let list = state.services.wikis.pages(&opened.loaded.ctx, viewer.user(), after).await.map_err(web_error)?;
    let text = opened.text;
    let template = WikiIndexTemplate {
        page: opened.page(page, text.text(WikiMessage::AllPages).as_str()),
        header: opened.loaded.header(state, "wiki"),
        entries: list.entries.iter().map(|entry| WikiEntryView::new(entry, &opened.urls, None)).collect(),
        next_url: list.next.map(|next| format!("{}?after={}", opened.urls.pages(), crate::urls::encode_path(next.as_str()))),
        heading: text.text(WikiMessage::AllPages),
        empty: text.text(WikiMessage::Empty),
        show_settings: true,
        wiki: chrome,
    };
    Ok(opened.respond(template))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SearchQuery {
    q: String,
}

/// `GET W/-/search?q=`: titles and paths only.
async fn search(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    locale: RequestLocale,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<SearchQuery>,
) -> WebResult<Response> {
    let opened = Opened::load(&state, &viewer, &owner, &repo, locale).await?;
    let q: String = query.q.trim().chars().take(200).collect();
    let chrome = opened.chrome(&state, &viewer, None).await?;
    let found = state.services.wikis.search(&opened.loaded.ctx, viewer.user(), &q).await.map_err(web_error)?;
    let text = opened.text;
    let count = u64::try_from(found.len()).unwrap_or(u64::MAX);
    let template = WikiIndexTemplate {
        page: opened.page(page, text.text(WikiMessage::Search).as_str()),
        header: opened.loaded.header(&state, "wiki"),
        entries: found.iter().map(|entry| WikiEntryView::new(entry, &opened.urls, None)).collect(),
        next_url: None,
        heading: text.text(WikiMessage::SearchCount { count, query: &q }),
        empty: text.text(WikiMessage::NoResults { query: &q }),
        show_settings: false,
        wiki: chrome,
    };
    Ok(opened.respond(template))
}
