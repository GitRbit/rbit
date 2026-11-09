use super::*;

const HTML: &str = "https://git.example.com/acme/tools";

fn commit(index: usize) -> Value {
    json!({ "n": index })
}

fn envelope<'a>(update: PushedRef<'a>, commits: &'a [&'a str], pusher: Option<&'a str>) -> PushEnvelope<'a> {
    PushEnvelope {
        update,
        html_url: HTML,
        commits,
        render: &commit,
        total: commits.len(),
        pusher,
        repository: json!({ "id": 7 }),
        sender: pusher.map(|name| json!({ "login": name })),
        organization: Some(json!({ "login": "acme" })),
    }
}

#[test]
fn pushes_list_at_most_twenty_commits_oldest_first_with_the_head() {
    let ids: Vec<String> = (0..25).map(|n| format!("{n:040}")).collect();
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let before = "a".repeat(40);
    let update = PushedRef { name: "refs/heads/main", before: Some(&before), after: Some(ids[0]), created: false, deleted: false };
    let payload = envelope(update, &ids, Some("ada")).into_json();
    let listed: Vec<_> = payload["commits"].as_array().unwrap().iter().map(|commit| commit["n"].as_u64().unwrap()).collect();
    assert_eq!(listed, (0..20).rev().collect::<Vec<u64>>());
    assert_eq!(payload["head_commit"], json!({ "n": 0 }));
    assert_eq!(payload["total_commits"], 25);
    assert_eq!(payload["compare"], format!("{HTML}/compare/aaaaaaaaaaaa...000000000000"));
    assert_eq!(payload["pusher"], json!({ "name": "ada", "email": null }));
    assert_eq!((payload["base_ref"].clone(), payload["organization"]["login"].clone()), (Value::Null, json!("acme")));
}

#[test]
fn created_and_deleted_refs_use_the_null_sha() {
    let after = "c".repeat(40);
    let created = PushedRef { name: "refs/tags/v1", before: None, after: Some(&after), created: true, deleted: false };
    let payload = envelope(created, &[], None).into_json();
    assert_eq!((payload["before"].as_str(), payload["created"].as_bool()), (Some(NULL_SHA), Some(true)));
    assert_eq!(payload["compare"], format!("{HTML}/commit/{after}"));
    assert_eq!((payload["head_commit"].clone(), payload["sender"].clone()), (Value::Null, Value::Null));
    assert_eq!(payload["pusher"]["name"], "deploy-key");
    let deleted = PushedRef { name: "refs/heads/old", before: Some(&after), after: None, created: false, deleted: true };
    let payload = envelope(deleted, &[], None).into_json();
    assert_eq!((payload["after"].as_str(), payload["compare"].as_str()), (Some(NULL_SHA), Some(HTML)));
}

#[test]
fn envelopes_have_their_github_shapes() {
    let repo = json!({ "id": 7 });
    let ada = json!({ "login": "ada" });
    let moved = transferred(&repo, &ada, false, Some(&ada), None);
    assert_eq!(moved["changes"]["owner"]["from"]["user"]["login"], "ada");
    assert!(moved.get("organization").is_none());
    let moved = transferred(&repo, &json!({ "login": "acme" }), true, None, Some(json!({ "login": "acme" })));
    assert_eq!(moved["changes"]["owner"]["from"]["organization"]["login"], "acme");
    let gone = deleted_repository(7, "acme/tools", None, "https://x");
    assert_eq!(gone, json!({ "id": 7, "name": "tools", "full_name": "acme/tools", "owner": null, "html_url": "https://x/acme/tools" }));
    let edited = member("edited", (Some("write"), Some("admin")), &ada, &repo, None, None);
    assert_eq!(edited["changes"], json!({ "permission": { "from": "write", "to": "admin" }, "old_permission": { "from": "write" } }));
    assert!(member("removed", (None, None), &ada, &repo, None, None).get("changes").is_none());
    let pinged = ping(&json!({ "id": 8 }), 8, Some(("repository", repo.clone())), None);
    assert_eq!((pinged["zen"].as_str(), pinged["repository"]["id"].as_i64()), (Some("Small commits, clear messages."), Some(7)));
    assert_eq!(ping(&Value::Null, -1, None, None)["zen"], "Simplicity is the ultimate sophistication.");
    let published = release_published(&json!({ "id": 9 }), &repo, Some(&ada), None);
    assert_eq!(action_of("release", &published).as_deref(), Some("published"));
    for event in ["push", "fork", "ping", "status"] {
        assert_eq!(action_of(event, &published), None, "{event}");
    }
}
