//! Releases: `/{owner}/{repo}/releases`, `/releases/tag/{tag}`, `/releases/new`, editing.
//!
//! File uploads and downloads are not here: see [`file_routes`].
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Readers see published releases; writers also
//! see drafts and get the editor, which is a 404 for everyone else.

use std::{io, time::Duration};

use axum::{
    Form, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{from_fn, from_fn_with_state},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use futures_util::TryStreamExt;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rustit_core::{User, release::ReleaseId};
use rustit_git::RefKind;
use rustit_services::{
    ServiceError,
    releases::{ReleaseDetail, ReleaseInput, human_bytes},
};
use serde::Deserialize;
use tokio_util::io::{ReaderStream, StreamReader};
use tower_http::timeout::TimeoutLayer;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    middleware,
    views::{
        components::{FormError, Pagination},
        layout::Page,
        releases::{ReleaseCard, ReleaseForm, ReleaseFormPage, ReleasePage, ReleasesPage},
    },
};

/// Room for the multipart framing around an uploaded file.
const UPLOAD_OVERHEAD_BYTES: u64 = 1024 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/releases", get(list))
        .route("/{owner}/{repo}/releases/tag/{*tag}", get(show))
        .route("/{owner}/{repo}/releases/new", get(new_form).post(create))
        .route("/{owner}/{repo}/releases/{id}/edit", get(edit_form).post(update))
        .route("/{owner}/{repo}/releases/{id}/delete", post(delete))
        .route("/{owner}/{repo}/releases/{id}/assets/{asset}/delete", post(delete_asset))
}

/// Attached-file uploads and downloads. Merged outside the page router (no 30-second
/// timeout, no 1 MiB body limit, no compression), so this router applies the session,
/// CSRF and security-header middleware itself.
pub fn file_routes(state: &AppState) -> Router<AppState> {
    let config = &state.config.releases;
    let body_limit = usize::try_from(config.max_asset_bytes.saturating_add(UPLOAD_OVERHEAD_BYTES)).unwrap_or(usize::MAX);
    let uploads = Router::new()
        .route("/{owner}/{repo}/releases/{id}/assets", post(upload))
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(config.upload_timeout_seconds)));
    Router::new()
        .route("/{owner}/{repo}/releases/download/{*rest}", get(download))
        .merge(uploads)
        .layer(from_fn_with_state(state.clone(), middleware::error_pages))
        .layer(from_fn_with_state(state.clone(), middleware::session))
        .layer(from_fn_with_state(state.clone(), middleware::rate_limit))
        .layer(from_fn(middleware::csrf_origin_check))
        .layer(from_fn(middleware::security_headers))
}

fn parse_id(id: &str) -> WebResult<ReleaseId> {
    id.parse().map(ReleaseId).map_err(|_| WebError::NotFound)
}

/// Loads the repository for a viewer who may write to it; 404 for everyone else.
async fn load_for_writer(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if !loaded.ctx.access.can_write() {
        return Err(WebError::NotFound);
    }
    Ok(loaded)
}

// ------------------------------------------------------------------ reading

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    page: Option<u32>,
}

/// `GET /{owner}/{repo}/releases[?page=n]`
async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> WebResult<ReleasesPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let releases = &state.services.releases;
    let (listing, latest) = tokio::join!(releases.list(&loaded.ctx, query.page.unwrap_or(1)), releases.latest(&loaded.ctx));
    let (listing, latest) = (listing?, latest?);

    let urls = &loaded.urls;
    let page_url = |n: u32| format!("{}?page={n}", urls.releases());
    let pagination = Pagination {
        page: listing.page,
        page_count: listing.page_count(),
        previous_url: (listing.page > 1).then(|| page_url(listing.page - 1)),
        next_url: (listing.page < listing.page_count()).then(|| page_url(listing.page + 1)),
    };
    Ok(ReleasesPage {
        page: page.titled(format!("Releases · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "releases"),
        releases: listing.releases.iter().map(|detail| ReleaseCard::new(detail, latest.as_ref(), urls)).collect(),
        pagination,
        can_write: loaded.ctx.access.can_write(),
        new_url: format!("{}/new", urls.releases()),
        tags_url: urls.tags(),
    })
}

#[derive(Debug, Deserialize)]
pub struct ShowQuery {
    uploaded: Option<String>,
    removed: Option<String>,
}

/// `GET /{owner}/{repo}/releases/tag/{tag}` (the tag may contain `/`).
async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, tag)): Path<(String, String, String)>,
    Query(query): Query<ShowQuery>,
) -> WebResult<ReleasePage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let detail = state.services.releases.by_tag(&loaded.ctx, &tag).await?;
    let notice = match (query.uploaded, query.removed) {
        (Some(name), _) => Some(format!("Uploaded {name}.")),
        (None, Some(name)) => Some(format!("Removed {name}.")),
        (None, None) => None,
    };
    release_page(&state, &loaded, page, &detail, notice, FormError::default()).await
}

