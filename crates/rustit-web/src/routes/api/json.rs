//! JSON in and out: a body extractor with JSON errors, and the representations the API
//! returns that webhook payloads do not share (those live in
//! [`rustit_services::webhooks::payloads`]).

use axum::{
    Json,
    extract::{FromRequest, Request},
};
use rustit_core::{
    Account, OrgMember, Organization, RepoAccess, User,
    repository::OwnedRepository,
    webhook::{Delivery, Webhook},
};
use rustit_git::{CommitSummary, RefDetail};
use rustit_services::{profiles::PublicProfile, webhooks::payloads};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::error::ApiError;

/// A JSON request body; malformed or oversized bodies become JSON errors.
#[derive(Debug)]
pub struct ApiJson<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(request, state).await?;
        Ok(Self(value))
    }
}

/// For `#[serde(default, deserialize_with = "present")]` on an `Option<Value>`: a missing
/// field stays `None`, while an explicit `null` becomes `Some(Value::Null)` (plain
/// `Option` would conflate the two, and `null` means "clear" in PATCH bodies).
pub fn present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    <Value as serde::Deserialize>::deserialize(deserializer).map(Some)
}

/// `GET /user`: the caller, including their email address.
pub fn me(user: &User, base_url: &str) -> Value {
    let mut value = payloads::account(&user.account, base_url);
    value["email"] = Value::String(user.email.as_str().to_owned());
    value["site_admin"] = Value::Bool(user.is_admin);
    value["bio"] = json!(user.account.bio);
    value["created_at"] = payloads::timestamp(user.account.created_at);
    value
}

/// `GET /users/{username}`: public profile fields.
pub fn profile(account: &Account, profile: &PublicProfile, base_url: &str) -> Value {
    let mut value = payloads::account(account, base_url);
    value["bio"] = json!(account.bio);
    value["blog"] = json!(profile.website);
    value["location"] = json!(profile.location);
    value["created_at"] = payloads::timestamp(account.created_at);
    value
}

/// `GET /orgs/{org}`.
pub fn organization(org: &Organization, base_url: &str) -> Value {
    let mut value = payloads::account(&org.account, base_url);
    value["description"] = json!(org.description);
    value["blog"] = json!(org.website);
    value["created_at"] = payloads::timestamp(org.account.created_at);
    value
}

/// One entry of `GET /orgs/{org}/members`.
pub fn member(member: &OrgMember, base_url: &str) -> Value {
    let mut value = payloads::account(&member.user, base_url);
    value["role"] = Value::String(member.role.as_str().to_owned());
    value
}

/// A repository; `access` adds GitHub's `permissions` object when known.
pub fn repository(repo: &OwnedRepository, access: Option<RepoAccess>, base_url: &str) -> Value {
    let mut value = payloads::repository(repo, base_url, false);
    if let Some(access) = access {
        value["permissions"] = json!({ "admin": access.can_admin(), "push": access.can_write(), "pull": access.can_read() });
    }
    value
}

/// A branch or tag with the commit it points at.
pub fn git_ref(detail: &RefDetail, repo_api: &str, branch: bool) -> Value {
    let mut value = json!({
        "name": detail.name,
        "commit": { "sha": detail.commit.id, "url": format!("{repo_api}/commits/{}", detail.commit.id) },
    });
    if branch {
        value["protected"] = Value::Bool(false);
    }
    value
}

/// A commit, in GitHub's `GET /repos/{owner}/{repo}/commits` shape.
pub fn commit(commit: &CommitSummary, repo_api: &str, html_url: &str) -> Value {
    let message = match &commit.body {
        Some(body) if !body.is_empty() => format!("{}\n\n{body}", commit.summary),
        _ => commit.summary.clone(),
    };
    let author = json!({ "name": commit.author_name, "email": commit.author_email, "date": payloads::timestamp(commit.authored_at) });
    json!({
        "sha": commit.id,
        "url": format!("{repo_api}/commits/{}", commit.id),
        "html_url": format!("{html_url}/commit/{}", commit.id),
        "commit": { "message": message, "author": author, "committer": author },
        "parents": commit.parent_ids.iter().map(|id| json!({ "sha": id, "url": format!("{repo_api}/commits/{id}") })).collect::<Vec<_>>(),
    })
}

