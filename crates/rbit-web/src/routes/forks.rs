//! Forking: `POST /{owner}/{repo}/fork` and the forks list `/{owner}/{repo}/forks`.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically.

use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use rbit_services::{ServiceError, forks::ForkInput};
use serde::Deserialize;

use super::{repo::RepoPage, repo_create::owner_options};
use crate::{
    AppState,
    error::WebResult,
    extract::{SignedIn, Viewer},
    views::{
        components::{FormError, RepoCard},
        forks::{ForkFormPage, ForksPage},
        layout::Page,
    },
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/{owner}/{repo}/fork", get(fork_form).post(fork)).route("/{owner}/{repo}/forks", get(list))
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
        page: page.titled(format!("Fork {}", loaded.ctx.repo.full_name())),
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
                page: page.titled(format!("Fork {}", loaded.ctx.repo.full_name())),
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
        page: page.titled(format!("Forks · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "code"),
        forks: RepoCard::list(&forks, &stars),
        fork_url: viewer.user().is_some().then(|| format!("{}/fork", loaded.urls.home())),
    })
}
