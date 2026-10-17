//! Repository transfer: `/{owner}/{repo}/settings/transfer`.
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Repository administrators only; everyone else
//! gets a 404. Every rule lives in `TransferService`.

use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use rbit_core::User;
use rbit_services::ServiceError;
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::{
        components::FormError,
        layout::Page,
        transfer::{TransferForm, TransferPage},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/{owner}/{repo}/settings/transfer", get(show).post(transfer))
}

#[derive(Debug, Deserialize)]
pub struct TransferFormData {
    #[serde(default)]
    new_owner: String,
    #[serde(default)]
    confirm: String,
}

/// Loads the repository for a viewer who may administer it; 404 for everyone else.
async fn load_for_admin(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if !loaded.ctx.access.can_admin() {
        return Err(WebError::NotFound);
    }
    Ok(loaded)
}

async fn transfer_page(state: &AppState, loaded: &RepoPage, user: &User, page: Page) -> WebResult<TransferPage> {
    let suggestions = state.services.transfers.suggested_owners(user, &loaded.ctx).await?;
    Ok(TransferPage::new(page, loaded.header(state, "settings"), &suggestions, user.is_admin))
}

/// `GET /{owner}/{repo}/settings/transfer`
pub async fn show(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<TransferPage> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    transfer_page(&state, &loaded, &user, page).await
}

/// `POST /{owner}/{repo}/settings/transfer`: on success, to the repository's new home.
pub async fn transfer(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<TransferFormData>,
) -> WebResult<Response> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    match state.services.transfers.transfer(&user, &loaded.ctx, &form.new_owner, &form.confirm).await {
        Ok(moved) => Ok(Redirect::to(&format!("/{}/{}", moved.owner.name, moved.repo.name)).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let submitted = TransferForm { new_owner: form.new_owner, confirm: form.confirm };
            let page = transfer_page(&state, &loaded, &user, page).await?.rejected(submitted, FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}
