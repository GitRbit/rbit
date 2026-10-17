//! Users, organisations, their repositories and stars.

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rbit_core::{TokenScope, Visibility, repository::OwnedRepository};
use rbit_services::CreateRepository;
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

/// `GET /version`
pub async fn version(_caller: Caller) -> Json<Value> {
    Json(serde_json::json!({ "version": env!("CARGO_PKG_VERSION"), "api": "v1" }))
}

/// `GET /user`
pub async fn me(State(state): State<AppState>, caller: Caller) -> ApiResult<Json<Value>> {
    let user = caller.require_user()?;
    Ok(Json(json::me(user, state.config.server.base_url())))
}

/// `GET /users/{username}`
pub async fn user(State(state): State<AppState>, _caller: Caller, Path(name): Path<String>) -> ApiResult<Json<Value>> {
    let account = state.services.accounts.find_account(&name).await?;
    let profile = state.services.profiles.public_profile(account.id).await?;
    Ok(Json(json::profile(&account, &profile, state.config.server.base_url())))
}

/// `GET /orgs/{org}`
pub async fn organization(State(state): State<AppState>, caller: Caller, Path(name): Path<String>) -> ApiResult<Json<Value>> {
    let ctx = state.services.orgs.open(caller.user(), &name).await?;
    Ok(Json(json::organization(&ctx.org, state.config.server.base_url())))
}

/// `GET /orgs/{org}/members`
pub async fn members(State(state): State<AppState>, caller: Caller, pager: Pager, Path(name): Path<String>) -> ApiResult<Paged<Value>> {
    let ctx = state.services.orgs.open(caller.user(), &name).await?;
    let base = state.config.server.base_url();
    let members = state.services.orgs.members(&ctx.org).await?;
    Ok(pager.page_of(members.iter().map(|member| json::member(member, base)).collect()))
}

/// One page of a complete list of repositories.
async fn repositories(state: &AppState, pager: &Pager, repos: &[OwnedRepository]) -> ApiResult<Paged<Value>> {
    let page: Vec<&OwnedRepository> = repos.iter().skip(pager.offset()).take(pager.limit()).collect();
    let items = repository_values(state, &page).await?;
    Ok(pager.paged(items, Some(u64::try_from(repos.len()).unwrap_or(u64::MAX)), false))
}

/// List entries (no `permissions`), with their `fork` flags read in one query.
async fn repository_values(state: &AppState, repos: &[&OwnedRepository]) -> ApiResult<Vec<Value>> {
    let base = state.config.server.base_url();
    let ids: Vec<_> = repos.iter().map(|repo| repo.repo.id).collect();
    let forks = state.services.forks.forks_among(&ids).await?;
    Ok(repos.iter().map(|repo| json::repository(repo, None, forks.contains(&repo.repo.id), base)).collect())
}

/// `GET /user/repos`: repositories the caller owns, reaches through organisations or
/// collaborates on, paged in the database.
pub async fn my_repositories(State(state): State<AppState>, caller: Caller, pager: Pager) -> ApiResult<Paged<Value>> {
    let user = caller.require_user()?;
    let limit = u32::try_from(pager.limit()).unwrap_or(u32::MAX);
    let offset = u32::try_from(pager.offset()).unwrap_or(u32::MAX);
    let (repos, total) = state.services.repositories.page_for_member(user, limit, offset).await?;
    let items = repository_values(&state, &repos.iter().collect::<Vec<_>>()).await?;
    Ok(pager.paged(items, Some(total), false))
}

/// `GET /users/{username}/repos`
pub async fn user_repositories(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(name): Path<String>,
) -> ApiResult<Paged<Value>> {
    let account = state.services.accounts.find_account(&name).await?;
    let repos = state.services.repositories.list_for_owner(caller.user(), &account).await?;
    repositories(&state, &pager, &repos).await
}

