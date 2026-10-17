//! Milestones: `/{owner}/{repo}/milestones`, `/milestones/new`, `/milestones/{id}`.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Repository pages load the repository with
//! `super::repo::RepoPage::load` and build the header with `loaded.header(&state, tab)`.
//!
//! Readers see milestones; writers create, edit, close, reopen and delete them (the
//! forms are 404 for everyone else, changes 403). Milestones are addressed by id.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::{
    IssueState, MilestoneId,
    issue_query::{IssueQuery, StateFilter},
};
use rbit_services::{ServiceError, issues::MilestoneInput};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    render::render_markdown,
    urls::DocumentBase,
    views::{
        components::{FormError, Pagination},
        issues::IssueRow,
        layout::Page,
        milestones::{MilestoneForm, MilestoneFormPage, MilestonePage, MilestoneView, MilestonesPage},
    },
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{owner}/{repo}/milestones", get(list).post(create))
        .route("/{owner}/{repo}/milestones/new", get(new_form))
        .route("/{owner}/{repo}/milestones/{id}", get(show).post(update))
        .route("/{owner}/{repo}/milestones/{id}/edit", get(edit_form))
        .route("/{owner}/{repo}/milestones/{id}/state", post(set_state))
        .route("/{owner}/{repo}/milestones/{id}/delete", post(delete))
}

fn parse_id(id: &str) -> WebResult<MilestoneId> {
    id.parse::<i64>().ok().filter(|id| *id > 0).map(MilestoneId).ok_or(WebError::NotFound)
}

