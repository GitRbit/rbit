//! Pull request state rules the shared timeline defers to.
//!
//! Milestone 2 workstream P (pull requests) owns this file.

use rbit_core::Issue;

use crate::{ServiceContext, ServiceError};

/// Called by `TimelineService::set_state` before reopening a pull request. Refuses
/// (`ServiceError::Invalid`) when it was merged, its head branch no longer exists, or
/// another open pull request has the same base and head.
pub async fn check_reopen(ctx: &ServiceContext, pull: &Issue) -> Result<(), ServiceError> {
    let store = ctx.database.pulls();
    let Some(row) = store.find(pull.id).await? else { return Ok(()) };
    if row.is_merged() {
        return Err(ServiceError::Invalid("a merged pull request cannot be reopened".to_owned()));
    }
    let head_exists = match row.head_repo_id {
        Some(id) => match ctx.database.admin().find_repository(id).await? {
            Some(head) => {
                let reader = ctx.repositories.reader(head.repo.owner_id, head.repo.id);
                reader.commit_id(format!("refs/heads/{}", row.head_branch)).await.is_ok()
            }
            None => false,
        },
        None => false,
    };
    if !head_exists {
        return Err(ServiceError::Invalid("the head branch no longer exists; push it again to reopen".to_owned()));
    }
    if let Some(number) = store.open_duplicate(&row).await? {
        return Err(ServiceError::Invalid(format!("pull request #{} already proposes these branches", number.get())));
    }
    Ok(())
}
