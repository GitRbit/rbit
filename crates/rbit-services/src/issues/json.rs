//! GitHub-shaped JSON for issues, comments, labels and milestones, shared by webhook
//! payloads and the REST API. P embeds these objects in `pull_request` payloads.
//!
//! Milestone 2 workstream I (issues) owns this file.
//!
//! [`issue`] builds the object from the row alone (`user`, `labels`, `assignees` and
//! `milestone` empty); [`issue_with`] fills them from the author and an
//! [`IssueMetadata`]. Pull requests carry `pull_request: {url, html_url, merged_at}`, as
//! GitHub's issue objects do. No email address appears anywhere.

use rbit_core::{Account, Comment, Issue, IssueKind, StateReason, issue::LabelSnapshot, repository::OwnedRepository};
use rbit_storage::sqlite::issue_meta::{Label, MilestoneProgress};
use serde_json::{Value, json};

use crate::{
    issues::{IssueMetadata, MilestoneRef},
    webhooks::payloads::{account, timestamp},
};

/// Encodes one URL path segment (label names may contain spaces and slashes).
fn segment(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn repo_api(repo: &OwnedRepository, base_url: &str) -> String {
    format!("{base_url}/api/v1/repos/{}", repo.full_name())
}

/// The page of a thread: `/issues/{n}` or `/pull/{n}` by kind.
pub fn html_url(issue: &Issue, repo: &OwnedRepository, base_url: &str) -> String {
    let page = match issue.kind {
        IssueKind::Issue => "issues",
        IssueKind::Pull => "pull",
    };
    format!("{base_url}/{}/{page}/{}", repo.full_name(), issue.number.get())
}

/// An issue (or the issue side of a pull request), as payloads and the API embed it,
/// from the row alone.
pub fn issue(issue: &Issue, repo: &OwnedRepository, base_url: &str) -> Value {
    issue_with(issue, None, &IssueMetadata::default(), repo, base_url)
}

/// [`issue`] with its author (`user`) and metadata (`labels`, `assignee`, `assignees`,
/// `milestone`).
pub fn issue_with(issue: &Issue, author: Option<&Account>, metadata: &IssueMetadata, repo: &OwnedRepository, base_url: &str) -> Value {
    let number = issue.number.get();
    let api = repo_api(repo, base_url);
    let assignees: Vec<Value> = metadata.assignees.iter().map(|a| account(a, base_url)).collect();
    let mut value = json!({
        "id": issue.id.0,
        "number": number,
        "title": issue.title.as_str(),
        "body": issue.body.as_str(),
        "state": issue.state.as_str(),
        "state_reason": issue.state_reason.map(StateReason::as_str),
        "locked": issue.locked,
        "user": author.map(|author| account(author, base_url)),
        "labels": metadata.labels.iter().map(|l| label(l, repo, base_url)).collect::<Vec<_>>(),
        "assignee": assignees.first().cloned(),
        "assignees": assignees,
        "milestone": metadata.milestone.as_ref().map(|m| milestone(m, repo, base_url)),
        "comments": issue.comment_count,
        "created_at": timestamp(issue.created_at),
        "updated_at": timestamp(issue.updated_at),
        "closed_at": issue.closed_at.map(timestamp),
        "html_url": html_url(issue, repo, base_url),
        "url": format!("{api}/issues/{number}"),
        "repository_url": api.clone(),
        "comments_url": format!("{api}/issues/{number}/comments"),
        "labels_url": format!("{api}/issues/{number}/labels"),
    });
    if issue.kind == IssueKind::Pull {
        let merged_at = (issue.state_reason == Some(StateReason::Merged)).then_some(issue.closed_at).flatten();
        value["pull_request"] = json!({
            "url": format!("{api}/pulls/{number}"),
            "html_url": html_url(issue, repo, base_url),
            "merged_at": merged_at.map(timestamp),
        });
    }
    value
}

/// A label (`color` without the `#`, as GitHub returns it).
pub fn label(label: &LabelSnapshot, repo: &OwnedRepository, base_url: &str) -> Value {
    json!({
        "id": label.id.0,
        "name": label.name,
        "color": label.color.trim_start_matches('#'),
        "url": format!("{}/labels/{}", repo_api(repo, base_url), segment(&label.name)),
    })
}

/// A full label with its description, for the labels API.
pub fn label_detail(stored: &Label, repo: &OwnedRepository, base_url: &str) -> Value {
    let mut value = label(&stored.snapshot(), repo, base_url);
    value["description"] = if stored.description.is_empty() { Value::Null } else { Value::String(stored.description.clone()) };
    value["default"] = Value::Bool(false);
    value
}

/// A milestone, as embedded in issues.
pub fn milestone(milestone: &MilestoneRef, repo: &OwnedRepository, base_url: &str) -> Value {
    json!({
        "id": milestone.id.0,
        "number": milestone.id.0,
        "title": milestone.title,
        "url": format!("{}/milestones/{}", repo_api(repo, base_url), milestone.id.0),
        "html_url": format!("{base_url}/{}/milestones/{}", repo.full_name(), milestone.id.0),
    })
}

/// A full milestone with its progress, for the milestones API. `due_on` is midnight UTC
/// of the due day, as GitHub reports it.
pub fn milestone_detail(progress: &MilestoneProgress, repo: &OwnedRepository, base_url: &str) -> Value {
    let stored = &progress.milestone;
    let mut value = milestone(&MilestoneRef { id: stored.id, title: stored.title.clone() }, repo, base_url);
    let object = value.as_object_mut().expect("milestone() builds an object");
    object.insert("description".into(), if stored.description.is_empty() { Value::Null } else { stored.description.clone().into() });
    object.insert("state".into(), stored.state.as_str().into());
    object.insert("open_issues".into(), progress.open_issues.into());
    object.insert("closed_issues".into(), progress.closed_issues.into());
    object.insert("due_on".into(), stored.due_on.as_ref().map_or(Value::Null, |day| format!("{day}T00:00:00Z").into()));
    object.insert("created_at".into(), timestamp(stored.created_at));
    object.insert("updated_at".into(), timestamp(stored.updated_at));
    object.insert("closed_at".into(), stored.closed_at.map_or(Value::Null, timestamp));
    value
}

/// A comment on `issue`.
pub fn comment(comment: &Comment, author: Option<&Account>, issue: &Issue, repo: &OwnedRepository, base_url: &str) -> Value {
    let api = repo_api(repo, base_url);
    json!({
        "id": comment.id.0,
        "body": comment.body.as_str(),
        "user": author.map(|author| account(author, base_url)),
        "created_at": timestamp(comment.created_at),
        "updated_at": timestamp(comment.updated_at),
        "html_url": format!("{}#issuecomment-{}", html_url(issue, repo, base_url), comment.id.0),
        "url": format!("{api}/issues/comments/{}", comment.id.0),
        "issue_url": format!("{api}/issues/{}", issue.number.get()),
    })
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        AccountId, AccountKind, CommentId, IssueId, IssueNumber, IssueState, IssueText, IssueTitle, LabelId, MilestoneId, RepoId,
        Repository, Timestamp, Visibility,
    };

    use super::*;

    const BASE: &str = "https://git.example.com";

    fn ada() -> Account {
        Account {
            id: AccountId(1),
            kind: AccountKind::User,
            name: "ada".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        }
    }

    fn repo() -> OwnedRepository {
        OwnedRepository {
            owner: ada(),
            repo: Repository {
                id: RepoId(4),
                owner_id: AccountId(1),
                name: "demo".parse().unwrap(),
                description: None,
                visibility: Visibility::Public,
                default_branch: "main".to_owned(),
                created_at: Timestamp(0),
                updated_at: Timestamp(0),
                pushed_at: None,
                archived_at: None,
            },
        }
    }

    fn thread(kind: IssueKind, merged: bool) -> Issue {
        Issue {
            id: IssueId(9),
            repo_id: RepoId(4),
            number: IssueNumber::new(3).unwrap(),
            kind,
            title: IssueTitle::parse("Crash").unwrap(),
            body: IssueText::body("Boom").unwrap(),
            author: Some(AccountId(1)),
            state: if merged { IssueState::Closed } else { IssueState::Open },
            state_reason: merged.then_some(StateReason::Merged),
            closed_at: merged.then_some(Timestamp(1_760_000_000)),
            closed_by: None,
            locked: false,
            comment_count: 2,
            created_at: Timestamp(1_760_000_000),
            updated_at: Timestamp(1_760_000_000),
        }
    }

    #[test]
    fn issues_carry_every_field() {
        let metadata = IssueMetadata {
            labels: vec![LabelSnapshot { id: LabelId(2), name: "good first issue".into(), color: "#7057ff".into() }],
            assignees: vec![ada()],
            milestone: Some(MilestoneRef { id: MilestoneId(5), title: "v1".into() }),
        };
        let value = issue_with(&thread(IssueKind::Issue, false), Some(&ada()), &metadata, &repo(), BASE);
        for key in [
            "id",
            "number",
            "title",
            "body",
            "state",
            "state_reason",
            "locked",
            "user",
            "labels",
            "assignee",
            "assignees",
            "milestone",
            "comments",
            "created_at",
            "updated_at",
            "closed_at",
            "html_url",
            "url",
            "repository_url",
            "comments_url",
        ] {
            assert!(value.get(key).is_some(), "{key}");
        }
        assert_eq!(value["html_url"], "https://git.example.com/ada/demo/issues/3");
        assert_eq!(value["url"], "https://git.example.com/api/v1/repos/ada/demo/issues/3");
        assert_eq!(value["user"]["login"], "ada");
        assert_eq!(value["assignee"]["login"], "ada");
        assert_eq!(value["labels"][0]["color"], "7057ff");
        assert_eq!(value["labels"][0]["url"], "https://git.example.com/api/v1/repos/ada/demo/labels/good%20first%20issue");
        assert_eq!(value["milestone"]["title"], "v1");
        assert_eq!(value["created_at"], "2025-10-09T08:53:20Z");
        assert!(value.get("pull_request").is_none());
        assert!(!value.to_string().contains("@example.com"), "no email addresses");

        let bare = issue(&thread(IssueKind::Issue, false), &repo(), BASE);
        assert_eq!((bare["user"].clone(), bare["assignees"].clone()), (Value::Null, json!([])));
    }

    #[test]
    fn pull_requests_link_their_pull_request() {
        let value = issue(&thread(IssueKind::Pull, true), &repo(), BASE);
        assert_eq!(value["html_url"], "https://git.example.com/ada/demo/pull/3");
        assert_eq!(value["pull_request"]["url"], "https://git.example.com/api/v1/repos/ada/demo/pulls/3");
        assert_eq!(value["pull_request"]["merged_at"], "2025-10-09T08:53:20Z");
        let open = issue(&thread(IssueKind::Pull, false), &repo(), BASE);
        assert_eq!(open["pull_request"]["merged_at"], Value::Null);
    }

    #[test]
    fn comments_link_their_anchor() {
        let body = IssueText::comment("Same").unwrap();
        let stored = Comment {
            id: CommentId(7),
            issue_id: IssueId(9),
            author: None,
            body,
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
            edited_at: None,
        };
        let value = comment(&stored, None, &thread(IssueKind::Issue, false), &repo(), BASE);
        assert_eq!(value["html_url"], "https://git.example.com/ada/demo/issues/3#issuecomment-7");
        assert_eq!(value["url"], "https://git.example.com/api/v1/repos/ada/demo/issues/comments/7");
        assert_eq!(value["user"], Value::Null, "a ghost");
    }
}
