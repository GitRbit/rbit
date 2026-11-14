//! The merge queue's endpoints (agent plan step 4.1): queue, place in line, dequeue.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rbit_core::{TokenScope, issue::MergeStrategy};
use rbit_services::{
    pulls::{MergeFailure, queue::QueuePlace},
    tokens::Right,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{PullPath, open_pull};
use crate::{
    AppState,
    routes::api::{
        auth::Caller,
        error::{ApiError, ApiResult},
        json::ApiJson,
    },
};

#[derive(Debug, Default, Deserialize)]
pub struct QueueRequest {
    /// The head the caller expects to land.
    #[serde(default)]
    head: Option<String>,
    /// The target branch the caller reviewed.
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    merge_method: Option<String>,
}

/// A place in line as the queue endpoints answer it.
pub(super) fn place_json(place: &QueuePlace) -> Value {
    let entry = &place.entry;
    json!({
        "schema": "rbit.queue/1",
        "number": entry.number,
        "state": entry.state,
        "position": place.position,
        "head": entry.head_sha,
        "base": entry.base_branch,
        "merge_method": entry.strategy,
        "candidate": entry.candidate_sha,
        "built_on": entry.base_sha,
        "attempts": entry.attempts,
        "reason": entry.reason,
    })
}

/// `POST /repos/{owner}/{repo}/pulls/{number}/queue {"head": "<sha>", "merge_method": "merge"}`:
/// puts the pull request in line to land once its merge on the current base passed CI.
pub async fn enqueue(
    State(state): State<AppState>,
    caller: Caller,
    Path(path): Path<PullPath>,
    ApiJson(request): ApiJson<QueueRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    caller.require_right(Right::Merge)?;
    let user = caller.require(TokenScope::Write)?;
    let ctx = open_pull(&state, &caller, &path).await?;
    // The service queues exactly ctx.pull.base_branch and checks it again at landing.
    // A retarget before this read must never send the reviewed head into a new line.
    if request.base.as_ref().is_some_and(|base| *base != ctx.pull.base_branch) {
        return Err(ApiError::new(StatusCode::CONFLICT, format!("the base branch is {}", ctx.pull.base_branch)).with_code("base_changed"));
    }
    let method = request.merge_method.as_deref().unwrap_or("merge");
    let strategy =
        MergeStrategy::parse(method).ok_or_else(|| ApiError::invalid("merge_method", "merge_method must be merge, squash or rebase"))?;
    match state.services.pulls.enqueue(user, &ctx, strategy, request.head.as_deref().filter(|head| !head.is_empty())).await {
        Ok(place) => Ok((StatusCode::ACCEPTED, Json(place_json(&place)))),
        Err(MergeFailure::HeadChanged | MergeFailure::BaseChanged) => {
            Err(ApiError::new(StatusCode::CONFLICT, format!("the head is {}", ctx.pull.head_sha)).with_code("head_changed"))
        }
        Err(MergeFailure::Blocked(message)) => Err(ApiError::new(StatusCode::METHOD_NOT_ALLOWED, message)),
        Err(MergeFailure::Service(error)) => Err(error.into()),
    }
}

/// `GET /repos/{owner}/{repo}/pulls/{number}/queue`: its newest entry and place in line.
pub async fn queue_place(State(state): State<AppState>, caller: Caller, Path(path): Path<PullPath>) -> ApiResult<Json<Value>> {
    let ctx = open_pull(&state, &caller, &path).await?;
    let place = state.services.pulls.queue_place(&ctx).await?.ok_or_else(ApiError::not_found)?;
    Ok(Json(place_json(&place)))
}

/// `DELETE /repos/{owner}/{repo}/pulls/{number}/queue`: takes it out of line (`204`), `404`
/// when it was not in line.
pub async fn dequeue(State(state): State<AppState>, caller: Caller, Path(path): Path<PullPath>) -> ApiResult<StatusCode> {
    caller.require_right(Right::Merge)?;
    let user = caller.require(TokenScope::Write)?;
    let ctx = open_pull(&state, &caller, &path).await?;
    if state.services.pulls.dequeue(user, &ctx).await? { Ok(StatusCode::NO_CONTENT) } else { Err(ApiError::not_found()) }
}