async fn release_page(
    state: &AppState,
    loaded: &RepoPage,
    page: Page,
    detail: &ReleaseDetail,
    notice: Option<String>,
    error: FormError,
) -> WebResult<ReleasePage> {
    let latest = state.services.releases.latest(&loaded.ctx).await?;
    let urls = &loaded.urls;
    let id = detail.release.id.0;
    Ok(ReleasePage {
        page: page.titled(format!("{} · {}", detail.release.name, loaded.ctx.repo.full_name())),
        header: loaded.header(state, "releases"),
        release: ReleaseCard::new(detail, latest.as_ref(), urls),
        can_write: loaded.ctx.access.can_write(),
        releases_url: urls.releases(),
        delete_url: urls.release_action(id, "delete"),
        upload_url: urls.release_action(id, "assets"),
        max_asset_size: human_bytes(state.config.releases.max_asset_bytes),
        notice,
        error,
    })
}

// ------------------------------------------------------------------ the editor

#[derive(Debug, Deserialize)]
pub struct ReleaseFormInput {
    #[serde(default)]
    tag: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    notes: String,
    prerelease: Option<String>,
    /// For a tag that does not exist yet: the branch to create it from.
    #[serde(default)]
    target: String,
    /// `draft` saves a draft; anything else publishes.
    #[serde(default)]
    action: String,
}

