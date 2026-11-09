//! The endpoints a workflow task's token may call (`docs/design/actions.md` §11.3) and the
//! permission each needs of the task's grant. [`super::auth::Caller`] refuses every other
//! endpoint to a task with 403.

use rbit_services::actions::tokens::TaskGrant;

use super::ApiMethod;

// Actions: S.
/// What an endpoint on [`TASK_ROUTES`] needs of a task's token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskNeed {
    /// Reading: its own repository at its grant, other public ones.
    Read,
    /// `contents: write`, on its own repository.
    ContentsWrite,
    /// `issues: write` or `pull-requests: write`, on its own repository.
    Comment,
    /// `issues: read` or `pull-requests: read`, on its own repository.
    IssuesRead,
}

impl TaskNeed {
    pub(super) fn allows(self, grant: &TaskGrant) -> bool {
        match self {
            Self::Read => true,
            Self::ContentsWrite => grant.contents().can_write(),
            Self::Comment => grant.can_comment(),
            Self::IssuesRead => grant.can_read_issues(),
        }
    }

    /// Whether the endpoint changes the repository, which a task may only do to its own.
    pub(super) fn writes(self) -> bool {
        matches!(self, Self::ContentsWrite | Self::Comment)
    }
}

/// The only endpoints a workflow task's token is accepted on (§11.3), as [`super::table`]
/// spells them. Everything else answers 403 to a task.
pub const TASK_ROUTES: &[(ApiMethod, &str, TaskNeed)] = &[
    (ApiMethod::Get, "/repos/{owner}/{repo}", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/branches", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/branches/{*branch}", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/tags", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/commits", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/commits/{*reference}", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases/latest", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases/tags/{*tag}", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases/{release_id}", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases/{release_id}/assets", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/releases/assets/{asset_id}", TaskNeed::Read),
    // Actions: workflow_run. What GitHub's default `actions: read` lets a run do: fetch the
    // artifacts of the run that started it, or of any other.
    (ApiMethod::Get, "/repos/{owner}/{repo}/actions/runs/{run_id}/artifacts", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/actions/artifacts", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/actions/artifacts/{artifact_id}", TaskNeed::Read),
    (ApiMethod::Get, "/repos/{owner}/{repo}/actions/artifacts/{artifact_id}/{archive_format}", TaskNeed::Read),
    // A status can satisfy a required check, so a read-only (untrusted) task may not post one.
    (ApiMethod::Post, "/repos/{owner}/{repo}/statuses/{sha}", TaskNeed::ContentsWrite),
    (ApiMethod::Post, "/repos/{owner}/{repo}/releases", TaskNeed::ContentsWrite),
    (ApiMethod::Post, "/repos/{owner}/{repo}/releases/{release_id}/assets", TaskNeed::ContentsWrite),
    // Actions: releases from workflows. What publishing actions do besides creating: finish a
    // draft, replace a file, start over.
    (ApiMethod::Patch, "/repos/{owner}/{repo}/releases/{release_id}", TaskNeed::ContentsWrite),
    (ApiMethod::Delete, "/repos/{owner}/{repo}/releases/{release_id}", TaskNeed::ContentsWrite),
    (ApiMethod::Delete, "/repos/{owner}/{repo}/releases/assets/{asset_id}", TaskNeed::ContentsWrite),
    (ApiMethod::Post, "/repos/{owner}/{repo}/issues/{number}/comments", TaskNeed::Comment),
    // Actions: task tokens. What comment bots do: find the thread and their own comment, then
    // edit or delete it (only comments rbit Actions wrote, `TimelineService::edit_comment_for_task`).
    (ApiMethod::Get, "/repos/{owner}/{repo}/issues", TaskNeed::IssuesRead),
    (ApiMethod::Get, "/repos/{owner}/{repo}/issues/{number}", TaskNeed::IssuesRead),
    (ApiMethod::Get, "/repos/{owner}/{repo}/issues/{number}/comments", TaskNeed::IssuesRead),
    (ApiMethod::Get, "/repos/{owner}/{repo}/issues/comments/{id}", TaskNeed::IssuesRead),
    (ApiMethod::Patch, "/repos/{owner}/{repo}/issues/comments/{id}", TaskNeed::Comment),
    (ApiMethod::Delete, "/repos/{owner}/{repo}/issues/comments/{id}", TaskNeed::Comment),
];

/// What `method path` (a route of [`super::table`]) needs of a task; `None` when tasks may
/// not call it at all.
pub fn task_route(method: ApiMethod, path: &str) -> Option<TaskNeed> {
    TASK_ROUTES.iter().find(|(allowed, template, _)| *allowed == method && *template == path).map(|(_, _, need)| *need)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Actions: S.
    /// §11.3: every route of the table either is on the task allowlist or refuses tasks, and
    /// the allowlist names only routes that exist; the dangerous ones are not on it.
    #[test]
    fn the_task_allowlist_names_only_routes_of_the_table() {
        let mut config = rbit_core::Config::default();
        config.actions.enabled = true;
        let table = super::super::table(&config);
        for (method, path, _) in TASK_ROUTES {
            assert!(table.iter().any(|route| route.method == *method && route.path == *path), "{method:?} {path} is not a route");
        }
        let accepting: Vec<_> = table.iter().filter(|route| task_route(route.method, &route.path).is_some()).collect();
        assert_eq!(accepting.len(), TASK_ROUTES.len(), "{accepting:?}");
        for refused in ["/user", "/user/repos", "/repos/{owner}/{repo}/hooks", "/repos/{owner}/{repo}/forks", "/repos/{owner}/{repo}/pulls"]
        {
            for method in [ApiMethod::Get, ApiMethod::Post, ApiMethod::Patch, ApiMethod::Delete] {
                assert_eq!(task_route(method, refused), None, "{method:?} {refused}");
            }
        }
        assert_eq!(task_route(ApiMethod::Patch, "/repos/{owner}/{repo}"), None, "a task never changes repository settings");
        assert_eq!(task_route(ApiMethod::Post, "/repos/{owner}/{repo}/issues"), None, "nor opens issues");
    }
}
