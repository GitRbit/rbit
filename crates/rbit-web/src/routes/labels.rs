//! Labels: `/{owner}/{repo}/labels` (list, create, edit, delete, default labels).
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Repository pages load the repository with
//! `super::repo::RepoPage::load` and build the header with `loaded.header(&state, tab)`.
//!
//! Everyone who can read the repository sees its labels; changing them needs write
//! access (403 otherwise). Invalid input re-renders the page with the reason.

use axum::{
    Form, Router,
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::LabelId;
use rbit_services::{ServiceError, issues::LabelInput};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::{
        components::FormError,
        labels::{LabelForm, LabelView, LabelsPage},
        layout::Page,
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/labels", get(list).post(create))
        .route("/{owner}/{repo}/labels/defaults", post(create_defaults))
        .route("/{owner}/{repo}/labels/{id}", post(update))
        .route("/{owner}/{repo}/labels/{id}/delete", post(delete))
}

fn parse_id(id: &str) -> WebResult<LabelId> {
    id.parse::<i64>().ok().filter(|id| *id > 0).map(LabelId).ok_or(WebError::NotFound)
}

async fn render(state: &AppState, loaded: &RepoPage, page: Page, form: LabelForm, error: FormError) -> WebResult<LabelsPage> {
    let usage = state.services.issues.label_usage(&loaded.ctx).await?;
    let home = loaded.urls.home();
    Ok(LabelsPage {
        page: page.titled(format!("Labels · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "issues"),
        labels: usage.iter().map(|usage| LabelView::new(usage, &home)).collect(),
        can_write: loaded.ctx.access.can_write(),
        create_url: format!("{home}/labels"),
        defaults_url: format!("{home}/labels/defaults"),
        issues_url: format!("{home}/issues"),
        milestones_url: format!("{home}/milestones"),
        error,
        form,
    })
}

/// `GET /{owner}/{repo}/labels`
async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<LabelsPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    render(&state, &loaded, page, LabelForm::default(), FormError::default()).await
}

#[derive(Debug, Deserialize)]
pub struct LabelFormInput {
    #[serde(default)]
    name: String,
    #[serde(default)]
    color: String,
    #[serde(default)]
    description: String,
}

impl LabelFormInput {
    fn input(&self) -> LabelInput {
        LabelInput { name: self.name.clone(), color: self.color.clone(), description: self.description.clone() }
    }
}

/// Back to the list, or the list again with the reason a change was refused.
async fn outcome(
    state: &AppState,
    loaded: &RepoPage,
    page: Page,
    form: &LabelFormInput,
    result: Result<(), ServiceError>,
) -> WebResult<Response> {
    match result {
        Ok(()) => Ok(Redirect::to(&format!("{}/labels", loaded.urls.home())).into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            let values = LabelForm { name: form.name.clone(), color: form.color.clone(), description: form.description.clone() };
            Ok(render(state, loaded, page, values, FormError::from_message(&error.to_string())).await?.into_response())
        }
        Err(error) => Err(error.into()),
    }
}

/// `POST /{owner}/{repo}/labels`
async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<LabelFormInput>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &Viewer(Some(user.clone())), &owner, &repo).await?;
    let result = state.services.issues.create_label(&user, &loaded.ctx, &form.input()).await.map(|_| ());
    outcome(&state, &loaded, page, &form, result).await
}

/// `POST /{owner}/{repo}/labels/{id}`
async fn update(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Form(form): Form<LabelFormInput>,
) -> WebResult<Response> {
    let id = parse_id(&id)?;
    let loaded = RepoPage::load(&state, &Viewer(Some(user.clone())), &owner, &repo).await?;
    let result = state.services.issues.update_label(&user, &loaded.ctx, id, &form.input()).await.map(|_| ());
    outcome(&state, &loaded, page, &form, result).await
}

/// `POST /{owner}/{repo}/labels/{id}/delete`
async fn delete(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, id)): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    let id = parse_id(&id)?;
    let loaded = RepoPage::load(&state, &Viewer(Some(user)), &owner, &repo).await?;
    state.services.issues.delete_label(&loaded.ctx, id).await?;
    Ok(Redirect::to(&format!("{}/labels", loaded.urls.home())))
}

/// `POST /{owner}/{repo}/labels/defaults`: the default set, when there are no labels.
async fn create_defaults(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<Redirect> {
    let loaded = RepoPage::load(&state, &Viewer(Some(user.clone())), &owner, &repo).await?;
    state.services.issues.create_default_labels(&user, &loaded.ctx).await?;
    Ok(Redirect::to(&format!("{}/labels", loaded.urls.home())))
}