/// The repository for a viewer who may write to it; 404 for everyone else.
async fn load_for_writer(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoPage> {
    let loaded = RepoPage::load(state, viewer, owner, repo).await?;
    if !loaded.ctx.access.can_write() {
        return Err(WebError::NotFound);
    }
    Ok(loaded)
}

#[derive(Debug, Deserialize)]
pub struct StateQuery {
    state: Option<String>,
    page: Option<u32>,
}

/// `GET /{owner}/{repo}/milestones[?state=closed]`
async fn list(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<StateQuery>,
) -> WebResult<MilestonesPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let closed = query.state.as_deref() == Some("closed");
    let issues = &state.services.issues;
    let wanted = if closed { IssueState::Closed } else { IssueState::Open };
    let (milestones, (open, closed_count)) =
        tokio::try_join!(issues.milestones(&loaded.ctx, Some(wanted)), issues.milestone_counts(&loaded.ctx))?;
    let home = loaded.urls.home();
    Ok(MilestonesPage {
        page: page.titled(format!("Milestones · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "issues"),
        milestones: milestones.iter().map(|progress| MilestoneView::new(progress, &home)).collect(),
        open,
        closed: closed_count,
        showing_closed: closed,
        open_url: format!("{home}/milestones"),
        closed_url: format!("{home}/milestones?state=closed"),
        can_write: loaded.ctx.access.can_write(),
        new_url: format!("{home}/milestones/new"),
        issues_url: format!("{home}/issues"),
        labels_url: format!("{home}/labels"),
    })
}

/// `GET /{owner}/{repo}/milestones/{id}[?state=closed&page=n]`: its threads, open or closed.
async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Query(query): Query<StateQuery>,
) -> WebResult<MilestonePage> {
    let id = parse_id(&id)?;
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let issues = &state.services.issues;
    let progress = issues.milestone(&loaded.ctx, id).await?;
    let showing_closed = query.state.as_deref() == Some("closed");
    let state_filter = if showing_closed { StateFilter::Closed } else { StateFilter::Open };
    let search = IssueQuery { state: Some(state_filter), milestone: Some(progress.milestone.title.clone()), ..IssueQuery::default() };
    let results = issues.search(viewer.user(), &loaded.ctx, &search, query.page.unwrap_or(1)).await?;
    let home = loaded.urls.home();
    let view = MilestoneView::new(&progress, &home);
    let base = DocumentBase::new(loaded.urls.clone(), loaded.default_branch().to_owned(), "");
    let description = &progress.milestone.description;
    let state_param = if showing_closed { "state=closed&" } else { "" };
    let page_url = |n: u32| format!("{}?{state_param}page={n}", view.url);
    let pagination = Pagination {
        page: results.page,
        page_count: results.page_count(),
        previous_url: (results.page > 1).then(|| page_url(results.page - 1)),
        next_url: (results.page < results.page_count()).then(|| page_url(results.page + 1)),
    };
    let repo = &loaded.ctx.repo;
    Ok(MilestonePage {
        page: page.titled(format!("{} · Milestones · {}", progress.milestone.title, repo.full_name())),
        header: loaded.header(&state, "issues"),
        description_html: if description.trim().is_empty() { String::new() } else { render_markdown(description, Some(&base)) },
        rows: IssueRow::list(&results, repo.owner.name.as_str(), repo.repo.name.as_str(), &format!("{home}/issues")),
        showing_closed,
        open_url: view.url.clone(),
        closed_url: format!("{}?state=closed", view.url),
        pagination,
        can_write: loaded.ctx.access.can_write(),
        milestones_url: format!("{home}/milestones"),
        milestone: view,
    })
}

#[derive(Debug, Deserialize)]
pub struct MilestoneFormInput {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    due_on: String,
}

impl MilestoneFormInput {
    fn input(&self) -> MilestoneInput {
        MilestoneInput { title: self.title.clone(), description: self.description.clone(), due_on: self.due_on.clone() }
    }

    fn values(&self) -> MilestoneForm {
        MilestoneForm { title: self.title.clone(), description: self.description.clone(), due_on: self.due_on.clone() }
    }
}

fn form_page(
    state: &AppState,
    loaded: &RepoPage,
    page: Page,
    form: MilestoneForm,
    error: FormError,
    edit: Option<MilestoneId>,
) -> MilestoneFormPage {
    let home = loaded.urls.home();
    let (title, action_url, cancel_url) = match edit {
        None => ("New milestone", format!("{home}/milestones"), format!("{home}/milestones")),
        Some(id) => ("Edit milestone", format!("{home}/milestones/{}", id.0), format!("{home}/milestones/{}", id.0)),
    };
    MilestoneFormPage {
        page: page.titled(format!("{title} · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "issues"),
        error,
        form,
        action_url,
        cancel_url,
        is_new: edit.is_none(),
    }
}

/// `GET /{owner}/{repo}/milestones/new`
async fn new_form(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<MilestoneFormPage> {
    let loaded = load_for_writer(&state, &Viewer(Some(user)), &owner, &repo).await?;
    Ok(form_page(&state, &loaded, page, MilestoneForm::default(), FormError::default(), None))
}

/// `GET /{owner}/{repo}/milestones/{id}/edit`
async fn edit_form(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
) -> WebResult<MilestoneFormPage> {
    let id = parse_id(&id)?;
    let loaded = load_for_writer(&state, &Viewer(Some(user)), &owner, &repo).await?;
    let milestone = state.services.issues.milestone(&loaded.ctx, id).await?.milestone;
    let form = MilestoneForm { title: milestone.title, description: milestone.description, due_on: milestone.due_on.unwrap_or_default() };
    Ok(form_page(&state, &loaded, page, form, FormError::default(), Some(id)))
}

/// `POST /{owner}/{repo}/milestones`
async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Form(form): Form<MilestoneFormInput>,
) -> WebResult<Response> {
    let loaded = load_for_writer(&state, &Viewer(Some(user.clone())), &owner, &repo).await?;
    match state.services.issues.create_milestone(&user, &loaded.ctx, &form.input()).await {
        Ok(milestone) => Ok(Redirect::to(&format!("{}/milestones/{}", loaded.urls.home(), milestone.id.0)).into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            Ok(form_page(&state, &loaded, page, form.values(), FormError::from_message(&error.to_string()), None).into_response())
        }
        Err(error) => Err(error.into()),
    }
}

/// `POST /{owner}/{repo}/milestones/{id}`
async fn update(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    page: Page,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Form(form): Form<MilestoneFormInput>,
) -> WebResult<Response> {
    let id = parse_id(&id)?;
    let loaded = load_for_writer(&state, &Viewer(Some(user.clone())), &owner, &repo).await?;
    match state.services.issues.update_milestone(&user, &loaded.ctx, id, &form.input()).await {
        Ok(milestone) => Ok(Redirect::to(&format!("{}/milestones/{}", loaded.urls.home(), milestone.id.0)).into_response()),
        Err(error @ (ServiceError::Invalid(_) | ServiceError::Conflict(_))) => {
            Ok(form_page(&state, &loaded, page, form.values(), FormError::from_message(&error.to_string()), Some(id)).into_response())
        }
        Err(error) => Err(error.into()),
    }
}

#[derive(Debug, Deserialize)]
pub struct StateForm {
    #[serde(default)]
    state: String,
}

/// `POST /{owner}/{repo}/milestones/{id}/state`: `state=open|closed`.
async fn set_state(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, id)): Path<(String, String, String)>,
    Form(form): Form<StateForm>,
) -> WebResult<Redirect> {
    let id = parse_id(&id)?;
    let wanted = IssueState::parse(&form.state).ok_or_else(|| WebError::BadRequest("state must be open or closed".to_owned()))?;
    let loaded = RepoPage::load(&state, &Viewer(Some(user)), &owner, &repo).await?;
    state.services.issues.set_milestone_state(&loaded.ctx, id, wanted).await?;
    let suffix = if wanted == IssueState::Closed { "?state=closed" } else { "" };
    Ok(Redirect::to(&format!("{}/milestones{suffix}", loaded.urls.home())))
}

/// `POST /{owner}/{repo}/milestones/{id}/delete`
async fn delete(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    Path((owner, repo, id)): Path<(String, String, String)>,
) -> WebResult<Redirect> {
    let id = parse_id(&id)?;
    let loaded = RepoPage::load(&state, &Viewer(Some(user)), &owner, &repo).await?;
    state.services.issues.delete_milestone(&loaded.ctx, id).await?;
    Ok(Redirect::to(&format!("{}/milestones", loaded.urls.home())))
}
