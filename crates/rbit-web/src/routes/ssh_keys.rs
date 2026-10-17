//! SSH keys: `/settings/keys` and repository deploy keys `/{owner}/{repo}/settings/keys`.
//!
//! Merged into the page router, so these routes get sessions, CSRF protection, security
//! headers and error pages. Deploy key pages are for repository administrators only;
//! everyone else gets a 404.

use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::config::SshConfig;
use rbit_services::{
    ServiceError,
    ssh_keys::{AddDeployKey, AddSshKey, DeployKeyId, SshKeyId},
};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::{
        components::FormError,
        layout::Page,
        settings::SettingsSection,
        ssh_keys::{DeployKeysPage, KeyForm, SshKeysPage},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settings/keys", get(keys).post(add_key))
        .route("/settings/keys/{id}/delete", post(delete_key))
        .route("/{owner}/{repo}/settings/keys", get(deploy_keys).post(add_deploy_key))
        .route("/{owner}/{repo}/settings/keys/{id}/delete", post(delete_deploy_key))
}

/// The submitted add form (user keys and deploy keys share it).
#[derive(Debug, Deserialize)]
pub struct KeyFormInput {
    #[serde(default)]
    title: String,
    #[serde(default)]
    key: String,
    /// The "Allow write access" checkbox (deploy keys only); present when ticked.
    allow_write: Option<String>,
}

impl KeyFormInput {
    fn into_form(self) -> KeyForm {
        KeyForm { title: self.title, key: self.key, allow_write: self.allow_write.is_some() }
    }
}

/// `ssh -T git@host` for checking a key, when the SSH server is enabled.
fn test_command(state: &AppState) -> Option<String> {
    let ssh = &state.config.ssh;
    ssh.enabled.then(|| {
        let host = ssh.public_host(&state.config.server);
        match ssh.public_port() {
            22 => format!("ssh -T {}@{host}", SshConfig::USER),
            port => format!("ssh -T -p {port} {}@{host}", SshConfig::USER),
        }
    })
}

// ------------------------------------------------------------------ user keys

/// `GET /settings/keys`
pub async fn keys(State(state): State<AppState>, SignedIn(user): SignedIn, page: Page) -> WebResult<SshKeysPage> {
    let keys = state.services.ssh_keys.list_user_keys(&user).await?;
    Ok(SshKeysPage::new(page, &keys, test_command(&state)))
}

/// `POST /settings/keys`
pub async fn add_key(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Form(input): Form<KeyFormInput>,
) -> WebResult<Response> {
    let form = input.into_form();
    let request = AddSshKey { title: form.title.clone(), key: form.key.clone() };
    match state.services.ssh_keys.add_user_key(&user, request).await {
        Ok(_) => Ok(Redirect::to(SettingsSection::SshKeys.url()).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let keys = state.services.ssh_keys.list_user_keys(&user).await?;
            let page = SshKeysPage::new(page, &keys, test_command(&state)).rejected(form, FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `POST /settings/keys/{id}/delete`
pub async fn delete_key(State(state): State<AppState>, SignedIn(user): SignedIn, Path(id): Path<String>) -> WebResult<Redirect> {
    let id = id.parse().map(SshKeyId).map_err(|_| WebError::NotFound)?;
    state.services.ssh_keys.delete_user_key(&user, id).await?;
    Ok(Redirect::to(SettingsSection::SshKeys.url()))
}

// ------------------------------------------------------------------ deploy keys

/// Loads the repository for a viewer who may administer it; 404 for everyone else.
async fn load_for_admin(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if !loaded.ctx.access.can_admin() {
        return Err(WebError::NotFound);
    }
    Ok(loaded)
}

async fn deploy_keys_page(state: &AppState, loaded: &RepoPage, page: Page) -> WebResult<DeployKeysPage> {
    let keys = state.services.ssh_keys.list_deploy_keys(&loaded.ctx).await?;
    Ok(DeployKeysPage::new(page, loaded.header(state, "settings"), &keys))
}

/// `GET /{owner}/{repo}/settings/keys`
pub async fn deploy_keys(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<DeployKeysPage> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    deploy_keys_page(&state, &loaded, page).await
}

/// `POST /{owner}/{repo}/settings/keys`
pub async fn add_deploy_key(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(input): Form<KeyFormInput>,
) -> WebResult<Response> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    let form = input.into_form();
    let request = AddDeployKey { title: form.title.clone(), key: form.key.clone(), allow_write: form.allow_write };
    match state.services.ssh_keys.add_deploy_key(&user, &loaded.ctx, request).await {
        Ok(_) => Ok(Redirect::to(&format!("{}/settings/keys", loaded.urls.home())).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let page = deploy_keys_page(&state, &loaded, page).await?.rejected(form, FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `POST /{owner}/{repo}/settings/keys/{id}/delete`
pub async fn delete_deploy_key(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    Path((owner, repo, id)): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    let loaded = load_for_admin(&state, &viewer, &owner, &repo).await?;
    let id = id.parse().map(DeployKeyId).map_err(|_| WebError::NotFound)?;
    state.services.ssh_keys.delete_deploy_key(&user, &loaded.ctx, id).await?;
    Ok(Redirect::to(&format!("{}/settings/keys", loaded.urls.home())))
}
