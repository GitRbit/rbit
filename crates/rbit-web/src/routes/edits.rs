//! Editing files in the browser: `/{owner}/{repo}/new|edit|delete|upload/{branch}/{path…}`.
//!
//! Milestone 3 workstream E (editing files in the browser) owns this file.
//!
//! Every page needs write access to a repository that is not a mirror and an existing
//! branch; everyone else gets a 404 (readers never learn the pages exist). Each form
//! carries the commit it was opened at; the commit is made by
//! [`rbit_services::EditService`] and lands on the branch, or on a new branch followed by a
//! redirect to the new pull request page. A refused commit re-renders the form with what
//! was typed: `422` with the reason, `409` when the branch moved (with the new tip filled
//! in, so committing again is a deliberate step after reviewing).
//!
//! [`routes`] are merged into the page router with a body limit of their own (a textarea
//! of up to `[repository] max_render_bytes`, form-encoded). Uploads stream and may take
//! long, so [`upload_routes`] are merged outside it, like release files, and apply the
//! session, CSRF and security-header middleware themselves.

use std::{io, time::Duration};

use axum::{
    Form, Router,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::StatusCode,
    middleware::{from_fn, from_fn_with_state},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use futures_util::TryStreamExt;
use rbit_core::{User, file_edit::EditAction};
use rbit_git::RefKind;
use rbit_services::{
    ServiceError,
    edits::{CommitInput, CommitTarget, EditAccess, EditFailure, Edited},
    releases::human_bytes,
};
use serde::Deserialize;
use tokio_util::io::StreamReader;
use tower_http::timeout::TimeoutLayer;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    middleware,
    views::{
        components::FormError,
        edits::{CommitBox, Conflict, DeleteFilePage, EditFilePage, UploadFilesPage},
        layout::Page,
        pulls::new_pull_url,
    },
};

/// Room for the form encoding around the text: percent-encoding triples a byte, and every
/// line break may arrive as `%0D%0A`.
const TEXT_ENCODING_FACTOR: usize = 6;
/// Room for the other fields of the editor form and of multipart framing.
const FORM_OVERHEAD_BYTES: usize = 256 * 1024;
/// Longest value of a text field in the upload form.
const MAX_UPLOAD_FIELD_BYTES: usize = 128 * 1024;

pub fn routes(state: &AppState) -> Router<AppState> {
    let limit = state.services.edits.max_text_bytes().saturating_mul(TEXT_ENCODING_FACTOR).saturating_add(FORM_OVERHEAD_BYTES);
    Router::new()
        .route("/{owner}/{repo}/new/{*rest}", get(new_form).post(create))
        .route("/{owner}/{repo}/edit/{*rest}", get(edit_form).post(update))
        .route("/{owner}/{repo}/delete/{*rest}", get(delete_form).post(delete))
        // Inner layers win: this replaces the page router's 1 MiB limit for these routes.
        .layer(DefaultBodyLimit::max(limit))
}

/// The upload form and its streaming `POST`, outside the page router (no 30-second
/// timeout, no 1 MiB body limit, no compression).
pub fn upload_routes(state: &AppState) -> Router<AppState> {
    let config = &state.config.editor;
    let files = config.max_upload_bytes.saturating_mul(u64::try_from(config.max_upload_files.max(1)).unwrap_or(u64::MAX));
    let limit = usize::try_from(files).unwrap_or(usize::MAX).saturating_add(FORM_OVERHEAD_BYTES);
    Router::new()
        .route("/{owner}/{repo}/upload/{*rest}", get(upload_form).post(upload))
        .layer(DefaultBodyLimit::max(limit))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(config.upload_timeout_seconds.max(1))))
        .layer(from_fn_with_state(state.clone(), middleware::error_pages))
        .layer(from_fn_with_state(state.clone(), middleware::session))
        .layer(from_fn_with_state(state.clone(), middleware::rate_limit))
        .layer(from_fn(middleware::csrf_origin_check))
        .layer(from_fn(middleware::security_headers))
}

// ------------------------------------------------------------------ shared

/// The repository, branch and path a page edits, for a viewer who may edit them.
struct Target {
    loaded: RepoPage,
    branch: String,
    path: String,
    /// The branch's tip as the page was loaded.
    tip: String,
}

