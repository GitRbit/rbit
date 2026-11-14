//! Fork creation, the forks list, and synchronizing a fork with its upstream.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically.

use axum::{
    Form, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use rbit_localization::RepoBrowseMessage;
use rbit_services::{
    ServiceError,
    forks::{ForkInput, ForkSyncExpectation},
};
use serde::Deserialize;

use super::{repo::RepoPage, repo_create::owner_options};
use crate::{
    AppState,
    error::WebResult,
    extract::{SignedIn, Viewer},
    urls::RepoUrls,
    views::{
        components::{FormError, RepoCard},
        forks::{ForkFormPage, ForkSyncPage, ForkSyncPanel, ForksPage},
        layout::Page,
        repo_text,
    },
};

/// "Fork owner/name", the fork form's title.
fn fork_title(loaded: &RepoPage) -> RepoBrowseMessage<'_> {
    RepoBrowseMessage::ForkTitle { owner: loaded.ctx.repo.owner.name.as_str(), name: loaded.ctx.repo.repo.name.as_str() }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/fork", get(fork_form).post(fork))
        .route("/{owner}/{repo}/forks", get(list))
        .route("/{owner}/{repo}/fork/sync", get(sync_panel).post(sync_fork))
}

/// The sync status is loaded after the repository's file tree.
async fn sync_panel(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    headers: HeaderMap,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<Response> {
    let fragment = headers.contains_key("hx-request") && !headers.contains_key("hx-boosted");
    let fork = state.services.repositories.open(Some(&user), &owner, &repo).await?;
    let urls = RepoUrls::new(fork.repo.owner.name.as_str(), fork.repo.repo.name.as_str());
    let Some(sync) = state.services.forks.sync_status(&user, &fork).await? else {
        return Ok(if fragment { StatusCode::NO_CONTENT.into_response() } else { Redirect::to(&urls.home()).into_response() });
    };
    let upstream = super::compare::head_spec(&fork, &sync.upstream.full_name(), &sync.expected.upstream_branch);
    let compare_url = urls.compare(&sync.expected.branch, &upstream);
    let response = if fragment {
        ForkSyncPanel { page, action_url: format!("{}/fork/sync", urls.home()), compare_url, sync }.into_response()
    } else {
        ForkSyncPage {
            page: repo_text::titled(page, rbit_localization::RepoBrowseLabel::SyncFork, &fork.repo.full_name()),
            action_url: format!("{}/fork/sync", urls.home()),
            home_url: urls.home(),
            compare_url,
            sync,
        }
        .into_response()
    };
    Ok(([("vary", "HX-Request, HX-Boosted")], response).into_response())
}

#[derive(Debug, Deserialize)]
struct SyncForm {
    branch: String,
    fork_tip: String,
    upstream_branch: String,
    upstream_tip: String,
}

async fn sync_fork(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo)): Path<(String, String)>,
    Form(input): Form<SyncForm>,
) -> WebResult<Redirect> {
    let fork = state.services.repositories.open(Some(&user), &owner, &repo).await?;
    let SyncForm { branch, fork_tip, upstream_branch, upstream_tip } = input;
    let expectation = ForkSyncExpectation { branch, fork_tip, upstream_branch, upstream_tip };
    state.services.forks.sync(&user, &fork, &expectation).await?;
    let urls = RepoUrls::new(fork.repo.owner.name.as_str(), fork.repo.repo.name.as_str());
    Ok(Redirect::to(&urls.home()))
}

/// `GET /{owner}/{repo}/fork`: owner and name, prefilled with the viewer and the
/// repository's name.
async fn fork_form(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<ForkFormPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    Ok(ForkFormPage {
        page: repo_text::titled_as(page, fork_title(&loaded)),
        header: loaded.header(&state, "code"),
        error: FormError::default(),
        owners: owner_options(&state, &user, "").await?,
        name: loaded.ctx.repo.repo.name.to_string(),
        action_url: format!("{}/fork", loaded.urls.home()),
    })
}

#[derive(Debug, Deserialize)]
pub struct ForkFormInput {
    #[serde(default)]
    owner: String,
    #[serde(default)]
    name: String,
}

/// `POST /{owner}/{repo}/fork`: redirects to the new fork.
async fn fork(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<ForkFormInput>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let input = ForkInput { owner: form.owner.clone(), name: form.name.clone() };
    match state.services.forks.fork(&user, &loaded.ctx, input).await {
        Ok(created) => Ok(Redirect::to(&format!("/{}/{}", created.owner.name, created.repo.name)).into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            let view = ForkFormPage {
                page: repo_text::titled_as(page, fork_title(&loaded)),
                header: loaded.header(&state, "code"),
                error: FormError::from_message(&error.to_string()),
                owners: owner_options(&state, &user, &form.owner).await?,
                name: form.name,
                action_url: format!("{}/fork", loaded.urls.home()),
            };
            Ok((StatusCode::UNPROCESSABLE_ENTITY, view).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `GET /{owner}/{repo}/forks`
async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<ForksPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let forks = state.services.forks.list(viewer.user(), &loaded.ctx).await?;
    let stars = state.services.stars.counts(&forks).await?;
    Ok(ForksPage {
        page: repo_text::titled(page, rbit_localization::RepoNavLabel::Forks, &loaded.ctx.repo.full_name()),
        header: loaded.header(&state, "code"),
        forks: RepoCard::list(&forks, &stars),
        fork_url: viewer.user().is_some().then(|| format!("{}/fork", loaded.urls.home())),
    })
}
