//! GitHub-shaped JSON for pull requests and reviews, shared by webhook payloads and the
//! REST API. Embeds I's `issues::json` objects for labels, milestones and users.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! No email address appears in any object (accounts are `payloads::account`, which has
//! none).

use rbit_core::{
    Account, Issue, IssueState, RepoId,
    issue::ReviewState,
    pull::{Mergeability, PullRequest, Review, ReviewComment, ReviewSide},
    repository::OwnedRepository,
};
use rbit_git::DiffLimits;
use serde_json::{Value, json};

use crate::{
    RepoContext, ServiceContext, ServiceError,
    issues::{IssueMetadata, MilestoneRef},
    webhooks::payloads::{account, repository, timestamp},
};

/// Size figures of a pull request (`commits`, `additions`, `deletions`, `changed_files`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PullStats {
    pub commits: usize,
    pub additions: usize,
    pub deletions: usize,
    pub changed_files: usize,
}

/// Everything [`pull_request`] shows.
#[derive(Debug, Clone, Copy)]
pub struct PullJson<'a> {
    pub issue: &'a Issue,
    pub pull: &'a PullRequest,
    pub repo: &'a OwnedRepository,
    pub repo_is_fork: bool,
    pub head_repo: Option<&'a OwnedRepository>,
    pub head_is_fork: bool,
    /// The base branch's tip (or the merged base).
    pub base_sha: Option<&'a str>,
    pub author: Option<&'a Account>,
    pub merged_by: Option<&'a Account>,
    pub requested: &'a [Account],
    pub metadata: Option<&'a IssueMetadata>,
    pub stats: PullStats,
    pub review_comments: u64,
    /// Some protection requirement is unmet (`mergeable_state: blocked`).
    pub blocked: bool,
}

fn ghost(base_url: &str) -> Value {
    json!({ "id": null, "login": "ghost", "name": null, "type": "User", "html_url": format!("{base_url}/ghost"), "url": null })
}

fn user(account_value: Option<&Account>, base_url: &str) -> Value {
    account_value.map_or_else(|| ghost(base_url), |a| account(a, base_url))
}

/// `pull_request` as the API returns it and payloads embed it.
pub fn pull_request(data: &PullJson<'_>, base_url: &str) -> Value {
    let (issue, pull, repo) = (data.issue, data.pull, data.repo);
    let full_name = repo.full_name();
    let number = issue.number.get();
    let api = format!("{base_url}/api/v1/repos/{full_name}");
    let head_label = match data.head_repo {
        Some(head) => format!("{}:{}", head.owner.name, pull.head_branch),
        None => format!("unknown:{}", pull.head_branch),
    };
    let labels: Vec<Value> =
        data.metadata.map(|m| m.labels.iter().map(|label| crate::issues::json::label(label, repo, base_url)).collect()).unwrap_or_default();
    let assignees: Vec<Value> = data.metadata.map(|m| m.assignees.iter().map(|a| account(a, base_url)).collect()).unwrap_or_default();
    let milestone =
        data.metadata.and_then(|m| m.milestone.as_ref()).map_or(Value::Null, |m| crate::issues::json::milestone(m, repo, base_url));
    let state = match issue.state {
        IssueState::Open => "open",
        IssueState::Closed => "closed",
    };
    let mergeable = match (pull.is_merged(), pull.mergeable) {
        (false, Mergeability::Clean) => Value::Bool(true),
        (false, Mergeability::Conflict) => Value::Bool(false),
        (true, _) | (false, Mergeability::Unknown | Mergeability::Error) => Value::Null,
    };
    let mergeable_state = if issue.state == IssueState::Open { pull.mergeable_state(data.blocked) } else { "unknown" };
    json!({
        "id": issue.id.0,
        "number": number,
        "state": state,
        "locked": issue.locked,
        "title": issue.title.as_str(),
        "body": issue.body.as_str(),
        "user": user(data.author, base_url),
        "labels": labels,
        "milestone": milestone,
        "assignees": assignees,
        "requested_reviewers": data.requested.iter().map(|a| account(a, base_url)).collect::<Vec<_>>(),
        "draft": pull.is_draft,
        "head": {
            "label": head_label,
            "ref": pull.head_branch,
            "sha": pull.head_sha,
            "repo": data.head_repo.map_or(Value::Null, |head| repository(head, base_url, data.head_is_fork)),
        },
        "base": {
            "label": format!("{}:{}", repo.owner.name, pull.base_branch),
            "ref": pull.base_branch,
            "sha": data.base_sha,
            "repo": repository(repo, base_url, data.repo_is_fork),
        },
        "merged": pull.is_merged(),
        "mergeable": mergeable,
        "mergeable_state": mergeable_state,
        "merged_at": pull.merged_at.map(timestamp),
        "merged_by": if pull.is_merged() { user(data.merged_by, base_url) } else { Value::Null },
        "merge_commit_sha": pull.merge_commit_sha,
        "comments": issue.comment_count,
        "review_comments": data.review_comments,
        "commits": data.stats.commits,
        "additions": data.stats.additions,
        "deletions": data.stats.deletions,
        "changed_files": data.stats.changed_files,
        "html_url": format!("{base_url}/{full_name}/pull/{number}"),
        "diff_url": format!("{base_url}/{full_name}/pull/{number}/files"),
        "url": format!("{api}/pulls/{number}"),
        "issue_url": format!("{api}/issues/{number}"),
        "commits_url": format!("{api}/pulls/{number}/commits"),
        "review_comments_url": format!("{api}/pulls/{number}/comments"),
        "created_at": timestamp(issue.created_at),
        "updated_at": timestamp(issue.updated_at.max(pull.updated_at)),
        "closed_at": issue.closed_at.map(timestamp),
    })
}