/// `GET /orgs/{org}/repos`
pub async fn org_repositories(
    State(state): State<AppState>,
    caller: Caller,
    pager: Pager,
    Path(name): Path<String>,
) -> ApiResult<Paged<Value>> {
    let ctx = state.services.orgs.open(caller.user(), &name).await?;
    let repos = state.services.repositories.list_for_owner(caller.user(), &ctx.org.account).await?;
    repositories(&state, &pager, &repos).await
}

/// The body of `POST /user/repos` and `POST /orgs/{org}/repos`.
#[derive(Debug, Deserialize)]
pub struct NewRepository {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    private: Option<bool>,
    #[serde(default)]
    visibility: Option<String>,
}

impl NewRepository {
    fn input(self) -> ApiResult<CreateRepository> {
        let visibility = match (self.visibility.as_deref(), self.private) {
            (Some(value), _) => {
                Visibility::parse(value).ok_or_else(|| ApiError::invalid("visibility", "visibility must be public or private"))?
            }
            (None, Some(true)) => Visibility::Private,
            (None, _) => Visibility::Public,
        };
        Ok(CreateRepository { name: self.name, description: self.description.unwrap_or_default(), visibility })
    }
}

/// `201 Created` with the repository and its `Location`.
fn created(state: &AppState, repo: &OwnedRepository) -> Response {
    let value = json::repository(repo, Some(rbit_core::RepoAccess::Admin), false, state.config.server.base_url());
    let location = value["url"].as_str().unwrap_or_default().to_owned();
    (StatusCode::CREATED, [(header::LOCATION, location)], Json(value)).into_response()
}

/// `POST /user/repos`
pub async fn create_for_user(State(state): State<AppState>, caller: Caller, ApiJson(body): ApiJson<NewRepository>) -> ApiResult<Response> {
    let user = caller.require(TokenScope::Write)?;
    let repo = state.services.repositories.create(user, body.input()?).await.map_err(|error| ApiError::service(error, "name"))?;
    Ok(created(&state, &repo))
}

/// `POST /orgs/{org}/repos`: members of the organisation (and site administrators).
pub async fn create_for_org(
    State(state): State<AppState>,
    caller: Caller,
    Path(name): Path<String>,
    ApiJson(body): ApiJson<NewRepository>,
) -> ApiResult<Response> {
    let ctx = state.services.orgs.open(caller.user(), &name).await?;
    let user = caller.require(TokenScope::Write)?;
    if !ctx.is_member() && !user.is_admin {
        return Err(ApiError::forbidden(format!("You cannot create repositories in {}", ctx.org.name())));
    }
    let repo = state
        .services
        .repositories
        .create_in(user, ctx.org.name().as_str(), body.input()?)
        .await
        .map_err(|error| ApiError::service(error, "name"))?;
    Ok(created(&state, &repo))
}

/// `GET /user/starred`
pub async fn starred(State(state): State<AppState>, caller: Caller, pager: Pager) -> ApiResult<Paged<Value>> {
    let user = caller.require_user()?;
    let repos = state.services.stars.starred_by(Some(user), &user.account).await?;
    repositories(&state, &pager, &repos).await
}

/// `PUT /user/starred/{owner}/{repo}`
pub async fn star(State(state): State<AppState>, caller: Caller, Path((owner, name)): Path<(String, String)>) -> ApiResult<StatusCode> {
    let ctx = caller.open_repo(&state, &owner, &name).await?;
    let user = caller.require(TokenScope::Write)?;
    state.services.stars.star(user, &ctx).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /user/starred/{owner}/{repo}`
pub async fn unstar(State(state): State<AppState>, caller: Caller, Path((owner, name)): Path<(String, String)>) -> ApiResult<StatusCode> {
    let ctx = caller.open_repo(&state, &owner, &name).await?;
    let user = caller.require(TokenScope::Write)?;
    state.services.stars.unstar(user, &ctx).await?;
    Ok(StatusCode::NO_CONTENT)
}
