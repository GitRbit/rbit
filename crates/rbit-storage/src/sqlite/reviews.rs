//! Pull request reviews, review comments on diff lines and review requests.
//!
//! Milestone 2 workstream P (pull requests) owns this file. Keep row types here rather than in `rows.rs`.
//!
//! A review is **pending** while its author collects comments (one pending review per
//! user and pull request, enforced by a partial unique index) and becomes `commented`,
//! `approved` or `changes_requested` when submitted. Dismissing turns an approval or a
//! change request into `dismissed`, keeping who dismissed it and why. Submitting a review
//! clears its author's review request, in the same transaction.

use std::collections::HashMap;

use rbit_core::{
    AccountId, IssueId, ReviewId, TimelineEvent, Timestamp,
    issue::ReviewState,
    pull::{Review, ReviewComment, ReviewCommentId, ReviewSide},
};
use sqlx::{SqliteConnection, SqlitePool};

use crate::StorageError;

macro_rules! review_columns {
    () => {
        "id, pull_id, reviewer_id, state, body, commit_sha, created_at, submitted_at, dismissed_by, dismissed_at, dismissal_message"
    };
}

macro_rules! comment_columns {
    () => {
        "id, review_id, pull_id, author_id, path, side, line, commit_sha, diff_hunk, in_reply_to, body, outdated, created_at, updated_at, resolved_at, resolved_by"
    };
}

fn corrupt(what: &str, detail: impl std::fmt::Display) -> StorageError {
    StorageError::Corrupt(format!("{what}: {detail}"))
}

fn json_array(values: impl IntoIterator<Item = i64>) -> String {
    let values: Vec<i64> = values.into_iter().collect();
    serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_owned())
}

#[derive(Debug, sqlx::FromRow)]
struct ReviewRow {
    id: i64,
    pull_id: i64,
    reviewer_id: Option<i64>,
    state: String,
    body: String,
    commit_sha: String,
    created_at: i64,
    submitted_at: Option<i64>,
    dismissed_by: Option<i64>,
    dismissed_at: Option<i64>,
    dismissal_message: Option<String>,
}

impl TryFrom<ReviewRow> for Review {
    type Error = StorageError;

    fn try_from(row: ReviewRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: ReviewId(row.id),
            pull_id: IssueId(row.pull_id),
            reviewer: row.reviewer_id.map(AccountId),
            state: ReviewState::parse(&row.state).ok_or_else(|| corrupt("review state", &row.state))?,
            body: row.body,
            commit_sha: row.commit_sha,
            created_at: Timestamp(row.created_at),
            submitted_at: row.submitted_at.map(Timestamp),
            dismissed_by: row.dismissed_by.map(AccountId),
            dismissed_at: row.dismissed_at.map(Timestamp),
            dismissal_message: row.dismissal_message,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct CommentRow {
    id: i64,
    review_id: i64,
    pull_id: i64,
    author_id: Option<i64>,
    path: String,
    side: String,
    line: i64,
    commit_sha: String,
    diff_hunk: String,
    in_reply_to: Option<i64>,
    body: String,
    outdated: bool,
    resolved_at: Option<i64>,
    resolved_by: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

impl TryFrom<CommentRow> for ReviewComment {
    type Error = StorageError;

    fn try_from(row: CommentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: ReviewCommentId(row.id),
            review_id: ReviewId(row.review_id),
            pull_id: IssueId(row.pull_id),
            author: row.author_id.map(AccountId),
            path: row.path,
            side: ReviewSide::parse(&row.side).ok_or_else(|| corrupt("review side", &row.side))?,
            line: u32::try_from(row.line).map_err(|e| corrupt("review line", e))?,
            commit_sha: row.commit_sha,
            diff_hunk: row.diff_hunk,
            in_reply_to: row.in_reply_to.map(ReviewCommentId),
            body: row.body,
            outdated: row.outdated,
            resolved_at: row.resolved_at.map(Timestamp),
            resolved_by: row.resolved_by.map(AccountId),
            created_at: Timestamp(row.created_at),
            updated_at: Timestamp(row.updated_at),
        })
    }
}

/// A comment on a diff line, before it is stored.
#[derive(Debug, Clone)]
pub struct NewReviewComment {
    pub path: String,
    pub side: ReviewSide,
    pub line: u32,
    pub commit_sha: String,
    pub diff_hunk: String,
    pub in_reply_to: Option<ReviewCommentId>,
    pub body: String,
}

/// A review submitted at once ("Add single comment", or a review without pending comments).
#[derive(Debug, Clone)]
pub struct NewReview<'a> {
    pub pull: IssueId,
    pub reviewer: AccountId,
    pub state: ReviewState,
    pub body: &'a str,
    pub commit_sha: &'a str,
    pub at: Timestamp,
}

