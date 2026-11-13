//! Dismissing a review (`PUT …/pulls/{number}/reviews/{id}/dismissals`), apart from the
//! other pull request endpoints so that file stays within its size.

use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;
use serde_json::Value;

use super::{ApiError, ApiJson, ApiResult, Caller, Right, base_url, open, pull_json, review_id, writer};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub(super) struct Dismissal {
    message: String,
}

/// `PUT /repos/{owner}/{repo}/pulls/{number}/reviews/{id}/dismissals`
pub(super) async fn dismiss(
    State(state): State<AppState>,
    caller: Caller,
    Path((owner, name, number, id)): Path<(String, String, String, String)>,
    ApiJson(dismissal): ApiJson<Dismissal>,
) -> ApiResult<Json<Value>> {
    caller.require_right(Right::Approve)?;
    let id = review_id(&id)?;
    let ctx = open(&state, &caller, &owner, &name, &number).await?;
    let user = writer(&caller)?;
    let review = state
        .services
        .pulls
        .dismiss_review(user, &ctx, id, &dismissal.message)
        .await
        .map_err(|error| ApiError::service(error, "message"))?;
    let reviewer = match review.reviewer {
        Some(id) if id == user.id() => Some(user.account.clone()),
        Some(_) => state
            .services
            .pulls
            .reviews(Some(user), &ctx)
            .await?
            .reviews
            .into_iter()
            .find(|entry| entry.review.id == id)
            .and_then(|entry| entry.reviewer),
        None => None,
    };
    Ok(Json(pull_json::review(&review, reviewer.as_ref(), &ctx.repo().repo, ctx.issue().number.get(), base_url(&state))))
}