impl Target {
    /// 404 unless the viewer may edit the repository and `rest` starts with a branch.
    async fn load(state: &AppState, viewer: &Viewer, owner: &str, repo: &str, rest: String) -> WebResult<Self> {
        let loaded = RepoPage::load(state, viewer, owner, repo).await?;
        if !state.services.edits.can_edit(&loaded.ctx).await? {
            return Err(WebError::NotFound);
        }
        let (branch, path) = loaded.split(rest).await?;
        let tip = loaded.refs().iter().find(|r| r.kind == RefKind::Branch && r.name == branch).map(|r| r.target.clone());
        let tip = tip.ok_or(WebError::NotFound)?;
        Ok(Self { loaded, branch, path, tip })
    }

    fn header(&self, state: &AppState) -> crate::views::repo::RepoHeader {
        self.loaded.header(state, "code")
    }

    /// `/{owner}/{repo}/{action}/{branch}/{path}`.
    fn action_url(&self, action: &str) -> String {
        self.loaded.urls.edit(action, &self.branch, &self.path)
    }
}

/// The fields every edit form posts (each form uses some of them).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct FileForm {
    path: String,
    content: String,
    /// The commit the form was opened at.
    commit: String,
    title: String,
    description: String,
    /// `branch` (commit directly) or `new` (to `new_branch`).
    target: String,
    new_branch: String,
}

impl FileForm {
    fn input(&self, target: &Target) -> CommitInput {
        CommitInput {
            branch: target.branch.clone(),
            parent: self.commit.trim().to_owned(),
            target: if self.target == "new" { CommitTarget::NewBranch(self.new_branch.trim().to_owned()) } else { CommitTarget::Branch },
            title: self.title.clone(),
            description: self.description.clone(),
        }
    }
}

/// What the commit box shows after a refused commit.
struct Retry {
    status: StatusCode,
    error: FormError,
    conflict: Option<Conflict>,
    /// The commit the form is now based on.
    parent: String,
    protected_reason: Option<String>,
}

/// The commit box for `user` on `target`: the suggested branch name, and the direct
/// option only when protection allows it.
async fn commit_box(
    state: &AppState,
    user: &User,
    target: &Target,
    title_placeholder: String,
    form: Option<&FileForm>,
    retry: Option<&Retry>,
) -> WebResult<CommitBox> {
    let edits = &state.services.edits;
    let protected_reason = match retry.and_then(|retry| retry.protected_reason.clone()) {
        Some(reason) => Some(reason),
        None => match edits.access(user, &target.loaded.ctx, &target.branch).await.map_err(web_error)? {
            EditAccess::Direct => None,
            EditAccess::NewBranchOnly(reason) => Some(reason),
        },
    };
    let new_branch = match form.map(|form| form.new_branch.trim()).filter(|name| !name.is_empty()) {
        Some(name) => name.to_owned(),
        None => edits.suggested_branch(user, &target.loaded.ctx).await?,
    };
    Ok(CommitBox {
        title: form.map(|form| form.title.clone()).unwrap_or_default(),
        title_placeholder,
        description: form.map(|form| form.description.clone()).unwrap_or_default(),
        branch: target.branch.clone(),
        parent: retry.map_or_else(|| target.tip.clone(), |retry| retry.parent.clone()),
        new_branch_selected: form.is_some_and(|form| form.target == "new") || protected_reason.is_some(),
        protected_reason,
        new_branch,
    })
}

/// A failure that is not shown on the form.
fn web_error(failure: EditFailure) -> WebError {
    match failure {
        EditFailure::Service(error) => error.into(),
        other => WebError::BadRequest(other.to_string()),
    }
}

/// Where a successful commit leads: the new pull request page for a new branch,
/// otherwise `done`.
fn finished(target: &Target, edited: &Edited, done: &str) -> Response {
    if edited.new_branch {
        let repo = &target.loaded.ctx.repo;
        let url = new_pull_url(repo.owner.name.as_str(), repo.repo.name.as_str(), &target.branch, &edited.branch);
        return Redirect::to(&url).into_response();
    }
    Redirect::to(done).into_response()
}