#[derive(Debug, Clone)]
pub struct ReviewStore {
    pool: SqlitePool,
}

impl ReviewStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Every review of `pull`, pending ones included, oldest first.
    pub async fn reviews(&self, pull: IssueId) -> Result<Vec<Review>, StorageError> {
        let rows: Vec<ReviewRow> =
            sqlx::query_as(concat!("SELECT ", review_columns!(), " FROM pull_reviews WHERE pull_id = ? ORDER BY created_at, id"))
                .bind(pull.0)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(Review::try_from).collect()
    }

    /// The submitted reviews of each of `pulls` (for list rows), oldest first.
    pub async fn submitted_among(&self, pulls: &[IssueId]) -> Result<HashMap<IssueId, Vec<Review>>, StorageError> {
        let mut map: HashMap<IssueId, Vec<Review>> = HashMap::new();
        if pulls.is_empty() {
            return Ok(map);
        }
        let rows: Vec<ReviewRow> = sqlx::query_as(concat!(
            "SELECT ",
            review_columns!(),
            " FROM pull_reviews WHERE pull_id IN (SELECT value FROM json_each(?)) AND state <> 'pending' ORDER BY created_at, id"
        ))
        .bind(json_array(pulls.iter().map(|id| id.0)))
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            let review = Review::try_from(row)?;
            map.entry(review.pull_id).or_default().push(review);
        }
        Ok(map)
    }

    pub async fn find(&self, pull: IssueId, id: ReviewId) -> Result<Option<Review>, StorageError> {
        let row: Option<ReviewRow> =
            sqlx::query_as(concat!("SELECT ", review_columns!(), " FROM pull_reviews WHERE pull_id = ? AND id = ?"))
                .bind(pull.0)
                .bind(id.0)
                .fetch_optional(&self.pool)
                .await?;
        row.map(Review::try_from).transpose()
    }

    /// `reviewer`'s pending review of `pull`, if any.
    pub async fn pending(&self, pull: IssueId, reviewer: AccountId) -> Result<Option<Review>, StorageError> {
        let row: Option<ReviewRow> = sqlx::query_as(concat!(
            "SELECT ",
            review_columns!(),
            " FROM pull_reviews WHERE pull_id = ? AND reviewer_id = ? AND state = 'pending'"
        ))
        .bind(pull.0)
        .bind(reviewer.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Review::try_from).transpose()
    }

    /// Adds `comment` to `reviewer`'s pending review, starting one on `commit_sha` if
    /// there is none. Returns the review and the comment.
    pub async fn add_pending_comment(
        &self,
        pull: IssueId,
        reviewer: AccountId,
        commit_sha: &str,
        comment: &NewReviewComment,
        at: Timestamp,
    ) -> Result<(Review, ReviewComment), StorageError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO pull_reviews (pull_id, reviewer_id, state, body, commit_sha, created_at) VALUES (?, ?, 'pending', '', ?, ?) \
             ON CONFLICT (pull_id, reviewer_id) WHERE state = 'pending' DO NOTHING",
        )
        .bind(pull.0)
        .bind(reviewer.0)
        .bind(commit_sha)
        .bind(at.0)
        .execute(&mut *tx)
        .await?;
        let review: ReviewRow = sqlx::query_as(concat!(
            "SELECT ",
            review_columns!(),
            " FROM pull_reviews WHERE pull_id = ? AND reviewer_id = ? AND state = 'pending'"
        ))
        .bind(pull.0)
        .bind(reviewer.0)
        .fetch_one(&mut *tx)
        .await?;
        let review = Review::try_from(review)?;
        let comment = insert_comment(&mut tx, review.id, pull, reviewer, comment, at).await?;
        tx.commit().await?;
        Ok((review, comment))
    }

    /// Stores a submitted review with `comments` in one transaction and clears the
    /// reviewer's review request.
    pub async fn create_submitted(
        &self,
        new: &NewReview<'_>,
        comments: &[NewReviewComment],
    ) -> Result<(Review, Vec<ReviewComment>), StorageError> {
        let mut tx = self.pool.begin().await?;
        let row: ReviewRow = sqlx::query_as(concat!(
            "INSERT INTO pull_reviews (pull_id, reviewer_id, state, body, commit_sha, created_at, submitted_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING ",
            review_columns!()
        ))
        .bind(new.pull.0)
        .bind(new.reviewer.0)
        .bind(new.state.as_str())
        .bind(new.body)
        .bind(new.commit_sha)
        .bind(new.at.0)
        .bind(new.at.0)
        .fetch_one(&mut *tx)
        .await?;
        let review = Review::try_from(row)?;
        let mut stored = Vec::with_capacity(comments.len());
        for comment in comments {
            stored.push(insert_comment(&mut tx, review.id, new.pull, new.reviewer, comment, new.at).await?);
        }
        clear_request(&mut tx, new.pull, new.reviewer).await?;
        tx.commit().await?;
        Ok((review, stored))
    }

    /// Submits `reviewer`'s pending review as `state` with `body`. `None` without one.
    pub async fn submit_pending(
        &self,
        pull: IssueId,
        reviewer: AccountId,
        state: ReviewState,
        body: &str,
        commit_sha: &str,
        at: Timestamp,
    ) -> Result<Option<Review>, StorageError> {
        let mut tx = self.pool.begin().await?;
        let row: Option<ReviewRow> = sqlx::query_as(concat!(
            "UPDATE pull_reviews SET state = ?, body = ?, commit_sha = ?, submitted_at = ? \
             WHERE pull_id = ? AND reviewer_id = ? AND state = 'pending' RETURNING ",
            review_columns!()
        ))
        .bind(state.as_str())
        .bind(body)
        .bind(commit_sha)
        .bind(at.0)
        .bind(pull.0)
        .bind(reviewer.0)
        .fetch_optional(&mut *tx)
        .await?;
        if row.is_some() {
            clear_request(&mut tx, pull, reviewer).await?;
        }
        tx.commit().await?;
        row.map(Review::try_from).transpose()
    }

    /// Discards `reviewer`'s pending review and its comments. `true` if there was one.
    pub async fn discard_pending(&self, pull: IssueId, reviewer: AccountId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM pull_reviews WHERE pull_id = ? AND reviewer_id = ? AND state = 'pending'")
            .bind(pull.0)
            .bind(reviewer.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Replaces the body of a submitted review. `None` if it does not exist or is pending.
    pub async fn edit_body(&self, pull: IssueId, id: ReviewId, body: &str) -> Result<Option<Review>, StorageError> {
        let row: Option<ReviewRow> = sqlx::query_as(concat!(
            "UPDATE pull_reviews SET body = ? WHERE pull_id = ? AND id = ? AND state <> 'pending' RETURNING ",
            review_columns!()
        ))
        .bind(body)
        .bind(pull.0)
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Review::try_from).transpose()
    }

    /// Dismisses an approval or a change request. `None` when the review is not one.
    pub async fn dismiss(
        &self,
        pull: IssueId,
        id: ReviewId,
        by: AccountId,
        message: &str,
        at: Timestamp,
    ) -> Result<Option<Review>, StorageError> {
        let row: Option<ReviewRow> = sqlx::query_as(concat!(
            "UPDATE pull_reviews SET state = 'dismissed', dismissed_by = ?, dismissed_at = ?, dismissal_message = ? \
             WHERE pull_id = ? AND id = ? AND state IN ('approved', 'changes_requested') RETURNING ",
            review_columns!()
        ))
        .bind(by.0)
        .bind(at.0)
        .bind(message)
        .bind(pull.0)
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Review::try_from).transpose()
    }

    // ------------------------------------------------------------- comments

    /// Every review comment of `pull` (pending ones included), oldest first.
    pub async fn comments(&self, pull: IssueId) -> Result<Vec<ReviewComment>, StorageError> {
        let rows: Vec<CommentRow> =
            sqlx::query_as(concat!("SELECT ", comment_columns!(), " FROM pull_review_comments WHERE pull_id = ? ORDER BY created_at, id"))
                .bind(pull.0)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(ReviewComment::try_from).collect()
    }

    pub async fn find_comment(&self, pull: IssueId, id: ReviewCommentId) -> Result<Option<ReviewComment>, StorageError> {
        let row: Option<CommentRow> =
            sqlx::query_as(concat!("SELECT ", comment_columns!(), " FROM pull_review_comments WHERE pull_id = ? AND id = ?"))
                .bind(pull.0)
                .bind(id.0)
                .fetch_optional(&self.pool)
                .await?;
        row.map(ReviewComment::try_from).transpose()
    }

    /// Changes a submitted root's resolution and records its timeline event atomically.
    /// Repeating the same state is a no-op, including under concurrent requests.
    pub async fn set_thread_resolved(
        &self,
        pull: IssueId,
        root: ReviewCommentId,
        actor: AccountId,
        resolved: bool,
        at: Timestamp,
    ) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query(
            "UPDATE pull_review_comments SET resolved_at = ?, resolved_by = ? \
             WHERE pull_id = ? AND id = ? AND in_reply_to IS NULL \
             AND (resolved_at IS NOT NULL) <> ? \
             AND review_id IN (SELECT id FROM pull_reviews WHERE state <> 'pending')",
        )
        .bind(resolved.then_some(at.0))
        .bind(resolved.then_some(actor.0))
        .bind(pull.0)
        .bind(root.0)
        .bind(resolved)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;
        if changed {
            let event = if resolved {
                TimelineEvent::ReviewThreadResolved { comment: root }
            } else {
                TimelineEvent::ReviewThreadReopened { comment: root }
            };
            super::issues::insert_event(&mut tx, pull, Some(actor), &event, at).await?;
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// Comments on submitted reviews per pull request (GitHub's `review_comments`).
    pub async fn submitted_comment_count(&self, pull: IssueId) -> Result<u64, StorageError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pull_review_comments c JOIN pull_reviews r ON r.id = c.review_id \
             WHERE c.pull_id = ? AND r.state <> 'pending'",
        )
        .bind(pull.0)
        .fetch_one(&self.pool)
        .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    /// Marks `ids` outdated.
    pub async fn mark_outdated(&self, ids: &[ReviewCommentId]) -> Result<(), StorageError> {
        if ids.is_empty() {
            return Ok(());
        }
        sqlx::query("UPDATE pull_review_comments SET outdated = 1 WHERE id IN (SELECT value FROM json_each(?))")
            .bind(json_array(ids.iter().map(|id| id.0)))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ------------------------------------------------------------- requests

    /// Requests a review from `user`. `false` if already requested.
    pub async fn request(&self, pull: IssueId, user: AccountId, by: AccountId, at: Timestamp) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "INSERT INTO pull_review_requests (pull_id, user_id, requested_by, created_at) VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
        )
        .bind(pull.0)
        .bind(user.0)
        .bind(by.0)
        .bind(at.0)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Withdraws the request to `user`. `false` if there was none.
    pub async fn remove_request(&self, pull: IssueId, user: AccountId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM pull_review_requests WHERE pull_id = ? AND user_id = ?")
            .bind(pull.0)
            .bind(user.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// The users asked to review `pull`, in request order.
    pub async fn requested(&self, pull: IssueId) -> Result<Vec<AccountId>, StorageError> {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM pull_review_requests WHERE pull_id = ? ORDER BY created_at, user_id")
            .bind(pull.0)
            .fetch_all(&self.pool)
            .await?;
        Ok(ids.into_iter().map(AccountId).collect())
    }
}

async fn insert_comment(
    conn: &mut SqliteConnection,
    review: ReviewId,
    pull: IssueId,
    author: AccountId,
    comment: &NewReviewComment,
    at: Timestamp,
) -> Result<ReviewComment, StorageError> {
    let row: CommentRow = sqlx::query_as(concat!(
        "INSERT INTO pull_review_comments (review_id, pull_id, author_id, path, side, line, commit_sha, diff_hunk, in_reply_to, \
         body, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING ",
        comment_columns!()
    ))
    .bind(review.0)
    .bind(pull.0)
    .bind(author.0)
    .bind(&comment.path)
    .bind(comment.side.as_str())
    .bind(comment.line)
    .bind(&comment.commit_sha)
    .bind(&comment.diff_hunk)
    .bind(comment.in_reply_to.map(|id| id.0))
    .bind(&comment.body)
    .bind(at.0)
    .bind(at.0)
    .fetch_one(&mut *conn)
    .await?;
    row.try_into()
}

async fn clear_request(conn: &mut SqliteConnection, pull: IssueId, user: AccountId) -> Result<(), StorageError> {
    sqlx::query("DELETE FROM pull_review_requests WHERE pull_id = ? AND user_id = ?").bind(pull.0).bind(user.0).execute(&mut *conn).await?;
    Ok(())
}
