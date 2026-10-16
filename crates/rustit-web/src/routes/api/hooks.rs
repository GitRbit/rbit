//! Webhooks of a repository (`/repos/{owner}/{repo}/hooks`) or an organisation
//! (`/orgs/{org}/hooks`). Every endpoint needs the `admin` scope and repository
//! administrator or organisation owner rights; callers who can see the repository or
//! organisation but lack them get 403.

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rustit_core::{
    TokenScope,
    webhook::{DeliveryId, WebhookId},
};
use rustit_services::{
    OrgContext, RepoContext,
    webhooks::{HookTarget, SecretUpdate, WebhookInput},
};
use serde::Deserialize;
use serde_json::Value;

use super::{
    auth::Caller,
    error::{ApiError, ApiResult},
    json,
    json::ApiJson,
    pagination::{Paged, Pager},
};
use crate::AppState;

/// The path parameters of every hook endpoint: `owner` and `repo`, or `org`.
#[derive(Debug, Deserialize)]
pub struct HookPath {
    owner: Option<String>,
    repo: Option<String>,
    org: Option<String>,
    id: Option<String>,
    delivery: Option<String>,
}

impl HookPath {
    fn hook(&self) -> ApiResult<WebhookId> {
        self.id.as_deref().and_then(|id| id.parse().ok()).map(WebhookId).ok_or_else(ApiError::not_found)
    }

    fn delivery(&self) -> ApiResult<DeliveryId> {
        self.delivery.as_deref().and_then(|id| id.parse().ok()).map(DeliveryId).ok_or_else(ApiError::not_found)
    }
}

/// What the hooks belong to, as the caller sees it.
enum Owner {
    Repository(Box<RepoContext>),
    Organization(OrgContext),
}

impl Owner {
    /// Loads the repository or organisation (404 if hidden), then checks the token scope
    /// and the caller's rights (403).
    async fn load(state: &AppState, caller: &Caller, path: &HookPath) -> ApiResult<Self> {
        let owner = match (&path.owner, &path.repo, &path.org) {
            (Some(owner), Some(repo), _) => Self::Repository(Box::new(caller.open_repo(state, owner, repo).await?)),
            (_, _, Some(org)) => Self::Organization(state.services.orgs.open(caller.user(), org).await?),
            _ => return Err(ApiError::not_found()),
        };
        caller.require(TokenScope::Admin)?;
        if !owner.target().can_manage() {
            return Err(ApiError::forbidden(match owner {
                Self::Repository(_) => "You must be an administrator of this repository",
                Self::Organization(_) => "You must be an owner of this organization",
            }));
        }
        Ok(owner)
    }

    fn target(&self) -> HookTarget<'_> {
        match self {
            Self::Repository(ctx) => HookTarget::Repository(ctx),
            Self::Organization(ctx) => HookTarget::Organization(ctx),
        }
    }

    fn hook_url(&self, state: &AppState, id: WebhookId) -> String {
        self.target().hook_api_url(state.config.server.base_url(), id)
    }
}

/// `config` of a hook body (GitHub's shape).
#[derive(Debug, Default, Deserialize)]
pub struct HookConfig {
    url: Option<String>,
    content_type: Option<String>,
    /// A string sets it, `""` or `null` removes it, absent keeps it.
    #[serde(default, deserialize_with = "json::present")]
    secret: Option<Value>,
}

/// The body of `POST …/hooks` and `PATCH …/hooks/{id}`.
#[derive(Debug, Default, Deserialize)]
pub struct HookBody {
    #[serde(default)]
    config: HookConfig,
    events: Option<Vec<String>>,
    active: Option<bool>,
}

impl HookBody {
    fn secret(&self) -> ApiResult<SecretUpdate> {
        match &self.config.secret {
            None => Ok(SecretUpdate::Keep),
            Some(Value::Null) => Ok(SecretUpdate::Clear),
            Some(Value::String(secret)) if secret.is_empty() => Ok(SecretUpdate::Clear),
            Some(Value::String(secret)) => Ok(SecretUpdate::Set(secret.clone())),
            Some(_) => Err(ApiError::invalid("config.secret", "secret must be a string")),
        }
    }

    fn check_content_type(&self) -> ApiResult<()> {
        match self.config.content_type.as_deref() {
            None | Some("json") => Ok(()),
            Some(_) => Err(ApiError::invalid("config.content_type", "only json payloads are supported")),
        }
    }
}

/// `GET …/hooks`
pub async fn list(State(state): State<AppState>, caller: Caller, pager: Pager, Path(path): Path<HookPath>) -> ApiResult<Paged<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let hooks = state.services.webhooks.list(owner.target()).await?;
    Ok(pager.page_of(hooks.iter().map(|hook| json::hook(hook, &owner.hook_url(&state, hook.id))).collect()))
}