/// The re-rendered form's state after `failure`, or the error page.
fn retry(target: &Target, form: &FileForm, failure: EditFailure) -> WebResult<Retry> {
    let base = Retry {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        error: FormError::default(),
        conflict: None,
        parent: form.commit.clone(),
        protected_reason: None,
    };
    Ok(match failure {
        EditFailure::Invalid(message) | EditFailure::Service(ServiceError::Invalid(message)) => {
            Retry { error: FormError::from_message(&message), ..base }
        }
        EditFailure::Protected(reason) => Retry {
            error: FormError::new(format!(
                "You cannot commit to {} directly: {reason}. Commit to a new branch and open a pull request instead.",
                target.branch
            )),
            protected_reason: Some(reason),
            ..base
        },
        EditFailure::BranchMoved { current } => Retry {
            status: StatusCode::CONFLICT,
            conflict: Some(Conflict {
                branch: target.branch.clone(),
                current_short: current.as_deref().map(|id| id.chars().take(7).collect()),
                history_url: target.loaded.urls.commits(&target.branch),
            }),
            parent: current.unwrap_or_else(|| form.commit.clone()),
            ..base
        },
        EditFailure::Service(error) => return Err(error.into()),
    })
}

fn max_text_size(state: &AppState) -> String {
    human_bytes(u64::try_from(state.services.edits.max_text_bytes()).unwrap_or(u64::MAX))
}

// ------------------------------------------------------------------ new file

/// `GET /{owner}/{repo}/new/{branch}/{directory…}`
async fn new_form(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<EditFilePage> {
    let target = Target::load(&state, &viewer, &owner, &repo, rest).await?;
    let directory = if target.path.is_empty() { String::new() } else { format!("{}/", target.path) };
    new_page(&state, &user, page, &target, None, None, directory, String::new()).await
}

/// `POST /{owner}/{repo}/new/{branch}/{directory…}`
async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
    Form(form): Form<FileForm>,
) -> WebResult<Response> {
    let target = Target::load(&state, &viewer, &owner, &repo, rest).await?;
    let result = state.services.edits.create_file(&user, &target.loaded.ctx, &form.input(&target), &form.path, &form.content).await;
    match result {
        Ok(edited) => Ok(finished(&target, &edited, &target.loaded.urls.blob(&edited.branch, form.path.trim()))),
        Err(failure) => {
            let retry = retry(&target, &form, failure)?;
            let status = retry.status;
            let view = new_page(&state, &user, page, &target, Some(&form), Some(&retry), form.path.clone(), form.content.clone()).await?;
            Ok((status, view).into_response())
        }
    }
}

#[expect(clippy::too_many_arguments, reason = "the page's parts, all needed together")]
async fn new_page(
    state: &AppState,
    user: &User,
    page: Page,
    target: &Target,
    form: Option<&FileForm>,
    retry: Option<&Retry>,
    path: String,
    content: String,
) -> WebResult<EditFilePage> {
    let placeholder = EditAction::Create(if path.trim().is_empty() { "a file" } else { path.trim() }).default_title();
    let commit = commit_box(state, user, target, placeholder, form, retry).await?;
    let preview_url = markdown_preview_url(target);
    Ok(EditFilePage {
        page: page.titled(format!("New file · {}", target.loaded.ctx.repo.full_name())),
        header: target.header(state),
        is_new: true,
        original_path: String::new(),
        action_url: target.action_url("new"),
        cancel_url: target.loaded.urls.tree(&target.branch, &target.path),
        path,
        content,
        not_editable: None,
        preview_url,
        max_size: max_text_size(state),
        commit,
        conflict: retry.and_then(|retry| retry.conflict.clone()),
        error: retry.map(|retry| retry.error.clone()).unwrap_or_default(),
    })
}

/// Every editor retains context, so typing or changing a Markdown filename enables previews.
fn markdown_preview_url(target: &Target) -> String {
    format!(
        "/issues/markdown-preview?owner={}&repo={}&revision={}",
        target.loaded.ctx.repo.owner.name,
        target.loaded.ctx.repo.repo.name,
        percent_encoding::utf8_percent_encode(&target.branch, percent_encoding::NON_ALPHANUMERIC)
    )
}

// ------------------------------------------------------------------ edit

