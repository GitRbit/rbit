//! `GET` and `PUT /repos/{owner}/{repo}/actions/supersede` (rbit's own): whether a newer push
//! to a branch other than the default one cancels that branch's older runs of the same
//! workflow. `{"enabled": true}`; readers may look, administrators change it (`admin` scope).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rbit_core::TokenScope;
use serde::Deserialize;
use serde_json::{Value, json};

use super::actions;
use crate::{
    AppState,
    routes::api::{
        auth::Caller,
        error::{ApiError, ApiResult},
    },
};

#[derive(Debug, Deserialize)]
pub struct RepoPath {
    owner: String,
    repo: String,
}

#[derive(Debug, Deserialize)]
pub struct SupersedeBody {
    enabled: bool,
}

/// `GET /repos/{owner}/{repo}/actions/supersede`
pub async fn show(State(state): State<AppState>, caller: Caller, Path(path): Path<RepoPath>) -> ApiResult<Json<Value>> {
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    Ok(Json(json!({ "enabled": actions(&state)?.supersede(&ctx).await? })))
}

/// `PUT /repos/{owner}/{repo}/actions/supersede`: `204`.
pub async fn update(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<RepoPath>,
    body: Result<Json<SupersedeBody>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<StatusCode> {
    let Json(body) = body.map_err(ApiError::from)?;
    let ctx = caller.open_repo(&state, &path.owner, &path.repo).await?;
    let user = caller.require(TokenScope::Admin)?;
    actions(&state)?.set_supersede(user, &ctx, body.enabled).await?;
    Ok(StatusCode::NO_CONTENT)
}
