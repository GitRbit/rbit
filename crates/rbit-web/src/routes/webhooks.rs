//! Webhook settings: `/{owner}/{repo}/settings/hooks` and `/{org}/settings/hooks`,
//! with the delivery log and redelivery.
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Every handler serves both kinds of owner: the
//! path tells them apart (`repo` is present for repository hooks). Only repository
//! administrators and organisation owners get these pages; everyone else gets 404.

use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use rbit_core::webhook::{DeliveryId, WebhookEvent, WebhookId};
use rbit_services::{
    OrgContext, ServiceError,
    webhooks::{HookTarget, SecretUpdate, WebhookInput},
};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::{SignedIn, Viewer},
    views::{
        components::{FormError, Pagination},
        layout::Page,
        org::{OrgHeader, OrgTab},
        webhooks::{DeliveryPage, HookForm, HookFrame, WebhookFormPage, WebhooksPage},
    },
};

/// Deliveries per page on a hook's page.
const DELIVERIES_PER_PAGE: u32 = 20;

pub fn routes() -> Router<AppState> {
    let mut router = Router::new();
    for prefix in ["/{owner}/{repo}/settings/hooks", "/{owner}/settings/hooks"] {
        router = router
            .route(prefix, get(list).post(create))
            .route(&format!("{prefix}/new"), get(new_form))
            .route(&format!("{prefix}/{{id}}"), get(show).post(update))
            .route(&format!("{prefix}/{{id}}/ping"), post(ping))
            .route(&format!("{prefix}/{{id}}/delete"), post(delete))
            .route(&format!("{prefix}/{{id}}/deliveries/{{delivery}}"), get(delivery))
            .route(&format!("{prefix}/{{id}}/deliveries/{{delivery}}/redeliver"), post(redeliver));
    }
    router
}

/// The path parameters of every webhook page.
#[derive(Debug, Deserialize)]
pub struct HookPath {
    owner: String,
    repo: Option<String>,
    id: Option<String>,
    delivery: Option<String>,
}

impl HookPath {
    fn hook_id(&self) -> WebResult<WebhookId> {
        self.id.as_deref().and_then(|id| id.parse().ok()).map(WebhookId).ok_or(WebError::NotFound)
    }

    fn delivery_id(&self) -> WebResult<DeliveryId> {
        self.delivery.as_deref().and_then(|id| id.parse().ok()).map(DeliveryId).ok_or(WebError::NotFound)
    }
}

/// The repository or organisation, loaded for a viewer who may manage its hooks.
enum Owner {
    Repository(Box<RepoPage>),
    Organization(OrgContext),
}

impl Owner {
    async fn load(state: &AppState, viewer: &Viewer, path: &HookPath) -> WebResult<Self> {
        if let Some(repo) = &path.repo {
            let loaded = RepoPage::load(state, viewer, &path.owner, repo).await?;
            if !loaded.ctx.access.can_admin() {
                return Err(WebError::NotFound);
            }
            return Ok(Self::Repository(Box::new(loaded)));
        }
        let ctx = state.services.orgs.open(viewer.user(), &path.owner).await?;
        if !ctx.can_manage {
            return Err(WebError::NotFound);
        }
        Ok(Self::Organization(ctx))
    }

    fn target(&self) -> HookTarget<'_> {
        match self {
            Self::Repository(loaded) => HookTarget::Repository(&loaded.ctx),
            Self::Organization(ctx) => HookTarget::Organization(ctx),
        }
    }

    fn frame(&self, state: &AppState) -> HookFrame {
        match self {
            Self::Repository(loaded) => HookFrame::for_repository(loaded.header(state, "settings")),
            Self::Organization(ctx) => HookFrame::for_organization(OrgHeader::new(ctx, OrgTab::Settings)),
        }
        // Actions: integration (I).
        .with_actions(state.services.actions.is_some())
    }
}

/// The submitted create/edit form. Checkboxes are present only when ticked.
#[derive(Debug, Deserialize)]
pub struct HookFormInput {
    #[serde(default)]
    url: String,
    #[serde(default)]
    secret: String,
    remove_secret: Option<String>,
    active: Option<String>,
    event_push: Option<String>,
    event_repository: Option<String>,
    event_release: Option<String>,
    event_fork: Option<String>,
    event_issues: Option<String>,
    event_issue_comment: Option<String>,
    event_pull_request: Option<String>,
    event_pull_request_review: Option<String>,
    event_member: Option<String>,
    // Milestone 3: commit statuses (S).
    event_status: Option<String>,
    // Actions: integration (I).
    event_workflow_run: Option<String>,
    event_workflow_job: Option<String>,
}