/// `GET /{owner}/{repo}/edit/{branch}/{path…}`
async fn edit_form(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<EditFilePage> {
    let target = Target::load(&state, &viewer, &owner, &repo, rest).await?;
    if target.path.is_empty() {
        return Err(WebError::NotFound);
    }
    let (content, not_editable) = match state.services.edits.editable_text(&target.loaded.ctx, &target.tip, &target.path).await {
        Ok(bytes) => (String::from_utf8(bytes).unwrap_or_default(), None),
        Err(EditFailure::Invalid(_)) => {
            // A directory, or a path that does not exist: no editor for it.
            if target.loaded.ctx.reader.blob(target.tip.clone(), target.path.clone(), 0).await.is_err() {
                return Err(WebError::NotFound);
            }
            let limit = max_text_size(&state);
            (
                String::new(),
                Some(format!(
                    "This file is binary or larger than {limit}, so it cannot be edited in the browser. You can delete it, or upload a replacement."
                )),
            )
        }
        Err(failure) => return Err(web_error(failure)),
    };
    let path = target.path.clone();
    edit_page(&state, &user, page, &target, None, None, path, content, not_editable).await
}

/// `POST /{owner}/{repo}/edit/{branch}/{path…}`
async fn update(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
    Form(form): Form<FileForm>,
) -> WebResult<Response> {
    let target = Target::load(&state, &viewer, &owner, &repo, rest).await?;
    if target.path.is_empty() {
        return Err(WebError::NotFound);
    }
    let input = form.input(&target);
    let result = state.services.edits.update_file(&user, &target.loaded.ctx, &input, &target.path, &form.path, &form.content).await;
    match result {
        Ok(edited) => {
            let path = if form.path.trim().is_empty() { target.path.clone() } else { form.path.trim().to_owned() };
            Ok(finished(&target, &edited, &target.loaded.urls.blob(&edited.branch, &path)))
        }
        Err(failure) => {
            let retry = retry(&target, &form, failure)?;
            let status = retry.status;
            let view =
                edit_page(&state, &user, page, &target, Some(&form), Some(&retry), form.path.clone(), form.content.clone(), None).await?;
            Ok((status, view).into_response())
        }
    }
}

#[expect(clippy::too_many_arguments, reason = "the page's parts, all needed together")]
async fn edit_page(
    state: &AppState,
    user: &User,
    page: Page,
    target: &Target,
    form: Option<&FileForm>,
    retry: Option<&Retry>,
    path: String,
    content: String,
    not_editable: Option<String>,
) -> WebResult<EditFilePage> {
    let placeholder = EditAction::Update(&target.path).default_title();
    let commit = commit_box(state, user, target, placeholder, form, retry).await?;
    let preview_url = markdown_preview_url(target);
    Ok(EditFilePage {
        page: page.titled(format!("Editing {} · {}", target.path, target.loaded.ctx.repo.full_name())),
        header: target.header(state),
        is_new: false,
        original_path: target.path.clone(),
        action_url: target.action_url("edit"),
        cancel_url: target.loaded.urls.blob(&target.branch, &target.path),
        path,
        content,
        not_editable,
        preview_url,
        max_size: max_text_size(state),
        commit,
        conflict: retry.and_then(|retry| retry.conflict.clone()),
        error: retry.map(|retry| retry.error.clone()).unwrap_or_default(),
    })
}

// ------------------------------------------------------------------ delete

/// `GET /{owner}/{repo}/delete/{branch}/{path…}`: the confirmation.
async fn delete_form(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<DeleteFilePage> {
    let target = Target::load(&state, &viewer, &owner, &repo, rest).await?;
    if target.path.is_empty() || target.loaded.ctx.reader.blob(target.tip.clone(), target.path.clone(), 0).await.is_err() {
        return Err(WebError::NotFound);
    }
    delete_page(&state, &user, page, &target, None, None).await
}

/// `POST /{owner}/{repo}/delete/{branch}/{path…}`
async fn delete(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
    Form(form): Form<FileForm>,
) -> WebResult<Response> {
    let target = Target::load(&state, &viewer, &owner, &repo, rest).await?;
    if target.path.is_empty() {
        return Err(WebError::NotFound);
    }
    match state.services.edits.delete_file(&user, &target.loaded.ctx, &form.input(&target), &target.path).await {
        Ok(edited) => {
            let directory = existing_directory(&target, &edited.branch, parent_of(&target.path)).await;
            Ok(finished(&target, &edited, &target.loaded.urls.tree(&edited.branch, &directory)))
        }
        Err(failure) => {
            let retry = retry(&target, &form, failure)?;
            let status = retry.status;
            Ok((status, delete_page(&state, &user, page, &target, Some(&form), Some(&retry)).await?).into_response())
        }
    }
}

async fn delete_page(
    state: &AppState,
    user: &User,
    page: Page,
    target: &Target,
    form: Option<&FileForm>,
    retry: Option<&Retry>,
) -> WebResult<DeleteFilePage> {
    let placeholder = EditAction::Delete(&target.path).default_title();
    let commit = commit_box(state, user, target, placeholder, form, retry).await?;
    Ok(DeleteFilePage {
        page: page.titled(format!("Delete {} · {}", target.path, target.loaded.ctx.repo.full_name())),
        header: target.header(state),
        path: target.path.clone(),
        action_url: target.action_url("delete"),
        cancel_url: target.loaded.urls.blob(&target.branch, &target.path),
        commit,
        conflict: retry.and_then(|retry| retry.conflict.clone()),
        error: retry.map(|retry| retry.error.clone()).unwrap_or_default(),
    })
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// `directory` or its nearest ancestor that still exists on `branch` (deleting a
/// directory's last file removes the directory).
async fn existing_directory(target: &Target, branch: &str, directory: &str) -> String {
    let mut directory = directory.to_owned();
    while !directory.is_empty() && target.loaded.ctx.reader.tree(branch.to_owned(), directory.clone()).await.is_err() {
        directory = parent_of(&directory).to_owned();
    }
    directory
}

// ------------------------------------------------------------------ upload

/// `GET /{owner}/{repo}/upload/{branch}/{directory…}`
async fn upload_form(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<UploadFilesPage> {
    let target = Target::load(&state, &viewer, &owner, &repo, rest).await?;
    if !target.path.is_empty() && target.loaded.ctx.reader.tree(target.tip.clone(), target.path.clone()).await.is_err() {
        return Err(WebError::NotFound);
    }
    upload_page(&state, &user, page, &target, None, None).await
}

/// `POST /{owner}/{repo}/upload/{branch}/{directory…}` (`multipart/form-data`): every
/// `files` part streams into the upload as it arrives; the other fields are the commit
/// form's.
async fn upload(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
    mut multipart: Multipart,
) -> WebResult<Response> {
    let target = Target::load(&state, &viewer, &owner, &repo, rest).await?;
    let mut upload = state.services.edits.start_upload(&target.loaded.ctx).await.map_err(web_error)?;
    let mut form = FileForm::default();
    let mut failure = None;
    while let Some(field) = multipart.next_field().await.map_err(|error| WebError::BadRequest(error.body_text()))? {
        let name = field.name().unwrap_or_default().to_owned();
        if name == "files" {
            let file_name = field.file_name().unwrap_or_default().to_owned();
            let body = StreamReader::new(field.map_err(io::Error::other));
            if let Err(error) = upload.add(&file_name, body).await {
                failure = Some(error);
                break;
            }
            continue;
        }
        let value = field.text().await.map_err(|error| WebError::BadRequest(error.body_text()))?;
        if value.len() > MAX_UPLOAD_FIELD_BYTES {
            return Err(WebError::BadRequest(format!("the {name} field is too long")));
        }
        match name.as_str() {
            "commit" => form.commit = value,
            "title" => form.title = value,
            "description" => form.description = value,
            "target" => form.target = value,
            "new_branch" => form.new_branch = value,
            _ => {}
        }
    }
    let result = match failure {
        Some(failure) => Err(failure),
        None => state.services.edits.finish_upload(&user, &target.loaded.ctx, &form.input(&target), upload, &target.path).await,
    };
    match result {
        Ok(edited) => Ok(finished(&target, &edited, &target.loaded.urls.tree(&edited.branch, &target.path))),
        Err(failure) => {
            let retry = retry(&target, &form, failure)?;
            let status = retry.status;
            Ok((status, upload_page(&state, &user, page, &target, Some(&form), Some(&retry)).await?).into_response())
        }
    }
}

async fn upload_page(
    state: &AppState,
    user: &User,
    page: Page,
    target: &Target,
    form: Option<&FileForm>,
    retry: Option<&Retry>,
) -> WebResult<UploadFilesPage> {
    let commit = commit_box(state, user, target, EditAction::Upload.default_title(), form, retry).await?;
    let config = &state.config.editor;
    Ok(UploadFilesPage {
        page: page.titled(format!("Upload files · {}", target.loaded.ctx.repo.full_name())),
        header: target.header(state),
        directory: if target.path.is_empty() { "/".to_owned() } else { format!("{}/", target.path) },
        action_url: target.action_url("upload"),
        cancel_url: target.loaded.urls.tree(&target.branch, &target.path),
        max_file_size: human_bytes(config.max_upload_bytes),
        max_files: config.max_upload_files.max(1),
        commit,
        conflict: retry.and_then(|retry| retry.conflict.clone()),
        error: retry.map(|retry| retry.error.clone()).unwrap_or_default(),
    })
}
