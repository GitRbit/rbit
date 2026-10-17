//! Push mirrors: `/{owner}/{repo}/settings/push-mirrors`.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Repository administrators only; everyone else
//! gets 404. Secrets are never put back into a form.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::push_mirror::PushMirrorId;
use rbit_services::{ServiceError, push_mirrors::PushMirrorInput};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::{
        components::FormError,
        layout::Page,
        push_mirrors::{PushMirrorEditPage, PushMirrorForm, PushMirrorView, PushMirrorsPage},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/settings/push-mirrors", get(list).post(create))
        .route("/{owner}/{repo}/settings/push-mirrors/{id}", get(edit).post(update))
        .route("/{owner}/{repo}/settings/push-mirrors/{id}/sync", post(sync))
        .route("/{owner}/{repo}/settings/push-mirrors/{id}/delete", post(delete))
}

#[derive(Debug, Deserialize)]
pub struct MirrorForm {
    #[serde(default)]
    url: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    secret: String,
    enabled: Option<String>,
    sync_on_push: Option<String>,
}

impl MirrorForm {
    fn input(&self, enabled: bool) -> PushMirrorInput {
        PushMirrorInput {
            url: self.url.clone(),
            username: self.username.clone(),
            secret: self.secret.clone(),
            enabled,
            sync_on_push: self.sync_on_push.is_some(),
        }
    }

    /// The form shown again after a refusal (never with the secret).
    fn redisplay(&self, has_secret: bool, enabled: bool) -> PushMirrorForm {
        PushMirrorForm {
            url: self.url.clone(),
            username: self.username.clone(),
            has_secret,
            enabled,
            sync_on_push: self.sync_on_push.is_some(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    added: Option<String>,
    saved: Option<String>,
    queued: Option<String>,
    removed: Option<String>,
}

/// Loads the repository for one of its administrators; 404 for everyone else.
async fn load(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if !loaded.ctx.access.can_admin() {
        return Err(WebError::NotFound);
    }
    Ok(loaded)
}

fn list_url(loaded: &RepoPage) -> String {
    format!("{}/settings/push-mirrors", loaded.urls.home())
}

fn parse_id(id: &str) -> WebResult<PushMirrorId> {
    id.parse::<i64>().ok().filter(|id| *id > 0).map(PushMirrorId).ok_or(WebError::NotFound)
}

async fn list_view(state: &AppState, loaded: &RepoPage, page: Page) -> WebResult<PushMirrorsPage> {
    let service = &state.services.push_mirrors;
    let base = list_url(loaded);
    let mirrors = if service.is_enabled() { service.list(&loaded.ctx).await? } else { Vec::new() };
    Ok(PushMirrorsPage {
        page: page.titled(format!("Push mirrors · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "settings"),
        disabled: !service.is_enabled(),
        mirrors: mirrors.iter().map(|mirror| PushMirrorView::new(mirror, &base)).collect(),
        form: PushMirrorForm { enabled: true, sync_on_push: true, ..PushMirrorForm::default() },
        error: FormError::default(),
        notice: None,
        interval_minutes: service.interval_minutes(),
        action_url: base,
        allow_local: state.config.push_mirrors.allow_local_urls,
    })
}

/// `GET /{owner}/{repo}/settings/push-mirrors`
async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> WebResult<PushMirrorsPage> {
    let loaded = load(&state, &viewer, &owner, &repo).await?;
    let mut view = list_view(&state, &loaded, page).await?;
    view.notice = if query.added.is_some() {
        Some("Push mirror added. The first push is on its way; refresh in a moment to see its outcome.")
    } else if query.queued.is_some() {
        Some("A push is queued; refresh in a moment to see its outcome.")
    } else if query.saved.is_some() {
        Some("Saved.")
    } else if query.removed.is_some() {
        Some("Push mirror removed. The remote keeps what it has.")
    } else {
        None
    };
    Ok(view)
}

/// `POST /{owner}/{repo}/settings/push-mirrors`: add a mirror.
async fn create(
    State(state): State<AppState>,
    viewer: Viewer,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<MirrorForm>,
) -> WebResult<Response> {
    let loaded = load(&state, &viewer, &owner, &repo).await?;
    match state.services.push_mirrors.create(&loaded.ctx, &user, &form.input(true)).await {
        Ok(_) => Ok(Redirect::to(&format!("{}?added=1", list_url(&loaded))).into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            let mut view = list_view(&state, &loaded, page).await?;
            view.form = form.redisplay(false, true);
            view.error = FormError::from_message(&error.to_string());
            Ok((StatusCode::UNPROCESSABLE_ENTITY, view).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `GET /{owner}/{repo}/settings/push-mirrors/{id}`: the edit form.
async fn edit(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
) -> WebResult<PushMirrorEditPage> {
    let loaded = load(&state, &viewer, &owner, &repo).await?;
    let id = parse_id(&id)?;
    let mirrors = state.services.push_mirrors.list(&loaded.ctx).await?;
    let mirror = mirrors.iter().find(|mirror| mirror.id == id).ok_or(WebError::NotFound)?;
    let form = PushMirrorForm {
        url: mirror.url.clone(),
        username: mirror.username.clone(),
        has_secret: mirror.has_secret,
        enabled: mirror.enabled,
        sync_on_push: mirror.sync_on_push,
    };
    Ok(edit_view(&state, &loaded, page, &PushMirrorView::new(mirror, &list_url(&loaded)), form))
}

fn edit_view(state: &AppState, loaded: &RepoPage, page: Page, mirror: &PushMirrorView, form: PushMirrorForm) -> PushMirrorEditPage {
    PushMirrorEditPage {
        page: page.titled(format!("Push mirror · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "settings"),
        mirror: mirror.clone(),
        form,
        error: FormError::default(),
        list_url: list_url(loaded),
        allow_local: state.config.push_mirrors.allow_local_urls,
    }
}

/// `POST /{owner}/{repo}/settings/push-mirrors/{id}`: save the edit form.
async fn update(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Form(form): Form<MirrorForm>,
) -> WebResult<Response> {
    let loaded = load(&state, &viewer, &owner, &repo).await?;
    let id = parse_id(&id)?;
    let service = &state.services.push_mirrors;
    let mirrors = service.list(&loaded.ctx).await?;
    let mirror = mirrors.iter().find(|mirror| mirror.id == id).ok_or(WebError::NotFound)?;
    match service.update(&loaded.ctx, id, &form.input(form.enabled.is_some())).await {
        Ok(_) => Ok(Redirect::to(&format!("{}?saved=1", list_url(&loaded))).into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            let view = PushMirrorView::new(mirror, &list_url(&loaded));
            let mut page = edit_view(&state, &loaded, page, &view, form.redisplay(mirror.has_secret, form.enabled.is_some()));
            page.error = FormError::from_message(&error.to_string());
            Ok((StatusCode::UNPROCESSABLE_ENTITY, page).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `POST /{owner}/{repo}/settings/push-mirrors/{id}/sync`: push now (in the background).
async fn sync(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, id)): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    let loaded = load(&state, &viewer, &owner, &repo).await?;
    state.services.push_mirrors.sync_now(&loaded.ctx, parse_id(&id)?).await?;
    Ok(Redirect::to(&format!("{}?queued=1", list_url(&loaded))))
}

/// `POST /{owner}/{repo}/settings/push-mirrors/{id}/delete`
async fn delete(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, id)): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    let loaded = load(&state, &viewer, &owner, &repo).await?;
    state.services.push_mirrors.delete(&loaded.ctx, parse_id(&id)?).await?;
    Ok(Redirect::to(&format!("{}?removed=1", list_url(&loaded))))
}
