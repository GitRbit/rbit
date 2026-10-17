//! JSON shapes shared by webhook payloads and the REST API.
//!
//! They follow GitHub's field names where GitHub has an equivalent, so receivers and
//! clients written for GitHub keep working: `snake_case` fields, numeric `id`s, RFC 3339
//! UTC timestamps, absolute `html_url`s built from `server.public_url`, and explicit
//! `null` rather than missing fields.

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use rustit_core::{
    Account, AccountKind, Timestamp,
    release::{Release, ReleaseAsset},
    repository::OwnedRepository,
    webhook::{Webhook, WebhookEvent, WebhookOwner},
};
use rustit_git::{CommitSummary, RefUpdate};
use serde_json::{Value, json};

/// Characters escaped inside one URL path segment (the web crate's `urls` uses the same set).
const SEGMENT: &AsciiSet =
    &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'%').add(b'<').add(b'>').add(b'?').add(b'`').add(b'{').add(b'}').add(b'/');

/// Encodes a slash-separated path (a tag name may contain `/`), keeping the slashes.
fn encode_path(path: &str) -> String {
    path.split('/').map(|segment| utf8_percent_encode(segment, SEGMENT).to_string()).collect::<Vec<_>>().join("/")
}

/// The all-zero object id GitHub sends for a ref's missing side.
pub const NULL_SHA: &str = "0000000000000000000000000000000000000000";

/// Commits listed in a `push` payload; `total_commits` says how many there were.
pub const MAX_PUSH_COMMITS: usize = 20;

/// RFC 3339, UTC (`2026-10-15T12:00:00Z`).
pub fn timestamp(at: Timestamp) -> Value {
    Value::String(at.iso8601())
}

fn optional_timestamp(at: Option<Timestamp>) -> Value {
    at.map_or(Value::Null, timestamp)
}

/// `"User"` or `"Organization"`, as GitHub spells account types.
pub const fn account_type(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::User => "User",
        AccountKind::Organization => "Organization",
    }
}

/// A user or organisation, as it appears inside other objects (`owner`, `sender`).
pub fn account(account: &Account, base_url: &str) -> Value {
    let api = match account.kind {
        AccountKind::User => format!("{base_url}/api/v1/users/{}", account.name),
        AccountKind::Organization => format!("{base_url}/api/v1/orgs/{}", account.name),
    };
    json!({
        "id": account.id.0,
        "login": account.name.as_str(),
        "name": account.display_name.as_deref().filter(|name| !name.is_empty()),
        "type": account_type(account.kind),
        "html_url": format!("{base_url}/{}", account.name),
        "url": api,
    })
}

/// A repository, as the API returns it and payloads embed it.
pub fn repository(repo: &OwnedRepository, base_url: &str, fork: bool) -> Value {
    let full_name = repo.full_name();
    let html_url = format!("{base_url}/{full_name}");
    json!({
        "id": repo.repo.id.0,
        "name": repo.repo.name.as_str(),
        "full_name": full_name,
        "owner": account(&repo.owner, base_url),
        "private": !repo.repo.visibility.is_public(),
        "visibility": repo.repo.visibility.as_str(),
        "description": repo.repo.description,
        "fork": fork,
        "default_branch": repo.repo.default_branch,
        "html_url": html_url,
        "clone_url": format!("{html_url}.git"),
        "url": format!("{base_url}/api/v1/repos/{full_name}"),
        "created_at": timestamp(repo.repo.created_at),
        "updated_at": timestamp(repo.repo.updated_at),
        "pushed_at": optional_timestamp(repo.repo.pushed_at),
    })
}

/// The `organization` field payloads carry for organisation-owned repositories.
fn organization(repo: &OwnedRepository, base_url: &str) -> Option<Value> {
    repo.owner.is_organization().then(|| account(&repo.owner, base_url))
}