impl ReleaseFormInput {
    fn into_input(self) -> ReleaseInput {
        ReleaseInput {
            tag_name: self.tag,
            name: self.name,
            notes: self.notes.replace("\r\n", "\n"),
            is_draft: self.action == "draft",
            is_prerelease: self.prerelease.is_some(),
            target: self.target,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct NewQuery {
    tag: Option<String>,
}

/// Existing tags, most recently committed first.
async fn tag_names(loaded: &RepoPage) -> WebResult<Vec<String>> {
    let tags = loaded.ctx.reader.ref_details(RefKind::Tag).await.map_err(ServiceError::from)?;
    Ok(tags.into_iter().map(|t| t.name).collect())
}

async fn form_page(
    state: &AppState,
    loaded: &RepoPage,
    page: Page,
    form: ReleaseForm,
    editing: Option<(ReleaseId, bool)>,
) -> WebResult<ReleaseFormPage> {
    let urls = &loaded.urls;
    let branches = loaded.refs().iter().filter(|r| r.kind == RefKind::Branch).map(|r| r.name.clone()).collect();
    let (title, action_url, cancel_url) = match editing {
        None => ("New release".to_owned(), format!("{}/new", urls.releases()), urls.releases()),
        Some((id, _)) => (format!("Edit {}", form.tag), urls.release_action(id.0, "edit"), urls.release(&form.tag)),
    };
    Ok(ReleaseFormPage {
        page: page.titled(format!("{title} · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "releases"),
        error: FormError::default(),
        tags: tag_names(loaded).await?,
        branches,
        action_url,
        cancel_url,
        is_new: editing.is_none(),
        is_published: editing.is_some_and(|(_, published)| published),
        tags_url: urls.tags(),
        form,
    })
}

fn form_values(input: &ReleaseInput) -> ReleaseForm {
    ReleaseForm {
        tag: input.tag_name.clone(),
        name: input.name.clone(),
        notes: input.notes.clone(),
        is_draft: input.is_draft,
        is_prerelease: input.is_prerelease,
        target: input.target.clone(),
    }
}

/// `GET /{owner}/{repo}/releases/new[?tag=…]`
async fn new_form(
    State(state): State<AppState>,
    SignedIn(_user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<NewQuery>,
) -> WebResult<ReleaseFormPage> {
    let loaded = load_for_writer(&state, &viewer, &owner, &repo).await?;
    let form = ReleaseForm { target: loaded.default_branch().to_owned(), ..ReleaseForm::default() };
    let mut form_page = form_page(&state, &loaded, page, form, None).await?;
    form_page.form.tag = query.tag.unwrap_or_else(|| form_page.tags.first().cloned().unwrap_or_default());
    Ok(form_page)
}

/// `POST /{owner}/{repo}/releases/new`: redirects to the new release.
async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<ReleaseFormInput>,
) -> WebResult<Response> {
    let loaded = load_for_writer(&state, &viewer, &owner, &repo).await?;
    let input = form.into_input();
    match state.services.releases.create(&loaded.ctx, &user, input.clone()).await {
        Ok(release) => Ok(Redirect::to(&loaded.urls.release(&release.tag_name)).into_response()),
        Err(error) => rejected_form(&state, &loaded, page, &input, None, error).await,
    }
}

async fn rejected_form(
    state: &AppState,
    loaded: &RepoPage,
    page: Page,
    input: &ReleaseInput,
    editing: Option<(ReleaseId, bool)>,
    error: ServiceError,
) -> WebResult<Response> {
    match error {
        ServiceError::Invalid(_) | ServiceError::Conflict(_) => {
            let mut view = form_page(state, loaded, page, form_values(input), editing).await?;
            view.error = FormError::from_message(&error.to_string());
            Ok((StatusCode::UNPROCESSABLE_ENTITY, view).into_response())
        }
        other => Err(other.into()),
    }
}

/// `GET /{owner}/{repo}/releases/{id}/edit`
async fn edit_form(
    State(state): State<AppState>,
    SignedIn(_user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
) -> WebResult<ReleaseFormPage> {
    let loaded = load_for_writer(&state, &viewer, &owner, &repo).await?;
    let release = state.services.releases.by_id(&loaded.ctx, parse_id(&id)?).await?.release;
    let form = ReleaseForm {
        tag: release.tag_name.clone(),
        name: release.name.clone(),
        notes: release.notes.clone(),
        is_draft: release.is_draft,
        is_prerelease: release.is_prerelease,
        target: String::new(),
    };
    form_page(&state, &loaded, page, form, Some((release.id, !release.is_draft))).await
}

/// `POST /{owner}/{repo}/releases/{id}/edit`: redirects to the release.
async fn update(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Form(form): Form<ReleaseFormInput>,
) -> WebResult<Response> {
    let loaded = load_for_writer(&state, &viewer, &owner, &repo).await?;
    let current = state.services.releases.by_id(&loaded.ctx, parse_id(&id)?).await?.release;
    let input = form.into_input();
    match state.services.releases.update(&loaded.ctx, &user, current.id, input.clone()).await {
        Ok(release) => Ok(Redirect::to(&loaded.urls.release(&release.tag_name)).into_response()),
        Err(error) => rejected_form(&state, &loaded, page, &input, Some((current.id, !current.is_draft)), error).await,
    }
}

/// `POST /{owner}/{repo}/releases/{id}/delete`: deletes the release (not the tag).
async fn delete(
    State(state): State<AppState>,
    SignedIn(_user): SignedIn,
    viewer: Viewer,
    Path((owner, repo, id)): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    let loaded = load_for_writer(&state, &viewer, &owner, &repo).await?;
    state.services.releases.delete(&loaded.ctx, parse_id(&id)?).await?;
    Ok(Redirect::to(&loaded.urls.releases()))
}

/// `POST /{owner}/{repo}/releases/{id}/assets/{asset}/delete`
async fn delete_asset(
    State(state): State<AppState>,
    SignedIn(_user): SignedIn,
    viewer: Viewer,
    Path((owner, repo, id, asset)): Path<(String, String, String, String)>,
) -> WebResult<Redirect> {
    let loaded = load_for_writer(&state, &viewer, &owner, &repo).await?;
    let detail = state.services.releases.by_id(&loaded.ctx, parse_id(&id)?).await?;
    let asset_id = asset.parse().map(rustit_core::release::ReleaseAssetId).map_err(|_| WebError::NotFound)?;
    let name = detail.assets.iter().find(|a| a.id == asset_id).map(|a| a.name.clone()).ok_or(WebError::NotFound)?;
    state.services.releases.delete_asset(&loaded.ctx, detail.release.id, asset_id).await?;
    Ok(Redirect::to(&format!("{}?removed={}", loaded.urls.release(&detail.release.tag_name), utf8_percent_encode(&name, NON_ALPHANUMERIC))))
}

// ------------------------------------------------------------------ files

/// `POST /{owner}/{repo}/releases/{id}/assets` (`multipart/form-data`, one `file`
/// field): streams the file to disk, then redirects to the release.
async fn upload(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
    mut multipart: Multipart,
) -> WebResult<Response> {
    let loaded = load_for_writer(&state, &viewer, &owner, &repo).await?;
    let detail = state.services.releases.by_id(&loaded.ctx, parse_id(&id)?).await?;

    let mut result = Err(ServiceError::Invalid("choose a file to upload".to_owned()));
    while let Some(field) = multipart.next_field().await.map_err(|e| WebError::BadRequest(e.body_text()))? {
        if field.name() != Some("file") {
            continue;
        }
        let name = field.file_name().map(file_name_only).unwrap_or_default();
        if name.is_empty() {
            break;
        }
        let content_type = field.content_type().unwrap_or_default().to_owned();
        let body = StreamReader::new(field.map_err(io::Error::other));
        result = upload_field(&state, &loaded, &user, &detail, &name, &content_type, body).await;
        break;
    }

    match result {
        Ok(name) => Ok(Redirect::to(&format!(
            "{}?uploaded={}",
            loaded.urls.release(&detail.release.tag_name),
            utf8_percent_encode(&name, NON_ALPHANUMERIC)
        ))
        .into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            let view = release_page(&state, &loaded, page, &detail, None, FormError::from_message(&error.to_string())).await?;
            Ok((StatusCode::UNPROCESSABLE_ENTITY, view).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

async fn upload_field(
    state: &AppState,
    loaded: &RepoPage,
    user: &User,
    detail: &ReleaseDetail,
    name: &str,
    content_type: &str,
    body: impl tokio::io::AsyncRead + Unpin + Send,
) -> Result<String, ServiceError> {
    let asset = state.services.releases.upload_asset(&loaded.ctx, user, detail.release.id, name, content_type, body).await?;
    Ok(asset.name)
}

/// Browsers may send a full path (`C:\Users\…\app.zip`); keep the last component.
fn file_name_only(name: &str) -> String {
    name.rsplit(['/', '\\']).next().unwrap_or_default().trim().to_owned()
}

/// `GET /{owner}/{repo}/releases/download/{tag}/{name}`: the tag may contain `/`, the
/// name never does, so the last segment is the name.
async fn download(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let (tag, name) = rest.rsplit_once('/').ok_or(WebError::NotFound)?;
    let ctx = state.services.repositories.open(viewer.user(), &owner, &repo).await?;
    let download = state.services.releases.open_asset(&ctx, tag, name).await?;
    let asset = &download.asset;

    let disposition = format!("attachment; filename*=UTF-8''{}", utf8_percent_encode(&asset.name, NON_ALPHANUMERIC));
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream")),
        (header::CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).map_err(WebError::internal)?),
        (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        (header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'")),
        (header::CONTENT_LENGTH, HeaderValue::from(asset.size)),
        (header::ETAG, HeaderValue::from_str(&format!("\"{}\"", asset.sha256_hex())).map_err(WebError::internal)?),
    ];
    Ok((headers, Body::from_stream(ReaderStream::new(download.file))).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uploaded_names_lose_their_directories() {
        assert_eq!(file_name_only(r"C:\Users\ada\app.zip"), "app.zip");
        assert_eq!(file_name_only("dir/app.zip"), "app.zip");
        assert_eq!(file_name_only("app.zip"), "app.zip");
        assert_eq!(file_name_only("dir/"), "");
    }
}