/// A review (`state` in GitHub's upper case: `APPROVED`, `CHANGES_REQUESTED`, …).
pub fn review(review: &Review, reviewer: Option<&Account>, repo: &OwnedRepository, number: u32, base_url: &str) -> Value {
    let full_name = repo.full_name();
    json!({
        "id": review.id.0,
        "user": user(reviewer, base_url),
        "body": review.body,
        "state": review_state(review.state),
        "commit_id": review.commit_sha,
        "submitted_at": review.submitted_at.map(timestamp),
        "html_url": format!("{base_url}/{full_name}/pull/{number}#pullrequestreview-{}", review.id.0),
        "pull_request_url": format!("{base_url}/api/v1/repos/{full_name}/pulls/{number}"),
    })
}

/// GitHub's spelling of a review state.
pub const fn review_state(state: ReviewState) -> &'static str {
    match state {
        ReviewState::Pending => "PENDING",
        ReviewState::Commented => "COMMENTED",
        ReviewState::Approved => "APPROVED",
        ReviewState::ChangesRequested => "CHANGES_REQUESTED",
        ReviewState::Dismissed => "DISMISSED",
    }
}

/// A review comment on a diff line.
pub fn review_comment(comment: &ReviewComment, author: Option<&Account>, repo: &OwnedRepository, number: u32, base_url: &str) -> Value {
    let full_name = repo.full_name();
    json!({
        "id": comment.id.0,
        "pull_request_review_id": comment.review_id.0,
        "user": user(author, base_url),
        "body": comment.body,
        "path": comment.path,
        "side": match comment.side { ReviewSide::Left => "LEFT", ReviewSide::Right => "RIGHT" },
        "line": comment.line,
        "commit_id": comment.commit_sha,
        "diff_hunk": comment.diff_hunk,
        "in_reply_to_id": comment.in_reply_to.map(|id| id.0),
        "outdated": comment.outdated,
        "created_at": timestamp(comment.created_at),
        "updated_at": timestamp(comment.updated_at),
        "html_url": format!("{base_url}/{full_name}/pull/{number}/files#r{}", comment.id.0),
        "pull_request_url": format!("{base_url}/api/v1/repos/{full_name}/pulls/{number}"),
    })
}

