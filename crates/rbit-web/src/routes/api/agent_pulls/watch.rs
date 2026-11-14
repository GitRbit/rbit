//! A bounded, cursor-based view of a pull request's checks, reviews and lifecycle.

use std::{
    hash::{DefaultHasher, Hash, Hasher},
    time::Duration,
};

use axum::{
    Json,
    extract::{Path, Query, State, rejection::QueryRejection},
    http::{HeaderMap, header},
};
use rbit_services::{
    ServiceError,
    pulls::{PullContext, ReviewData, inbox::excerpt},
    tokens::Right,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::{Instant, sleep_until};

use super::{PullPath, open_pull, queue::place_json, readiness_json};
use crate::{
    AppState,
    routes::api::{
        REQUEST_TIMEOUT,
        agent_review::thread_items,
        auth::Caller,
        error::{ApiError, ApiResult},
    },
};

const MAX_WAIT: u64 = 25;
const _: () = assert!(MAX_WAIT + 3 <= REQUEST_TIMEOUT.as_secs());
const LOOK_EVERY: Duration = Duration::from_secs(2);
const REVIEW_LIMIT: usize = 50;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct WatchQuery {
    cursor: Option<String>,
    timeout: Option<u64>,
}

/// Get a snapshot immediately, or wait for it to differ from a previous cursor.
pub(super) async fn watch(
    State(state): State<AppState>,
    mut caller: Caller,
    Path(path): Path<PullPath>,
    query: Result<Query<WatchQuery>, QueryRejection>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let Query(query) = query?;
    if query.cursor.as_ref().is_some_and(|cursor| cursor.len() != 16 || !cursor.bytes().all(|byte| byte.is_ascii_hexdigit())) {
        return Err(ApiError::invalid("cursor", "use the cursor returned by a previous PR watch"));
    }

    let deadline = Instant::now() + Duration::from_secs(query.timeout.unwrap_or(MAX_WAIT).min(MAX_WAIT));
    let pulls = &state.services.pulls;
    loop {
        let mut ctx = open_pull(&state, &caller, &path).await?;
        redact_inaccessible_head(&state, &caller, &mut ctx).await?;
        let (reviews, queue) = tokio::try_join!(pulls.reviews(caller.user(), &ctx), pulls.queue_place(&ctx))?;
        let readiness = pulls.merge_readiness(caller.user(), &ctx, Some(&reviews)).await?;
        let mut body = json!({
            "schema": "rbit.pr-watch/1",
            "number": ctx.issue().number.get(),
            "state": if ctx.pull.is_merged() { "merged" } else { ctx.issue().state.as_str() },
            "head": ctx.pull.head_sha,
            "head_branch": ctx.pull.head_branch,
            "base": ctx.pull.base_branch,
            "base_sha": readiness.base_tip,
            "merge_commit": ctx.pull.merge_commit_sha,
            "mergeability": readiness.mergeable.as_str(),
            "conflict_paths": readiness.conflict_paths,
            "readiness": readiness_json(&ctx, &readiness),
            "readiness_checks_protection": caller.user().is_some() && ctx.repo().access.can_write(),
            "can_request_merge": caller.user().is_some() && ctx.repo().can_write_content() && caller.require_right(Right::Merge).is_ok(),
            "queue": queue.as_ref().map(place_json),
            "discussion_count": ctx.issue().comment_count,
            "requested_reviewers": reviews.requested.iter().map(|account| account.name.as_str()).collect::<Vec<_>>(),
        });
        let (cursor, thread_count) = fingerprint(&ctx, &body, &reviews);
        let changed = query.cursor.as_deref() != Some(cursor.as_str());
        if changed || Instant::now() >= deadline {
            body["cursor"] = json!(cursor);
            body["changed"] = json!(changed);
            body["review_count"] = json!(reviews.reviews.len());
            body["reviews"] = json!(review_items(&reviews));
            body["thread_count"] = json!(thread_count);
            body["threads"] = json!(thread_items(&pulls.inbox_from(&ctx, &reviews).await));
            return Ok(Json(body));
        }

        sleep_until(deadline.min(Instant::now() + LOOK_EVERY)).await;
        caller.refresh_watch(&state, headers.get(header::AUTHORIZATION)).await?;
    }
}

/// The review and its imported commit remain part of the base repository. Checking
/// whether a branch still exists in another repository requires read access there.
async fn redact_inaccessible_head(state: &AppState, caller: &Caller, ctx: &mut PullContext) -> ApiResult<()> {
    let Some(head) = ctx.head_repo.as_ref().filter(|head| head.repo.id != ctx.repo().repo.repo.id) else { return Ok(()) };
    let accessible = if caller.reaches(head.repo.id) {
        match state.services.repositories.open(caller.user(), head.owner.name.as_str(), head.repo.name.as_str()).await {
            Ok(opened) => opened.repo.repo.id == head.repo.id,
            Err(ServiceError::NotFound) => false,
            Err(error) => return Err(error.into()),
        }
    } else {
        false
    };
    if !accessible {
        ctx.head_repo = None;
    }
    Ok(())
}

/// All submitted review activity participates, including findings beyond the response
/// limits. Pending reviews and the presentation of outdated anchors do not participate.
fn fingerprint(ctx: &PullContext, body: &Value, reviews: &ReviewData) -> (String, usize) {
    let mut hasher = DefaultHasher::new();
    ctx.issue().id.0.hash(&mut hasher);
    body.hash(&mut hasher);
    let mut thread_count = 0;
    for entry in &reviews.reviews {
        let review = &entry.review;
        (review.id.0, review.state.as_str(), &review.commit_sha, &review.body, review.dismissed_at, &review.dismissal_message)
            .hash(&mut hasher);
        for (comment, _) in &entry.comments {
            // A new reply, an edit, or a resolution changes the cursor even when the
            // corresponding review and its overall verdict stay the same.
            (comment.id.0, comment.resolved_at, &comment.body).hash(&mut hasher);
            thread_count += usize::from(comment.in_reply_to.is_none() && comment.resolved_at.is_none());
        }
    }
    (format!("{:016x}", hasher.finish()), thread_count)
}

fn review_items(reviews: &ReviewData) -> Vec<Value> {
    let newest = reviews.reviews.len().saturating_sub(REVIEW_LIMIT);
    reviews.reviews[newest..]
        .iter()
        .map(|entry| {
            let review = &entry.review;
            json!({
                "id": review.id.0,
                "author": entry.reviewer.as_ref().map(|account| account.name.as_str()),
                "state": review.state.as_str(),
                "commit": review.commit_sha,
                "says": excerpt(&review.body),
            })
        })
        .collect()
}
