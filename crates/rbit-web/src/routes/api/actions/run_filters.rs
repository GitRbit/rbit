//! The filters of `GET …/actions/runs` and `…/workflows/{id}/runs`, in GitHub's names,
//! plus rbit's own `head_sha`: the runs that tested one commit (a full id, or a prefix of
//! at least 7 hex digits). A `branch` also matches the runs of pull requests from it.

use rbit_core::actions::{RunEvent, Status};
use rbit_services::actions::browse::RunQuery;
use serde::Deserialize;

use super::ApiError;

/// The filters of `GET …/actions/runs`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RunsQuery {
    branch: String,
    event: String,
    status: String,
    actor: String,
    head_sha: String,
}

impl RunsQuery {
    /// The service's query (of `workflow` when given), the statuses a `status` stands for,
    /// and whether the filters can match no run at all (an unknown event, or a status rbit
    /// never produces): such a page is empty, not an error.
    pub fn query(&self, workflow: Option<String>) -> Result<(RunQuery, &'static [Status], bool), ApiError> {
        let statuses = match non_empty(&self.status) {
            Some(status) => github_statuses(&status).ok_or_else(|| ApiError::invalid("status", format!("unknown status `{status}`")))?,
            None => &[],
        };
        let sha = non_empty(&self.head_sha);
        if let Some(sha) = &sha
            && !(7..=40).contains(&sha.len())
        {
            return Err(ApiError::invalid("head_sha", "give a commit id or a prefix of at least 7 hex digits"));
        }
        if sha.as_deref().is_some_and(|sha| !sha.bytes().all(|byte| byte.is_ascii_hexdigit())) {
            return Err(ApiError::invalid("head_sha", "a commit id has only hex digits"));
        }
        let event = non_empty(&self.event);
        let matches_nothing = (!self.status.trim().is_empty() && statuses.is_empty())
            || event.as_deref().is_some_and(|event| RunEvent::parse(event).is_none());
        let query = RunQuery {
            workflow,
            branch: non_empty(&self.branch),
            sha: sha.map(|sha| sha.to_ascii_lowercase()),
            event: event.as_deref().and_then(RunEvent::parse),
            actor: non_empty(&self.actor),
            ..RunQuery::default()
        };
        Ok((query, statuses, matches_nothing))
    }
}

/// The run statuses a GitHub `status` filter stands for; `None` for an unknown one. Valid
/// values rbit never produces (`neutral`, `stale`, `timed_out`) stand for none.
fn github_statuses(status: &str) -> Option<&'static [Status]> {
    Some(match status {
        "queued" | "requested" | "pending" => &[Status::Waiting, Status::Blocked],
        "waiting" | "action_required" => &[Status::AwaitingApproval],
        "in_progress" => &[Status::Running],
        "completed" => &[Status::Success, Status::Failure, Status::Cancelled, Status::Skipped],
        "success" => &[Status::Success],
        "failure" => &[Status::Failure],
        "cancelled" => &[Status::Cancelled],
        "skipped" => &[Status::Skipped],
        "neutral" | "stale" | "timed_out" => &[],
        _ => return None,
    })
}

pub fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_statuses_cover_every_run_status_once() {
        for status in Status::ALL {
            let named = ["queued", "waiting", "in_progress", "completed"]
                .into_iter()
                .filter(|name| github_statuses(name).is_some_and(|statuses| statuses.contains(&status)))
                .count();
            assert_eq!(named, 1, "{status}");
        }
        assert_eq!(github_statuses("timed_out"), Some(&[][..]));
        assert_eq!(github_statuses("Completed"), None);
    }

    #[test]
    fn head_sha_takes_an_id_or_a_long_enough_prefix() {
        let with = |sha: &str| RunsQuery { head_sha: sha.to_owned(), ..RunsQuery::default() }.query(None);
        assert_eq!(with("ABCDEF1").unwrap().0.sha.as_deref(), Some("abcdef1"));
        assert_eq!(with(&"a".repeat(40)).unwrap().0.sha.as_deref(), Some(&*"a".repeat(40)));
        assert!(with("").unwrap().0.sha.is_none());
        for refused in ["abc", "zzzzzzz", &"a".repeat(41)] {
            assert_eq!(with(refused).unwrap_err().status, axum::http::StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
        }
    }
}