/// A commit as listed in `push` payloads.
pub fn push_commit(commit: &CommitSummary, html_url: &str) -> Value {
    let message = match &commit.body {
        Some(body) if !body.is_empty() => format!("{}\n\n{body}", commit.summary),
        _ => commit.summary.clone(),
    };
    let author = json!({ "name": commit.author_name, "email": commit.author_email });
    json!({
        "id": commit.id,
        "message": message,
        "timestamp": timestamp(commit.authored_at),
        "url": format!("{html_url}/commit/{}", commit.id),
        "author": author,
        "committer": author,
        "distinct": true,
    })
}

/// A `push` payload for one ref update. `commits` are the commits the update
/// introduced, newest first (as `new_commits` returns them), `total` how many there were.
#[derive(Debug)]
pub struct PushPayload<'a> {
    pub repo: &'a OwnedRepository,
    pub update: &'a RefUpdate,
    pub commits: &'a [CommitSummary],
    pub total: usize,
    /// The account that pushed; `None` for deploy keys and mirrors.
    pub pusher: Option<&'a Account>,
    /// Whether the repository is a fork (its `fork` flag).
    pub fork: bool,
    pub base_url: &'a str,
}

impl PushPayload<'_> {
    pub fn to_json(&self) -> Value {
        let html_url = format!("{}/{}", self.base_url, self.repo.full_name());
        let before = self.update.before.as_deref().unwrap_or(NULL_SHA);
        let after = self.update.after.as_deref().unwrap_or(NULL_SHA);
        let compare = if self.update.is_created() {
            format!("{html_url}/commit/{after}")
        } else if self.update.is_deleted() {
            html_url.clone()
        } else {
            format!("{html_url}/compare/{}...{}", short(before), short(after))
        };
        // GitHub lists commits oldest first; `head_commit` is the new tip.
        let shown: Vec<Value> = self.commits.iter().take(MAX_PUSH_COMMITS).rev().map(|commit| push_commit(commit, &html_url)).collect();
        let head_commit = self.commits.first().filter(|commit| Some(commit.id.as_str()) == self.update.after.as_deref());
        let pusher = match self.pusher {
            Some(account) => json!({ "name": account.name.as_str(), "email": null }),
            None => json!({ "name": "deploy-key", "email": null }),
        };
        let mut payload = json!({
            "ref": self.update.name,
            "before": before,
            "after": after,
            "created": self.update.is_created(),
            "deleted": self.update.is_deleted(),
            "base_ref": null,
            "compare": compare,
            "commits": shown,
            "total_commits": self.total,
            "head_commit": head_commit.map(|commit| push_commit(commit, &html_url)),
            "pusher": pusher,
            "repository": repository(self.repo, self.base_url, self.fork),
            "sender": self.pusher.map(|who| account(who, self.base_url)),
        });
        if let Some(org) = organization(self.repo, self.base_url) {
            payload["organization"] = org;
        }
        payload
    }
}

