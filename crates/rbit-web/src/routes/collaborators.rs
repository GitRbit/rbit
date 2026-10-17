//! Repository collaborators: `/{owner}/{repo}/settings/collaborators`.
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. The page is for repository administrators only
//! (everyone else gets a 404); the one exception is a collaborator removing themselves.

use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::{Permission, User};
use rbit_services::ServiceError;
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::{
        collaborators::{AddCollaboratorForm, CollaboratorsPage},
        components::FormError,
        layout::Page,
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/settings/collaborators", get(show).post(add))
        .route("/{owner}/{repo}/settings/collaborators/{user}/permission", post(change))
        .route("/{owner}/{repo}/settings/collaborators/{user}/remove", post(remove))
}

#[derive(Debug, Deserialize)]
pub struct AddFormData {
    #[serde(default)]
    username: String,
    permission: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PermissionFormData {
    permission: Option<String>,
}

fn parse_permission(value: Option<&str>) -> Result<Permission, ServiceError> {
    value.and_then(Permission::parse).ok_or_else(|| ServiceError::Invalid("choose a permission".to_owned()))
}

/// Loads the repository for a viewer who may administer it; 404 for everyone else.
async fn load_for_admin(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if !loaded.ctx.access.can_admin() {
        return Err(WebError::NotFound);
    }
    Ok(loaded)
}

async fn collaborators_page(state: &AppState, loaded: &RepoPage, user: &User, page: Page) -> WebResult<CollaboratorsPage> {
    let listed = state.services.collaborators.list(&loaded.ctx).await?;
    let is_org = loaded.ctx.repo.owner.is_organization();
    let header = loaded.header(state, "settings");
    Ok(CollaboratorsPage::new(page, header, &listed.collaborators, &listed.teams, is_org, user.name().as_str()))
}

/// Redirects back to the list after a change, or re-renders it with the validation
/// message and a 422.
async fn after_change(
    state: &AppState,
    loaded: &RepoPage,
    user: &User,
    page: Page,
    result: Result<(), ServiceError>,
    form: AddCollaboratorForm,
) -> WebResult<Response> {
    match result {
        Ok(()) => Ok(Redirect::to(&format!("{}/settings/collaborators", loaded.urls.home())).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let page = collaborators_page(state, loaded, user, page).await?.rejected(form, FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `GET /{owner}/{repo}/settings/collaborators`
pub async fn show(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<CollaboratorsPage> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    collaborators_page(&state, &loaded, &user, page).await
}

/// `POST /{owner}/{repo}/settings/collaborators`: add someone.
pub async fn add(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<AddFormData>,
) -> WebResult<Response> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    let permission = parse_permission(form.permission.as_deref());
    let submitted = AddCollaboratorForm::new(&form.username, *permission.as_ref().unwrap_or(&Permission::Write));
    let result = match permission {
        Ok(permission) => state.services.collaborators.add(&user, &loaded.ctx, &form.username, permission).await,
        Err(error) => Err(error),
    };
    after_change(&state, &loaded, &user, page, result, submitted).await
}

/// `POST /{owner}/{repo}/settings/collaborators/{user}/permission`
pub async fn change(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, member)): Path<(String, String, String)>,
    Form(form): Form<PermissionFormData>,
) -> WebResult<Response> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    let result = match parse_permission(form.permission.as_deref()) {
        Ok(permission) => state.services.collaborators.change(&user, &loaded.ctx, &member, permission).await,
        Err(error) => Err(error),
    };
    after_change(&state, &loaded, &user, page, result, AddCollaboratorForm::default()).await
}

/// `POST /{owner}/{repo}/settings/collaborators/{user}/remove`: administrators remove
/// anyone; a collaborator may remove themselves (and is then sent to the dashboard,
/// since they may no longer see the repository).
pub async fn remove(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, member)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let is_admin = loaded.ctx.access.can_admin();
    let leaving = member.eq_ignore_ascii_case(user.name().as_str());
    if !leaving && !is_admin {
        return Err(WebError::NotFound);
    }
    let result = state.services.collaborators.remove(&user, &loaded.ctx, &member).await;
    match result {
        Ok(()) if leaving => Ok(Redirect::to("/").into_response()),
        // Only administrators have a page to show the message on.
        Err(error) if !is_admin => Err(error.into()),
        result => after_change(&state, &loaded, &user, page, result, AddCollaboratorForm::default()).await,
    }
}