impl HookFormInput {
    fn events(&self) -> Vec<WebhookEvent> {
        let ticked = [
            (WebhookEvent::Push, &self.event_push),
            (WebhookEvent::Repository, &self.event_repository),
            (WebhookEvent::Release, &self.event_release),
            (WebhookEvent::Fork, &self.event_fork),
            (WebhookEvent::Issues, &self.event_issues),
            (WebhookEvent::IssueComment, &self.event_issue_comment),
            (WebhookEvent::PullRequest, &self.event_pull_request),
            (WebhookEvent::PullRequestReview, &self.event_pull_request_review),
            (WebhookEvent::Member, &self.event_member),
            (WebhookEvent::Status, &self.event_status),
            (WebhookEvent::WorkflowRun, &self.event_workflow_run),
            (WebhookEvent::WorkflowJob, &self.event_workflow_job),
        ];
        ticked.into_iter().filter(|(_, value)| value.is_some()).map(|(event, _)| event).collect()
    }

    fn input(&self) -> WebhookInput {
        let secret = if self.remove_secret.is_some() {
            SecretUpdate::Clear
        } else if self.secret.is_empty() {
            SecretUpdate::Keep
        } else {
            SecretUpdate::Set(self.secret.clone())
        };
        WebhookInput {
            url: self.url.clone(),
            events: self.events().iter().map(|event| event.as_str().to_owned()).collect(),
            active: self.active.is_some(),
            secret,
        }
    }

    /// The form as submitted, for re-rendering (the secret is never echoed back).
    fn form(&self, actions: bool) -> HookForm {
        HookForm::with(self.url.clone(), &self.events(), self.active.is_some(), false, actions)
    }
}

#[derive(Debug, Deserialize)]
pub struct HookQuery {
    page: Option<u32>,
    saved: Option<String>,
    pinged: Option<String>,
}

/// `GET …/settings/hooks`
pub async fn list(State(state): State<AppState>, viewer: Viewer, page: Page, Path(path): Path<HookPath>) -> WebResult<WebhooksPage> {
    let owner = Owner::load(&state, &viewer, &path).await?;
    let hooks = state.services.webhooks.list(owner.target()).await?;
    Ok(WebhooksPage::new(page, owner.frame(&state), &hooks))
}

/// `GET …/settings/hooks/new`
pub async fn new_form(State(state): State<AppState>, viewer: Viewer, page: Page, Path(path): Path<HookPath>) -> WebResult<WebhookFormPage> {
    let owner = Owner::load(&state, &viewer, &path).await?;
    Ok(WebhookFormPage::new_hook(page, owner.frame(&state)))
}

