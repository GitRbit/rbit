//! Actions: shared cache. Runners save `actions/cache` entries through their running task
//! and restore them from any runner of the repository; the server picks the scopes, so an
//! untrusted run cannot poison what trusted runs restore. Repositories keep a bounded
//! number of bytes, and the sweeper removes entries nobody restored.

use axum::http::{Method, StatusCode};
use rbit_core::{Timestamp, TokenScope};
use serde_json::json;

use crate::{
    actions_api::{api, token},
    actions_artifacts::{Answer, call, hex, stored},
    actions_runner::Fixture,
    actions_runner_tasks::{payload, poll, seed, seed_run, spec, template},
};

fn version(text: &str) -> String {
    hex(text.as_bytes())
}

async fn save(fixture: &Fixture, credential: &str, task: i64, key: &str, version: &str, bytes: &[u8]) -> Answer {
    let path = format!("tasks/{task}/caches?key={key}&version={version}");
    call(fixture, Method::PUT, &path, credential, Some(&hex(bytes)), bytes.to_vec()).await
}

async fn lookup(fixture: &Fixture, credential: &str, task: i64, key: &str, restore_keys: &[&str], version: &str) -> Answer {
    let body = json!({"key": key, "restore_keys": restore_keys, "version": version});
    call(fixture, Method::POST, &format!("tasks/{task}/caches/lookup"), credential, None, serde_json::to_vec(&body).unwrap()).await
}

/// A task of a new run, trusted or not.
async fn task(fixture: &Fixture, credential: &str, trusted: bool, key: &str) -> i64 {
    seed_run(fixture, &[payload(&template("build", &["linux"], &[]))], trusted).await;
    spec(&poll(fixture, credential, Some(key)).await.1).task_id.0
}

#[tokio::test]
async fn runners_save_and_restore_entries_of_their_repository() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let first = task(&fixture, &credential, true, "a").await;
    let (paths, bytes) = (version("target\nLinux"), stored(&[("target/out", b"built")]));

    let saved = save(&fixture, &credential, first, "cargo-1", &paths, &bytes).await;
    assert_eq!(saved.status, StatusCode::CREATED, "{:?}", saved.json());
    assert_eq!((saved.json()["key"].as_str(), saved.json()["scope"].as_str()), (Some("cargo-1"), Some("heads/main")));
    let again = save(&fixture, &credential, first, "cargo-1", &paths, &bytes).await;
    assert_eq!(again.status, StatusCode::CONFLICT, "entries are immutable");

    // Another run, and so possibly another runner, restores it.
    let later = task(&fixture, &credential, true, "b").await;
    let exact = lookup(&fixture, &credential, later, "cargo-1", &[], &paths).await.json();
    assert_eq!(exact["entry"]["key"], "cargo-1");
    let prefix = lookup(&fixture, &credential, later, "cargo-2", &["nope-", "cargo-"], &paths).await.json();
    assert_eq!(prefix["entry"]["key"], "cargo-1", "the newest entry starting with a restore key");
    let other = lookup(&fixture, &credential, later, "cargo-1", &[], &version("other paths")).await.json();
    assert_eq!(other["entry"], serde_json::Value::Null, "another version never matches");

    let id = exact["entry"]["id"].as_i64().unwrap();
    let download = call(&fixture, Method::GET, &format!("tasks/{later}/caches/{id}"), &credential, None, Vec::new()).await;
    assert_eq!(download.status, StatusCode::OK);
    assert_eq!(download.body, bytes);
    assert_eq!(download.headers["rbit-content-sha256"], hex(&bytes).as_str());

    let bad =
        call(&fixture, Method::PUT, &format!("tasks/{later}/caches?key=x&version={paths}"), &credential, Some(&hex(b"a")), b"b".to_vec())
            .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "the body must match its digest");
    let comma = lookup(&fixture, &credential, later, "a,b", &[], &paths).await;
    assert_eq!(comma.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn untrusted_runs_poison_nothing_trusted_runs_restore() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let paths = version("target\nLinux");
    let trusted = task(&fixture, &credential, true, "a").await;
    assert_eq!(save(&fixture, &credential, trusted, "cargo-1", &paths, &stored(&[("a", b"good")])).await.status, StatusCode::CREATED);

    let untrusted = task(&fixture, &credential, false, "b").await;
    let found = lookup(&fixture, &credential, untrusted, "cargo-1", &[], &paths).await.json();
    assert_eq!(found["entry"]["scope"], "heads/main", "untrusted runs read the default branch's entries");
    let poison = save(&fixture, &credential, untrusted, "cargo-1", &paths, &stored(&[("a", b"evil")])).await;
    assert_eq!(poison.status, StatusCode::CREATED, "it saves into its own scope");
    assert!(poison.json()["scope"].as_str().unwrap().starts_with("untrusted/"));
    let poison_id = poison.json()["id"].as_i64().unwrap();

    let next = task(&fixture, &credential, true, "c").await;
    let restored = lookup(&fixture, &credential, next, "cargo-1", &[], &paths).await.json();
    assert_eq!(restored["entry"]["scope"], "heads/main");
    let stolen = call(&fixture, Method::GET, &format!("tasks/{next}/caches/{poison_id}"), &credential, None, Vec::new()).await;
    assert_eq!(stolen.status, StatusCode::NOT_FOUND, "a task downloads only entries of its own scopes");
}