fn short(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

/// A `repository` payload (`created`, `deleted` or `transferred`).
pub fn repository_event(action: &str, repo: &Value, sender: Option<&Account>, base_url: &str) -> Value {
    json!({
        "action": action,
        "repository": repo,
        "sender": sender.map(|who| account(who, base_url)),
    })
}

/// The `repository` object of a deleted repository: the row is gone, so only what the
/// event still carries.
pub fn deleted_repository(id: i64, full_name: &str, owner: Option<&Account>, base_url: &str) -> Value {
    let name = full_name.rsplit_once('/').map_or(full_name, |(_, name)| name);
    json!({
        "id": id,
        "name": name,
        "full_name": full_name,
        "owner": owner.map(|owner| account(owner, base_url)),
        "html_url": format!("{base_url}/{full_name}"),
    })
}

/// A `repository` payload with `action: transferred`, naming the previous owner.
pub fn transferred(repo: &OwnedRepository, fork: bool, from: &Account, sender: Option<&Account>, base_url: &str) -> Value {
    let key = match from.kind {
        AccountKind::User => "user",
        AccountKind::Organization => "organization",
    };
    let mut payload = repository_event("transferred", &repository(repo, base_url, fork), sender, base_url);
    payload["changes"] = json!({ "owner": { "from": { key: account(from, base_url) } } });
    with_organization(payload, repo, base_url)
}

/// Adds the `organization` field for organisation-owned repositories.
pub fn with_organization(mut payload: Value, repo: &OwnedRepository, base_url: &str) -> Value {
    if let Some(org) = organization(repo, base_url) {
        payload["organization"] = org;
    }
    payload
}

/// A `fork` payload: `forkee` is the new repository, `repository` the one forked
/// (`parent_is_fork` when that one is itself a fork).
pub fn fork(forkee: &OwnedRepository, parent: &OwnedRepository, parent_is_fork: bool, sender: Option<&Account>, base_url: &str) -> Value {
    let payload = json!({
        "forkee": repository(forkee, base_url, true),
        "repository": repository(parent, base_url, parent_is_fork),
        "sender": sender.map(|who| account(who, base_url)),
    });
    with_organization(payload, parent, base_url)
}

/// A release with its files, in GitHub's release shape. `author` is the creator's
/// account and `uploaders` the accounts that uploaded files, as far as they still exist.
///
/// Rustit's API has no release endpoints, so GitHub's API links (`url`, `assets_url`,
/// `upload_url`) are absent; `html_url`, the archive links and every asset's
/// `browser_download_url` point at the web interface.
pub fn release(
    release: &Release,
    assets: &[ReleaseAsset],
    author: Option<&Account>,
    uploaders: &[Account],
    repo: &OwnedRepository,
    base_url: &str,
) -> Value {
    let html = format!("{base_url}/{}", repo.full_name());
    let tag = encode_path(&release.tag_name);
    let assets: Vec<Value> = assets
        .iter()
        .map(|asset| {
            let uploader = uploaders.iter().find(|who| Some(who.id) == asset.uploader_id);
            json!({
                "id": asset.id.0,
                "name": asset.name,
                "label": null,
                "state": "uploaded",
                "content_type": asset.content_type,
                "size": asset.size,
                "download_count": asset.download_count,
                "created_at": timestamp(asset.created_at),
                "updated_at": timestamp(asset.created_at),
                "browser_download_url": format!("{html}/releases/download/{tag}/{}", encode_path(&asset.name)),
                "uploader": uploader.map(|who| account(who, base_url)),
            })
        })
        .collect();
    json!({
        "id": release.id.0,
        "tag_name": release.tag_name,
        "name": release.name,
        "body": release.notes,
        "draft": release.is_draft,
        "prerelease": release.is_prerelease,
        "created_at": timestamp(release.created_at),
        "published_at": optional_timestamp(release.published_at),
        "author": author.map(|who| account(who, base_url)),
        "html_url": format!("{html}/releases/tag/{tag}"),
        "tarball_url": format!("{html}/archive/{tag}.tar.gz"),
        "zipball_url": format!("{html}/archive/{tag}.zip"),
        "assets": assets,
    })
}

/// A `release` payload (`action: published`); `release` is a [`release`] object.
pub fn release_published(release: &Value, repo: &OwnedRepository, fork: bool, sender: Option<&Account>, base_url: &str) -> Value {
    let payload = json!({
        "action": "published",
        "release": release,
        "repository": repository(repo, base_url, fork),
        "sender": sender.map(|who| account(who, base_url)),
    });
    with_organization(payload, repo, base_url)
}

/// A hook, as the API returns it and `ping` payloads embed it. Never includes the secret.
pub fn hook(hook: &Webhook, api_url: &str) -> Value {
    let events: Vec<&str> = hook.events.iter().map(|event| event.as_str()).collect();
    let kind = match hook.owner {
        WebhookOwner::Repository(_) => "Repository",
        WebhookOwner::Organization(_) => "Organization",
    };
    json!({
        "id": hook.id.0,
        "type": kind,
        "name": "web",
        "active": hook.active,
        "events": events,
        "config": {
            "url": hook.url.as_str(),
            "content_type": "json",
            "insecure_ssl": "0",
            "secret": if hook.has_secret { Value::String("********".to_owned()) } else { Value::Null },
        },
        "created_at": timestamp(hook.created_at),
        "updated_at": timestamp(hook.updated_at),
        "url": api_url,
        "last_response": hook.last_delivery.map(|last| json!({
            "status": last.status.as_str(),
            "code": last.response_status,
            "at": timestamp(last.at),
        })),
    })
}

/// Short, friendly sentences for `ping` payloads, in the spirit of GitHub's.
const ZEN: &[&str] = &[
    "Keep it logically awesome.",
    "Simplicity is the ultimate sophistication.",
    "Small commits, clear messages.",
    "Practicality beats purity.",
    "Design for failure.",
    "Speak like a human.",
];

/// A `ping` payload for a newly created (or explicitly pinged) hook.
pub fn ping(hook_json: &Value, hook_id: i64, context: Option<(&str, Value)>, sender: Option<&Account>, base_url: &str) -> Value {
    let zen = ZEN[usize::try_from(hook_id.unsigned_abs()).unwrap_or_default() % ZEN.len()];
    let mut payload = json!({
        "zen": zen,
        "hook_id": hook_id,
        "hook": hook_json,
        "sender": sender.map(|who| account(who, base_url)),
    });
    if let Some((key, value)) = context {
        payload[key] = value;
    }
    payload
}

/// The `action` stored with a delivery, read back from its payload.
pub fn action_of(event: WebhookEvent, payload: &Value) -> Option<String> {
    match event {
        WebhookEvent::Push | WebhookEvent::Fork | WebhookEvent::Ping => None,
        WebhookEvent::Repository
        | WebhookEvent::Release
        | WebhookEvent::Issues
        | WebhookEvent::IssueComment
        | WebhookEvent::PullRequest
        | WebhookEvent::PullRequestReview => payload.get("action").and_then(Value::as_str).map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use rustit_core::{AccountId, ReleaseId, RepoId, Repository, Visibility, release::ReleaseAssetId};

    use super::*;

    const BASE: &str = "https://git.example.com";

    fn org() -> Account {
        Account {
            id: AccountId(3),
            kind: AccountKind::Organization,
            name: "acme".parse().unwrap(),
            display_name: Some("Acme".to_owned()),
            bio: None,
            created_at: Timestamp(0),
        }
    }

    fn user() -> Account {
        Account {
            id: AccountId(1),
            kind: AccountKind::User,
            name: "ada".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        }
    }

    fn repo(owner: Account) -> OwnedRepository {
        OwnedRepository {
            repo: Repository {
                id: RepoId(7),
                owner_id: owner.id,
                name: "tools".parse().unwrap(),
                description: None,
                visibility: Visibility::Private,
                default_branch: "main".to_owned(),
                created_at: Timestamp(1_792_022_400),
                updated_at: Timestamp(1_792_022_460),
                pushed_at: None,
            },
            owner,
        }
    }

    fn commit(id: char, summary: &str) -> CommitSummary {
        CommitSummary {
            id: id.to_string().repeat(40),
            short_id: id.to_string().repeat(7),
            summary: summary.to_owned(),
            body: None,
            author_name: "Ada".to_owned(),
            author_email: "ada@example.com".to_owned(),
            authored_at: Timestamp(1_792_022_400),
            parent_ids: Vec::new(),
        }
    }

    #[test]
    fn repositories_have_github_fields() {
        let value = repository(&repo(org()), BASE, false);
        assert_eq!(value["id"], 7);
        assert_eq!(value["name"], "tools");
        assert_eq!(value["full_name"], "acme/tools");
        assert_eq!(value["private"], true);
        assert_eq!(value["visibility"], "private");
        assert_eq!(value["description"], Value::Null, "explicit null, not omitted");
        assert_eq!(value["fork"], false);
        assert_eq!(value["default_branch"], "main");
        assert_eq!(value["html_url"], "https://git.example.com/acme/tools");
        assert_eq!(value["clone_url"], "https://git.example.com/acme/tools.git");
        assert_eq!(value["url"], "https://git.example.com/api/v1/repos/acme/tools");
        assert_eq!(value["created_at"], "2026-10-15T00:00:00Z");
        assert_eq!(value["updated_at"], "2026-10-15T00:01:00Z");
        assert_eq!(value["pushed_at"], Value::Null);
        assert_eq!(value["owner"]["login"], "acme");
        assert_eq!(value["owner"]["type"], "Organization");
        assert_eq!(value["owner"]["url"], "https://git.example.com/api/v1/orgs/acme");
        assert_eq!(account(&user(), BASE)["name"], Value::Null);
        assert_eq!(account(&user(), BASE)["url"], "https://git.example.com/api/v1/users/ada");
    }

    #[test]
    fn push_payloads_list_commits_oldest_first_with_the_head() {
        let update = RefUpdate { name: "refs/heads/main".to_owned(), before: Some("a".repeat(40)), after: Some("c".repeat(40)) };
        let commits = [commit('c', "Third"), commit('b', "Second")];
        let pusher = user();
        let payload = PushPayload {
            repo: &repo(org()),
            update: &update,
            commits: &commits,
            total: 2,
            pusher: Some(&pusher),
            fork: false,
            base_url: BASE,
        }
        .to_json();

        assert_eq!(payload["ref"], "refs/heads/main");
        assert_eq!(payload["before"], "a".repeat(40));
        assert_eq!(payload["after"], "c".repeat(40));
        assert_eq!((payload["created"].as_bool(), payload["deleted"].as_bool()), (Some(false), Some(false)));
        assert_eq!(payload["compare"], "https://git.example.com/acme/tools/compare/aaaaaaaaaaaa...cccccccccccc");
        assert_eq!(payload["total_commits"], 2);
        let messages: Vec<_> = payload["commits"].as_array().unwrap().iter().map(|c| c["message"].as_str().unwrap()).collect();
        assert_eq!(messages, ["Second", "Third"]);
        assert_eq!(payload["commits"][0]["url"], format!("https://git.example.com/acme/tools/commit/{}", "b".repeat(40)));
        assert_eq!(payload["commits"][0]["author"]["email"], "ada@example.com");
        assert_eq!(payload["head_commit"]["id"], "c".repeat(40));
        assert_eq!(payload["pusher"], json!({ "name": "ada", "email": null }));
        assert_eq!(payload["sender"]["login"], "ada");
        assert_eq!(payload["repository"]["full_name"], "acme/tools");
        assert_eq!(payload["organization"]["login"], "acme");
        assert_eq!(payload["base_ref"], Value::Null);
    }

    #[test]
    fn created_and_deleted_refs_use_the_null_sha() {
        let created = RefUpdate { name: "refs/tags/v1".to_owned(), before: None, after: Some("c".repeat(40)) };
        let payload =
            PushPayload { repo: &repo(user()), update: &created, commits: &[], total: 0, pusher: None, fork: false, base_url: BASE }
                .to_json();
        assert_eq!(payload["before"], NULL_SHA);
        assert_eq!(payload["created"], true);
        assert_eq!(payload["head_commit"], Value::Null);
        assert_eq!(payload["sender"], Value::Null);
        assert_eq!(payload["pusher"]["name"], "deploy-key");
        assert!(payload.get("organization").is_none(), "personal repositories have no organisation");

        let deleted = RefUpdate { name: "refs/heads/old".to_owned(), before: Some("a".repeat(40)), after: None };
        let payload =
            PushPayload { repo: &repo(user()), update: &deleted, commits: &[], total: 0, pusher: None, fork: false, base_url: BASE }
                .to_json();
        assert_eq!((payload["after"].as_str(), payload["deleted"].as_bool()), (Some(NULL_SHA), Some(true)));
        assert_eq!(payload["compare"], "https://git.example.com/ada/tools");
    }

    #[test]
    fn push_payloads_cap_the_commit_list() {
        let update = RefUpdate { name: "refs/heads/main".to_owned(), before: None, after: Some("0".repeat(40)) };
        let commits: Vec<_> = (0..25).map(|_| commit('0', "Work")).collect();
        let payload =
            PushPayload { repo: &repo(user()), update: &update, commits: &commits, total: 31, pusher: None, fork: false, base_url: BASE }
                .to_json();
        assert_eq!(payload["commits"].as_array().unwrap().len(), MAX_PUSH_COMMITS);
        assert_eq!(payload["total_commits"], 31);
    }

    #[test]
    fn other_events_have_their_github_shapes() {
        let repository_value = repository(&repo(org()), BASE, false);
        let created = with_organization(repository_event("created", &repository_value, Some(&user()), BASE), &repo(org()), BASE);
        assert_eq!((created["action"].as_str(), created["sender"]["login"].as_str()), (Some("created"), Some("ada")));
        assert_eq!(created["organization"]["login"], "acme");

        let moved = transferred(&repo(org()), false, &user(), Some(&user()), BASE);
        assert_eq!(moved["action"], "transferred");
        assert_eq!(moved["changes"]["owner"]["from"]["user"]["login"], "ada");

        let gone = repository_event("deleted", &deleted_repository(7, "acme/tools", Some(&org()), BASE), None, BASE);
        assert_eq!((gone["repository"]["name"].as_str(), gone["repository"]["owner"]["login"].as_str()), (Some("tools"), Some("acme")));
        assert_eq!(gone["sender"], Value::Null);

        let forked = fork(&repo(user()), &repo(org()), false, Some(&user()), BASE);
        assert_eq!((forked["forkee"]["fork"].as_bool(), forked["repository"]["fork"].as_bool()), (Some(true), Some(false)));
        let forked_fork = fork(&repo(user()), &repo(org()), true, None, BASE);
        assert_eq!(forked_fork["repository"]["fork"], true, "a fork of a fork says so");
        assert_eq!(forked["forkee"]["full_name"], "ada/tools");

        let release = release_published(&json!({ "id": 9 }), &repo(org()), false, None, BASE);
        assert_eq!((release["action"].as_str(), release["release"]["id"].as_i64()), (Some("published"), Some(9)));
        assert_eq!(action_of(WebhookEvent::Release, &release).as_deref(), Some("published"));
        assert_eq!(action_of(WebhookEvent::Push, &release), None);
    }

    #[test]
    fn releases_have_github_fields() {
        let published = Release {
            id: ReleaseId(9),
            repo_id: RepoId(7),
            tag_name: "release/1.0".to_owned(),
            name: "One point oh".to_owned(),
            notes: "Notes".to_owned(),
            is_draft: false,
            is_prerelease: true,
            author: None,
            created_at: Timestamp(1_792_022_400),
            updated_at: Timestamp(1_792_022_400),
            published_at: Some(Timestamp(1_792_022_460)),
        };
        let asset = ReleaseAsset {
            id: ReleaseAssetId(4),
            release_id: published.id,
            repo_id: RepoId(7),
            name: "app 1.zip".to_owned(),
            size: 12,
            content_type: "application/zip".to_owned(),
            sha256: [0; 32],
            download_count: 3,
            uploader_id: Some(AccountId(1)),
            created_at: Timestamp(1_792_022_400),
        };
        let value = release(&published, &[asset], None, &[user()], &repo(org()), BASE);
        assert_eq!(value["id"], 9);
        assert_eq!(value["tag_name"], "release/1.0");
        assert_eq!(value["name"], "One point oh");
        assert_eq!(value["body"], "Notes");
        assert_eq!((value["draft"].as_bool(), value["prerelease"].as_bool()), (Some(false), Some(true)));
        assert_eq!(value["published_at"], "2026-10-15T00:01:00Z");
        assert_eq!(value["author"], Value::Null, "explicit null once the author is gone");
        assert_eq!(value["html_url"], "https://git.example.com/acme/tools/releases/tag/release/1.0");
        assert_eq!(value["zipball_url"], "https://git.example.com/acme/tools/archive/release/1.0.zip");
        let file = &value["assets"][0];
        assert_eq!((file["id"].as_i64(), file["name"].as_str(), file["size"].as_u64()), (Some(4), Some("app 1.zip"), Some(12)));
        assert_eq!((file["download_count"].as_u64(), file["state"].as_str()), (Some(3), Some("uploaded")));
        assert_eq!(file["uploader"]["login"], "ada");
        assert_eq!(file["browser_download_url"], "https://git.example.com/acme/tools/releases/download/release/1.0/app%201.zip");
    }
}
