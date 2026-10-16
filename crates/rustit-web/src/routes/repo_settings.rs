//! Repository settings: `/{owner}/{repo}/settings`.
//!
//! Workstream D owns this file. Its routes are merged into the page router, so they get
//! sessions, CSRF protection, security headers and error pages automatically.
//!
//! Only repository administrators may see these pages; everyone else gets a 404.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rustit_core::Visibility;
use rustit_git::RefKind;
use rustit_services::{ServiceError, repo_settings::UpdateRepository};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    views::{
        components::FormError,
        layout::Page,
        repo_settings::{BranchOption, GeneralForm, RepoSettingsForm, RepoSettingsPage},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/settings", get(show).post(update))
        .route("/{owner}/{repo}/settings/rename", post(rename))
        .route("/{owner}/{repo}/settings/delete", post(delete))
}

/// Loads the repository for a viewer who may administer it; 404 for everyone else.
async fn load_for_admin(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if !loaded.ctx.access.can_admin() {
        return Err(WebError::NotFound);
    }
    Ok(loaded)
}

/// The repository's branch names, in the reader's order.
async fn branch_names(loaded: &RepoPage) -> WebResult<Vec<String>> {
    let refs = loaded.ctx.reader.refs().await.map_err(ServiceError::from)?;
    Ok(refs.into_iter().filter(|r| r.kind == RefKind::Branch).map(|r| r.name).collect())
}

/// The settings page with the stored values in every form.
async fn settings_page(state: &AppState, loaded: &RepoPage, page: Page) -> WebResult<RepoSettingsPage> {
    let repo = &loaded.ctx.repo.repo;
    let branches = branch_names(loaded).await?;
    let general = GeneralForm {
        description: repo.description.clone().unwrap_or_default(),
        is_private: !repo.visibility.is_public(),
        branches: BranchOption::list(branches.iter().map(String::as_str), &repo.default_branch),
    };
    Ok(RepoSettingsPage::new(page, loaded.header(state, "settings"), general))
}

/// Re-renders the settings page with a validation error above `form`.
fn rejected(page: RepoSettingsPage, form: RepoSettingsForm, message: &str) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, page.with_error(form, FormError::from_message(message))).into_response()
}

#[derive(Debug, Deserialize)]
pub struct SavedQuery {
    saved: Option<String>,
}

/// `GET /{owner}/{repo}/settings`
pub async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<SavedQuery>,
) -> WebResult<RepoSettingsPage> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    Ok(settings_page(&state, &loaded, page).await?.saved(query.saved.is_some()))
}

#[derive(Debug, Deserialize)]
pub struct GeneralFormInput {
    #[serde(default)]
    description: String,
    #[serde(default)]
    visibility: String,
    default_branch: Option<String>,
}

/// `POST /{owner}/{repo}/settings`: description, visibility and default branch.
pub async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<GeneralFormInput>,
) -> WebResult<Response> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    let result = match Visibility::parse(&form.visibility) {
        Some(visibility) => {
            let input = UpdateRepository { description: form.description.clone(), visibility, default_branch: form.default_branch.clone() };
            state.services.repo_settings.update(&loaded.ctx, input).await
        }
        None => Err(ServiceError::Invalid("choose public or private".to_owned())),
    };

    match result {
        Ok(()) => Ok(Redirect::to(&format!("{}/settings?saved=1", loaded.urls.home())).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let mut settings = settings_page(&state, &loaded, page).await?;
            settings.general.description = form.description;
            settings.general.is_private = form.visibility == Visibility::Private.as_str();
            if let Some(branch) = &form.default_branch {
                for option in &mut settings.general.branches {
                    option.selected = &option.name == branch;
                }
            }
            Ok(rejected(settings, RepoSettingsForm::General, &message))
        }
        Err(other) => Err(other.into()),
    }
}

#[derive(Debug, Deserialize)]
pub struct RenameFormInput {
    #[serde(default)]
    name: String,
}

/// `POST /{owner}/{repo}/settings/rename`: redirects to the settings under the new name.
pub async fn rename(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<RenameFormInput>,
) -> WebResult<Response> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    match state.services.repo_settings.rename(&loaded.ctx, &form.name).await {
        Ok(name) => Ok(Redirect::to(&format!("/{}/{name}/settings", loaded.ctx.repo.owner.name)).into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            let settings = settings_page(&state, &loaded, page).await?.with_name(form.name);
            Ok(rejected(settings, RepoSettingsForm::Rename, &error.to_string()))
        }
        Err(other) => Err(other.into()),
    }
}

#[derive(Debug, Deserialize)]
pub struct DeleteFormInput {
    #[serde(default)]
    confirm: String,
}

/// `POST /{owner}/{repo}/settings/delete`: redirects to the owner's profile.
pub async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<DeleteFormInput>,
) -> WebResult<Response> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    // Administrators are always signed in; `load_for_admin` answered 404 otherwise.
    let user = viewer.user().ok_or(WebError::NotFound)?;
    match state.services.repo_settings.delete(user, &loaded.ctx, &form.confirm).await {
        Ok(()) => Ok(Redirect::to(&format!("/{}", loaded.ctx.repo.owner.name)).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let settings = settings_page(&state, &loaded, page).await?;
            Ok(rejected(settings, RepoSettingsForm::Delete, &message))
        }
        Err(other) => Err(other.into()),
    }
}
