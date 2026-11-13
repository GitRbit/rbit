//! Personal access tokens over the API (agent plan step 4.8): the owner lists, creates and
//! revokes them, and may limit one to a repository and some rights. Only an unlimited token
//! with the `admin` scope may: a limited token can never make a broader one.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rbit_core::{AccessToken, AccessTokenId, TokenScope};
use rbit_services::{
    CreateAccessToken,
    tokens::{DELETED_REPOSITORY, Right, TokenLimit},
    webhooks::payloads::timestamp,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    ApiMethod::{Delete, Get, Post},
    ApiRoute,
    auth::Caller,
    error::{ApiError, ApiResult},
    json::ApiJson,
    route,
};
use crate::AppState;

/// This area's endpoints, appended to [`super::table`].
pub(super) fn routes() -> Vec<ApiRoute> {
    vec![
        route(Get, "/user/tokens".to_owned(), list),
        route(Post, "/user/tokens".to_owned(), create),
        route(Delete, "/user/tokens/{id}".to_owned(), revoke),
    ]
}

/// The caller, if they may manage their tokens: an unlimited `admin` token.
fn owner(caller: &Caller) -> ApiResult<&rbit_core::User> {
    let user = caller.require(TokenScope::Admin)?;
    if caller.is_limited() {
        return Err(ApiError::forbidden("A limited token cannot manage tokens"));
    }
    Ok(user)
}

fn token_json(token: &AccessToken, limit: Option<&TokenLimit>, repository: Option<&str>) -> Value {
    json!({
        "id": token.id.0,
        "name": token.name,
        "hint": token.hint,
        "scope": token.scope.as_str(),
        "created_at": timestamp(token.created_at),
        "last_used_at": token.last_used_at.map(timestamp),
        "expires_at": token.expires_at.map(timestamp),
        "repository": repository,
        // A deleted repository shows as none; the token reaches no repository.
        "repository_id": limit.and_then(|limit| limit.repo).filter(|repo| *repo != DELETED_REPOSITORY).map(|repo| repo.0),
        "rights": limit.map(|limit| limit.rights.iter().map(|right| right.as_str()).collect::<Vec<_>>()),
    })
}

/// `GET /user/tokens`: the caller's tokens, newest first; never their secrets.
pub async fn list(State(state): State<AppState>, caller: Caller) -> ApiResult<Json<Value>> {
    let user = owner(&caller)?;
    let tokens = state.services.tokens.list(user).await?;
    let mut items = Vec::with_capacity(tokens.len());
    for token in &tokens {
        let limit = state.services.tokens.limit_of(token.id).await?;
        items.push(token_json(token, limit.as_ref(), None));
    }
    Ok(Json(json!({ "schema": "rbit.tokens/1", "tokens": items })))
}

#[derive(Debug, Deserialize)]
pub struct NewToken {
    name: String,
    scope: String,
    #[serde(default)]
    expires_in_days: Option<u32>,
    /// `owner/name`: the one repository the token reaches.
    #[serde(default)]
    repository: Option<String>,
    /// Rights within the scope; every right when a repository is given without them.
    #[serde(default)]
    rights: Option<Vec<String>>,
}

/// `POST /user/tokens`: `201` with the new token's `token` (shown this once).
pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    ApiJson(new): ApiJson<NewToken>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let user = owner(&caller)?;
    let scope = TokenScope::parse(&new.scope).ok_or_else(|| ApiError::invalid("scope", "scope must be read, write or admin"))?;
    let input = CreateAccessToken { name: new.name, scope, expires_in_days: new.expires_in_days };
    let repository = match new.repository.as_deref().filter(|name| !name.is_empty()) {
        Some(full) => {
            let (owner_name, repo_name) =
                full.split_once('/').ok_or_else(|| ApiError::invalid("repository", "repository must be owner/name"))?;
            let repo = caller.open_repo(&state, owner_name, repo_name).await?;
            Some((repo.repo.repo.id, repo.repo.full_name()))
        }
        None => None,
    };
    let rights = match &new.rights {
        Some(names) => {
            let parsed: Option<Vec<Right>> = names.iter().map(|name| Right::parse(name)).collect();
            Some(parsed.ok_or_else(|| ApiError::invalid("rights", "rights are push, statuses, merge, dispatch, approve and read_logs"))?)
        }
        None => None,
    };
    let limit = match (&repository, rights) {
        (None, None) => None,
        (repository, rights) => {
            Some(TokenLimit { repo: repository.as_ref().map(|(id, _)| *id), rights: rights.unwrap_or_else(|| Right::ALL.to_vec()) })
        }
    };
    let (token, secret) = match &limit {
        Some(limit) => state.services.tokens.create_limited(user, input, limit).await?,
        None => state.services.tokens.create(user, input).await?,
    };
    let mut value = token_json(&token, limit.as_ref(), repository.as_ref().map(|(_, name)| name.as_str()));
    value["token"] = json!(secret);
    value["schema"] = json!("rbit.token/1");
    Ok((StatusCode::CREATED, Json(value)))
}

/// `DELETE /user/tokens/{id}`: revokes one of the caller's tokens at once (`204`).
pub async fn revoke(State(state): State<AppState>, caller: Caller, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let user = owner(&caller)?;
    state.services.tokens.revoke(user, AccessTokenId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