/// A hook; `api_url` is its own URL.
pub fn hook(hook: &Webhook, api_url: &str) -> Value {
    let mut value = payloads::hook(hook, api_url);
    value["deliveries_url"] = Value::String(format!("{api_url}/deliveries"));
    value["ping_url"] = Value::String(format!("{api_url}/pings"));
    value
}

/// A delivery in the log list.
pub fn delivery(delivery: &Delivery, hook_api: &str) -> Value {
    json!({
        "id": delivery.id.0,
        "guid": delivery.guid,
        "event": delivery.event.as_str(),
        "action": delivery.action,
        "status": delivery.status.as_str(),
        "status_code": delivery.response_status,
        "attempts": delivery.attempts,
        "delivered_at": delivery.last_attempt_at.map(payloads::timestamp),
        "next_attempt_at": delivery.next_attempt_at.filter(|_| !delivery.status.is_final()).map(payloads::timestamp),
        "duration_ms": delivery.duration_ms,
        "created_at": payloads::timestamp(delivery.created_at),
        "url": format!("{hook_api}/deliveries/{}", delivery.id.0),
    })
}

/// A delivery with its request and response.
pub fn delivery_detail(record: &Delivery, hook_api: &str) -> Value {
    let headers = |pairs: &[(String, String)]| {
        pairs.iter().map(|(name, value)| (name.clone(), Value::String(value.clone()))).collect::<serde_json::Map<_, _>>()
    };
    let payload = serde_json::from_str::<Value>(&record.payload).unwrap_or_else(|_| Value::String(record.payload.clone()));
    let mut value = delivery(record, hook_api);
    value["request"] = json!({ "headers": headers(&record.request_headers), "payload": payload });
    value["response"] = json!({ "headers": headers(&record.response_headers), "payload": record.response_body });
    value["error"] = json!(record.error);
    value
}

#[cfg(test)]
mod tests {
    use rustit_core::{AccountId, AccountKind, Timestamp};

    use super::*;

    #[test]
    fn commits_and_refs_have_github_shapes() {
        let summary = CommitSummary {
            id: "c".repeat(40),
            short_id: "c".repeat(7),
            summary: "Add things".to_owned(),
            body: Some("Details.".to_owned()),
            author_name: "Ada".to_owned(),
            author_email: "ada@example.com".to_owned(),
            authored_at: Timestamp(1_792_022_400),
            parent_ids: vec!["b".repeat(40)],
        };
        let value = commit(&summary, "https://h/api/v1/repos/ada/demo", "https://h/ada/demo");
        assert_eq!(value["sha"], "c".repeat(40));
        assert_eq!(value["html_url"], format!("https://h/ada/demo/commit/{}", "c".repeat(40)));
        assert_eq!(value["commit"]["message"], "Add things\n\nDetails.");
        assert_eq!(value["commit"]["author"]["date"], "2026-10-15T00:00:00Z");
        assert_eq!(value["parents"][0]["sha"], "b".repeat(40));

        let detail = RefDetail { name: "feature/x".to_owned(), kind: rustit_git::RefKind::Branch, commit: summary };
        let branch = git_ref(&detail, "https://h/api/v1/repos/ada/demo", true);
        assert_eq!((branch["name"].as_str(), branch["protected"].as_bool()), (Some("feature/x"), Some(false)));
        assert!(git_ref(&detail, "x", false).get("protected").is_none(), "tags are not protected");
    }

    #[test]
    fn members_carry_their_role() {
        let user = Account {
            id: AccountId(2),
            kind: AccountKind::User,
            name: "bob".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        };
        let value = member(&OrgMember { user, role: rustit_core::OrgRole::Owner }, "https://h");
        assert_eq!((value["login"].as_str(), value["role"].as_str()), (Some("bob"), Some("owner")));
    }
}