#[tokio::test]
async fn a_full_repository_evicts_the_least_recently_used_entries() {
    let fixture = Fixture::with(|config| config.actions.max_cache_storage_bytes = 1000).await;
    let credential = fixture.runner(&["linux"]).await;
    let paths = version("p");
    let task = task(&fixture, &credential, true, "a").await;
    let blob = |seed: u8| (0..600u32).map(|index| index.to_le_bytes()[0].wrapping_mul(seed)).collect::<Vec<u8>>();

    assert_eq!(save(&fixture, &credential, task, "one", &paths, &blob(3)).await.status, StatusCode::CREATED);
    assert_eq!(save(&fixture, &credential, task, "two", &paths, &blob(5)).await.status, StatusCode::CREATED);
    assert_eq!(lookup(&fixture, &credential, task, "one", &[], &paths).await.json()["entry"], serde_json::Value::Null, "evicted");
    assert_eq!(lookup(&fixture, &credential, task, "two", &[], &paths).await.json()["entry"]["key"], "two");
    let huge = save(&fixture, &credential, task, "huge", &paths, &[7u8; 1001]).await;
    assert_eq!(huge.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn the_sweeper_removes_entries_nobody_restored() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let paths = version("p");
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = spec(&poll(&fixture, &credential, Some("a")).await.1).task_id.0;
    assert_eq!(save(&fixture, &credential, task, "old", &paths, b"bytes").await.status, StatusCode::CREATED);
    let repo = fixture.repo().await.repo.repo.id;
    assert_eq!(fixture.database.actions_caches().stored_bytes(repo).await.unwrap(), 5);

    let swept = fixture.actions().sweep(Timestamp(Timestamp::now().0 + 8 * 86_400)).await.unwrap();
    assert_eq!(swept.expired_caches, 1, "{run:?}");
    assert_eq!(fixture.database.actions_caches().stored_bytes(repo).await.unwrap(), 0);
}

// Actions: cache REST API.
#[tokio::test]
async fn the_rest_api_lists_measures_and_deletes_entries_as_github_does() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let paths = version("p");
    let task = task(&fixture, &credential, true, "a").await;
    for (key, bytes) in [("cargo-a", &b"aaaa"[..]), ("cargo-b", b"bb"), ("node-a", b"n")] {
        assert_eq!(save(&fixture, &credential, task, key, &paths, bytes).await.status, StatusCode::CREATED);
    }
    let (read, write) = (token(&fixture, &fixture.alice, TokenScope::Read).await, token(&fixture, &fixture.alice, TokenScope::Write).await);
    let caches = "/repos/alice/rocket/actions/caches";

    let all = api(&fixture, Method::GET, caches, &read).await.json();
    assert_eq!(all["total_count"], 3);
    assert_eq!(all["actions_caches"][0]["ref"], "refs/heads/main");
    assert_eq!(all["actions_caches"][0]["version"], paths.as_str());
    let by_size = api(&fixture, Method::GET, &format!("{caches}?key=cargo-&sort=size_in_bytes&direction=asc"), &read).await.json();
    let keys: Vec<_> = by_size["actions_caches"].as_array().unwrap().iter().map(|entry| entry["key"].as_str().unwrap()).collect();
    assert_eq!(keys, ["cargo-b", "cargo-a"]);
    let other_ref = api(&fixture, Method::GET, &format!("{caches}?ref=refs/heads/other"), &read).await.json();
    assert_eq!(other_ref["total_count"], 0);
    assert_eq!(api(&fixture, Method::GET, &format!("{caches}?sort=name"), &read).await.status, StatusCode::UNPROCESSABLE_ENTITY);
    let usage = api(&fixture, Method::GET, "/repos/alice/rocket/actions/cache/usage", &read).await.json();
    assert_eq!((usage["active_caches_count"].as_u64(), usage["active_caches_size_in_bytes"].as_u64()), (Some(3), Some(7)));

    let path = format!("{caches}?key=cargo-a&ref=refs/heads/main");
    assert_eq!(api(&fixture, Method::DELETE, &path, &read).await.status, StatusCode::FORBIDDEN, "deleting needs the write scope");
    let deleted = api(&fixture, Method::DELETE, &path, &write).await;
    assert_eq!((deleted.status, deleted.json()["total_count"].as_u64()), (StatusCode::OK, Some(1)));
    assert_eq!(api(&fixture, Method::DELETE, &path, &write).await.status, StatusCode::NOT_FOUND);
    let id = by_size["actions_caches"][0]["id"].as_i64().unwrap();
    assert_eq!(api(&fixture, Method::DELETE, &format!("{caches}/{id}"), &write).await.status, StatusCode::NO_CONTENT);
    assert_eq!(api(&fixture, Method::GET, caches, &read).await.json()["total_count"], 1);
}
