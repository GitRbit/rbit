//! The parts of GitHub's and Gitea's REST API objects an import reads.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.
//!
//! Gitea and Forgejo mirror GitHub's shapes closely; where they differ the fields are
//! optional or have defaults, so one set of types reads both. Unknown fields are ignored.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// `GET /repos/{owner}/{repo}`.
#[derive(Debug, Clone, Deserialize)]
pub struct Repo {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub default_branch: Option<String>,
    pub clone_url: String,
}

/// A user as embedded in other objects (and `GET /users/{login}`).
#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub login: String,
    #[serde(default)]
    pub html_url: Option<String>,
    /// Public address, when the user shows one (GitHub: only in `GET /users/{login}`).
    #[serde(default)]
    pub email: Option<String>,
}

/// `GET /repos/{owner}/{repo}/labels`.
#[derive(Debug, Clone, Deserialize)]
pub struct Label {
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// A label as embedded in an issue.
#[derive(Debug, Clone, Deserialize)]
pub struct LabelRef {
    pub name: String,
}

/// `GET /repos/{owner}/{repo}/milestones?state=all`.
#[derive(Debug, Clone, Deserialize)]
pub struct Milestone {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub due_on: Option<String>,
    #[serde(default)]
    pub closed_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// A milestone as embedded in an issue.
#[derive(Debug, Clone, Deserialize)]
pub struct MilestoneRef {
    pub title: String,
}

/// `GET /repos/{owner}/{repo}/issues?state=all`: issues and pull requests alike.
#[derive(Debug, Clone, Deserialize)]
pub struct Issue {
    pub number: u32,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub user: Option<User>,
    pub state: String,
    /// GitHub only: `completed`, `not_planned`, `reopened`.
    #[serde(default)]
    pub state_reason: Option<String>,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub labels: Vec<LabelRef>,
    #[serde(default)]
    pub assignees: Option<Vec<User>>,
    #[serde(default)]
    pub milestone: Option<MilestoneRef>,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub closed_at: Option<String>,
    /// Present (with any content) when the issue is a pull request.
    #[serde(default)]
    pub pull_request: Option<serde_json::Value>,
    #[serde(default)]
    pub html_url: Option<String>,
}

/// One side of a pull request.
#[derive(Debug, Clone, Deserialize)]
pub struct Branch {
    #[serde(rename = "ref")]
    pub name: String,
    #[serde(default)]
    pub sha: String,
    #[serde(default)]
    pub repo: Option<BranchRepo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BranchRepo {
    #[serde(default)]
    pub full_name: String,
}

/// `GET /repos/{owner}/{repo}/pulls?state=all`.
#[derive(Debug, Clone, Deserialize)]
pub struct Pull {
    pub number: u32,
    pub head: Branch,
    pub base: Branch,
    #[serde(default)]
    pub merged_at: Option<String>,
    #[serde(default)]
    pub merge_commit_sha: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub merged_by: Option<User>,
}

/// `GET /repos/{owner}/{repo}/issues/comments`: comments on issues and pull requests.
#[derive(Debug, Clone, Deserialize)]
pub struct Comment {
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub body: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    /// `…/issues/{number}` (GitHub and Gitea).
    #[serde(default)]
    pub issue_url: Option<String>,
    /// `…/pulls/{number}` (Gitea, for comments on pull requests).
    #[serde(default)]
    pub pull_request_url: Option<String>,
}

impl Comment {
    /// The number of the thread the comment belongs to, from its URLs.
    pub fn number(&self) -> Option<u32> {
        [&self.issue_url, &self.pull_request_url]
            .into_iter()
            .flatten()
            .find_map(|url| url.trim_end_matches('/').rsplit('/').next()?.parse().ok())
            .filter(|number| *number > 0)
    }
}

/// `GET /repos/{owner}/{repo}/releases`.
#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub author: Option<User>,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

/// A file attached to a release.
#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub content_type: Option<String>,
    /// GitHub: the API URL that answers with the bytes for `Accept: application/octet-stream`.
    #[serde(default)]
    pub url: Option<String>,
    /// The public download URL (all Gitea downloads).
    #[serde(default)]
    pub browser_download_url: Option<String>,
}

// ------------------------------------------------------------------ Git LFS batch API

/// `POST <clone URL>/info/lfs/objects/batch`.
#[derive(Debug, Clone, Serialize)]
pub struct BatchRequest<'a> {
    pub operation: &'static str,
    pub transfers: [&'static str; 1],
    pub objects: Vec<BatchObjectRequest<'a>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchObjectRequest<'a> {
    pub oid: &'a str,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchResponse {
    #[serde(default)]
    pub objects: Vec<BatchObject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchObject {
    pub oid: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub actions: Option<BatchActions>,
    #[serde(default)]
    pub error: Option<BatchError>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchActions {
    #[serde(default)]
    pub download: Option<BatchAction>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchAction {
    pub href: String,
    #[serde(default)]
    pub header: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchError {
    #[serde(default)]
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_know_their_thread() {
        let comment = |issue: Option<&str>, pull: Option<&str>| Comment {
            user: None,
            body: None,
            created_at: String::new(),
            updated_at: None,
            issue_url: issue.map(str::to_owned),
            pull_request_url: pull.map(str::to_owned),
        };
        assert_eq!(comment(Some("https://api.github.com/repos/a/b/issues/12"), None).number(), Some(12));
        assert_eq!(comment(Some(""), Some("https://gitea.example/a/b/pulls/3")).number(), Some(3));
        assert_eq!(comment(None, None).number(), None);
        assert_eq!(comment(Some("https://x/issues/0"), None).number(), None);
    }

    #[test]
    fn github_and_gitea_shapes_both_parse() {
        let github = r#"{"number": 3, "title": "Crash", "state": "closed", "state_reason": "not_planned", "locked": true,
            "user": {"login": "octocat", "html_url": "https://github.com/octocat"}, "labels": [{"name": "bug", "color": "f00"}],
            "created_at": "2020-01-01T00:00:00Z", "closed_at": "2020-01-02T00:00:00Z", "pull_request": {"url": "x"}}"#;
        let issue: Issue = serde_json::from_str(github).unwrap();
        assert!(issue.pull_request.is_some() && issue.locked && issue.labels[0].name == "bug");
        let gitea = r#"{"number": 4, "title": "Typo", "state": "open", "body": "", "assignees": null, "milestone": null,
            "created_at": "2020-01-01T00:00:00+08:00", "updated_at": "2020-01-01T00:00:00+08:00", "closed_at": null}"#;
        let issue: Issue = serde_json::from_str(gitea).unwrap();
        assert!(issue.pull_request.is_none() && issue.assignees.is_none());
        let pull: Pull = serde_json::from_str(
            r#"{"number": 3, "head": {"ref": "fix", "sha": "abc", "repo": null}, "base": {"ref": "main", "sha": "def"},
                "merged_at": null, "merge_commit_sha": null}"#,
        )
        .unwrap();
        assert_eq!((pull.head.name.as_str(), pull.base.name.as_str()), ("fix", "main"));
    }
}