/// `POST …/settings/hooks`: creates the hook and shows it.
pub async fn create(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path(path): Path<HookPath>,
    Form(form): Form<HookFormInput>,
) -> WebResult<Response> {
    let owner = Owner::load(&state, &viewer, &path).await?;
    match state.services.webhooks.create(&user, owner.target(), form.input()).await {
        Ok(hook) => Ok(Redirect::to(&format!("{}/{}", owner.frame(&state).hooks_url, hook.id.0)).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let rendered = WebhookFormPage::new_hook(page, owner.frame(&state))
                .rejected(form.form(state.services.actions.is_some()), FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, rendered).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// The edit page of the hook in `path`, with one page of deliveries.
async fn edit_page(state: &AppState, owner: &Owner, page: Page, path: &HookPath, page_number: u32) -> WebResult<WebhookFormPage> {
    let id = path.hook_id()?;
    let webhooks = &state.services.webhooks;
    let hook = webhooks.get(owner.target(), id).await?;
    let log = webhooks.deliveries(owner.target(), id, page_number, DELIVERIES_PER_PAGE).await?;
    let frame = owner.frame(state);
    let page_count = u32::try_from(log.total.div_ceil(u64::from(DELIVERIES_PER_PAGE)).max(1)).unwrap_or(u32::MAX);
    let page_number = page_number.clamp(1, page_count);
    let hook_url = format!("{}/{}", frame.hooks_url, id.0);
    let pager = Pagination {
        page: page_number,
        page_count,
        previous_url: (page_number > 1).then(|| format!("{hook_url}?page={}", page_number - 1)),
        next_url: (page_number < page_count).then(|| format!("{hook_url}?page={}", page_number + 1)),
    };
    Ok(WebhookFormPage::edit(page, frame, &hook, &log.deliveries, pager))
}

/// `GET …/settings/hooks/{id}`
pub async fn show(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path(path): Path<HookPath>,
    Query(query): Query<HookQuery>,
) -> WebResult<WebhookFormPage> {
    let owner = Owner::load(&state, &viewer, &path).await?;
    let rendered = edit_page(&state, &owner, page, &path, query.page.unwrap_or(1)).await?;
    Ok(rendered.notices(query.saved.is_some(), query.pinged.is_some()))
}

/// `POST …/settings/hooks/{id}`
pub async fn update(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path(path): Path<HookPath>,
    Form(form): Form<HookFormInput>,
) -> WebResult<Response> {
    let owner = Owner::load(&state, &viewer, &path).await?;
    let id = path.hook_id()?;
    match state.services.webhooks.update(&user, owner.target(), id, form.input()).await {
        Ok(hook) => Ok(Redirect::to(&format!("{}/{}?saved=1", owner.frame(&state).hooks_url, hook.id.0)).into_response()),
        Err(ServiceError::Invalid(message)) => {
            let rendered = edit_page(&state, &owner, page, &path, 1)
                .await?
                .rejected(form.form(state.services.actions.is_some()), FormError::from_message(&message));
            Ok((StatusCode::UNPROCESSABLE_ENTITY, rendered).into_response())
        }
        Err(other) => Err(other.into()),
    }
}

/// `POST …/settings/hooks/{id}/ping`
pub async fn ping(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    Path(path): Path<HookPath>,
) -> WebResult<Redirect> {
    let owner = Owner::load(&state, &viewer, &path).await?;
    let id = path.hook_id()?;
    state.services.webhooks.ping(&user, owner.target(), id).await?;
    Ok(Redirect::to(&format!("{}/{}?pinged=1", owner.frame(&state).hooks_url, id.0)))
}

#[derive(Debug, Deserialize)]
pub struct DeleteFormInput {
    confirm: Option<String>,
}

/// `POST …/settings/hooks/{id}/delete`: needs the confirmation box ticked.
pub async fn delete(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    page: Page,
    Path(path): Path<HookPath>,
    Form(form): Form<DeleteFormInput>,
) -> WebResult<Response> {
    let owner = Owner::load(&state, &viewer, &path).await?;
    let id = path.hook_id()?;
    if form.confirm.is_none() {
        let rendered = edit_page(&state, &owner, page, &path, 1).await?.delete_rejected(FormError::new("Tick the box to confirm."));
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, rendered).into_response());
    }
    state.services.webhooks.delete(&user, owner.target(), id).await?;
    Ok(Redirect::to(&owner.frame(&state).hooks_url).into_response())
}

/// `GET …/settings/hooks/{id}/deliveries/{delivery}`
pub async fn delivery(State(state): State<AppState>, viewer: Viewer, page: Page, Path(path): Path<HookPath>) -> WebResult<DeliveryPage> {
    let owner = Owner::load(&state, &viewer, &path).await?;
    let (id, delivery_id) = (path.hook_id()?, path.delivery_id()?);
    let webhooks = &state.services.webhooks;
    let hook = webhooks.get(owner.target(), id).await?;
    let delivery = webhooks.delivery(owner.target(), id, delivery_id).await?;
    Ok(DeliveryPage::new(page, owner.frame(&state), &hook, &delivery))
}

/// `POST …/settings/hooks/{id}/deliveries/{delivery}/redeliver`: shows the new delivery.
pub async fn redeliver(
    State(state): State<AppState>,
    SignedIn(user): SignedIn,
    viewer: Viewer,
    Path(path): Path<HookPath>,
) -> WebResult<Redirect> {
    let owner = Owner::load(&state, &viewer, &path).await?;
    let (id, delivery_id) = (path.hook_id()?, path.delivery_id()?);
    let again = state.services.webhooks.redeliver(&user, owner.target(), id, delivery_id).await?;
    Ok(Redirect::to(&format!("{}/{}/deliveries/{}", owner.frame(&state).hooks_url, id.0, again.id.0)))
}