/// Loads everything [`pull_request`] needs for one pull request and builds it.
pub async fn load(ctx: &ServiceContext, issue: &Issue, pull: &PullRequest, base_url: &str) -> Result<Value, ServiceError> {
    let database = &ctx.database;
    let repo = database.admin().find_repository(pull.repo_id).await?.ok_or(ServiceError::NotFound)?;
    let head_repo = match pull.head_repo_id {
        Some(id) if id == pull.repo_id => Some(repo.clone()),
        Some(id) => database.admin().find_repository(id).await?,
        None => None,
    };
    let mut ids: Vec<RepoId> = vec![repo.repo.id];
    ids.extend(head_repo.as_ref().map(|head| head.repo.id));
    let forks = database.forks().forks_among(&ids).await?;
    let mut people: Vec<_> = issue.author.into_iter().chain(pull.merged_by).collect();
    let requested_ids = database.reviews().requested(issue.id).await?;
    people.extend(requested_ids.iter().copied());
    let accounts = database.accounts().find_accounts(&people).await?;
    let find = |id| accounts.iter().find(|account| account.id == id);
    let requested: Vec<Account> = requested_ids.iter().filter_map(|id| find(*id).cloned()).collect();
    let metadata = metadata(ctx, issue.id).await?;
    let review_comments = database.reviews().submitted_comment_count(issue.id).await?;

    let system = RepoContext::for_system(repo.clone(), &ctx.repositories);
    let base_sha = match &pull.merged_base_sha {
        Some(sha) => Some(sha.clone()),
        None => system.reader.commit_id(format!("refs/heads/{}", pull.base_branch)).await.ok(),
    };
    let head = pull.merged_head_sha.clone().unwrap_or_else(|| pull.head_sha.clone());
    let stats = match &base_sha {
        Some(base) => match system.reader.compare(base.clone(), head, DiffLimits::default(), 0).await {
            Ok(comparison) => PullStats {
                commits: comparison.ahead,
                additions: comparison.additions(),
                deletions: comparison.deletions(),
                changed_files: comparison.files.len(),
            },
            Err(error) => {
                tracing::debug!(%error, pull = issue.id.0, "cannot compute pull request statistics");
                PullStats::default()
            }
        },
        None => PullStats::default(),
    };
    let data = PullJson {
        issue,
        pull,
        repo: &repo,
        repo_is_fork: forks.contains(&repo.repo.id),
        head_repo: head_repo.as_ref(),
        head_is_fork: head_repo.as_ref().is_some_and(|head| forks.contains(&head.repo.id)),
        base_sha: base_sha.as_deref(),
        author: issue.author.and_then(find),
        merged_by: pull.merged_by.and_then(find),
        requested: &requested,
        metadata: metadata.as_ref(),
        stats,
        review_comments,
        blocked: false,
    };
    Ok(pull_request(&data, base_url))
}

/// Labels, assignees and milestone of one thread, read like `IssueService::metadata` does
/// (this module only has the service context, as the webhook hook signature fixes).
async fn metadata(ctx: &ServiceContext, issue: rbit_core::IssueId) -> Result<Option<IssueMetadata>, ServiceError> {
    let store = ctx.database.issue_meta();
    let ids = [issue];
    let (labels, assignees, milestones) = tokio::try_join!(store.labels_of(&ids), store.assignees_of(&ids), store.milestones_of(&ids))?;
    let mut metadata = IssueMetadata::default();
    metadata.labels.extend(labels.into_iter().map(|(_, label)| label));
    metadata.assignees.extend(assignees.into_iter().map(|(_, account)| account));
    metadata.milestone = milestones.into_iter().next().map(|milestone| MilestoneRef { id: milestone.id, title: milestone.title });
    Ok(Some(metadata))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_states_are_upper_case() {
        assert_eq!(review_state(ReviewState::ChangesRequested), "CHANGES_REQUESTED");
        assert_eq!(review_state(ReviewState::Approved), "APPROVED");
        assert_eq!(ghost("https://x")["login"], "ghost");
    }
}