/// `POST …/hooks`: `201 Created`; a `ping` is queued.
pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<HookPath>,
    ApiJson(body): ApiJson<HookBody>,
) -> ApiResult<Response> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let user = caller.require_user()?;
    body.check_content_type()?;
    let input = WebhookInput {
        url: body.config.url.clone().ok_or_else(|| ApiError::invalid("config.url", "url is required"))?,
        events: body.events.clone().unwrap_or_else(|| vec!["push".to_owned()]),
        active: body.active.unwrap_or(true),
        secret: body.secret()?,
    };
    let hook = state.services.webhooks.create(user, owner.target(), input).await.map_err(|error| field_error(error, "config.url"))?;
    let url = owner.hook_url(&state, hook.id);
    Ok((StatusCode::CREATED, [(header::LOCATION, url.clone())], Json(json::hook(&hook, &url))).into_response())
}

/// Validation messages mention what they are about; map them to the closest field.
fn field_error(error: rustit_services::ServiceError, default: &str) -> ApiError {
    let field = match &error {
        rustit_services::ServiceError::Invalid(message) if message.contains("event") => "events",
        rustit_services::ServiceError::Invalid(message) if message.contains("secret") => "config.secret",
        _ => default,
    };
    ApiError::service(error, field)
}

/// `GET …/hooks/{id}`
pub async fn show(State(state): State<AppState>, caller: Caller, Path(path): Path<HookPath>) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let hook = state.services.webhooks.get(owner.target(), path.hook()?).await?;
    Ok(Json(json::hook(&hook, &owner.hook_url(&state, hook.id))))
}

/// `PATCH …/hooks/{id}`: absent fields keep their value.
pub async fn update(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<HookPath>,
    ApiJson(body): ApiJson<HookBody>,
) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let user = caller.require_user()?;
    body.check_content_type()?;
    let webhooks = &state.services.webhooks;
    let current = webhooks.get(owner.target(), path.hook()?).await?;
    let input = WebhookInput {
        url: body.config.url.clone().unwrap_or_else(|| current.url.as_str().to_owned()),
        events: body.events.clone().unwrap_or_else(|| current.events.iter().map(|event| event.as_str().to_owned()).collect()),
        active: body.active.unwrap_or(current.active),
        secret: body.secret()?,
    };
    let hook = webhooks.update(user, owner.target(), current.id, input).await.map_err(|error| field_error(error, "config.url"))?;
    Ok(Json(json::hook(&hook, &owner.hook_url(&state, hook.id))))
}

/// `DELETE …/hooks/{id}`
pub async fn delete(State(state): State<AppState>, caller: Caller, Path(path): Path<HookPath>) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    state.services.webhooks.delete(caller.require_user()?, owner.target(), path.hook()?).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST …/hooks/{id}/pings`: `204`; the ping is queued.
pub async fn ping(State(state): State<AppState>, caller: Caller, Path(path): Path<HookPath>) -> ApiResult<StatusCode> {
    let owner = Owner::load(&state, &caller, &path).await?;
    state.services.webhooks.ping(caller.require_user()?, owner.target(), path.hook()?).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET …/hooks/{id}/deliveries`, newest first.
pub async fn deliveries(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(path): Path<HookPath>,
) -> ApiResult<Paged<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let id = path.hook()?;
    let log = state.services.webhooks.deliveries(owner.target(), id, pager.page, pager.per_page).await?;
    let hook_url = owner.hook_url(&state, id);
    Ok(pager.paged(log.deliveries.iter().map(|delivery| json::delivery(delivery, &hook_url)).collect(), Some(log.total), false))
}

/// `GET …/hooks/{id}/deliveries/{delivery}`: with the request and response.
pub async fn delivery(State(state): State<AppState>, caller: Caller, Path(path): Path<HookPath>) -> ApiResult<Json<Value>> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let id = path.hook()?;
    let delivery = state.services.webhooks.delivery(owner.target(), id, path.delivery()?).await?;
    Ok(Json(json::delivery_detail(&delivery, &owner.hook_url(&state, id))))
}

/// `POST …/hooks/{id}/deliveries/{delivery}/attempts`: `202 Accepted` with the new delivery.
pub async fn redeliver(State(state): State<AppState>, caller: Caller, Path(path): Path<HookPath>) -> ApiResult<Response> {
    let owner = Owner::load(&state, &caller, &path).await?;
    let id = path.hook()?;
    let again = state.services.webhooks.redeliver(caller.require_user()?, owner.target(), id, path.delivery()?).await?;
    Ok((StatusCode::ACCEPTED, Json(json::delivery(&again, &owner.hook_url(&state, id)))).into_response())
}
