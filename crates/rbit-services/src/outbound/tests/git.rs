//! Git remotes (pull mirrors, push mirrors, import fetches): Git connects only to the
//! answer rbit checked, whatever spelling of the host the URL uses; it follows no
//! redirect; and a stalled remote does not outlive the operation's time limit.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use rbit_core::{RepoId, Visibility};
use rbit_git::{
    mirror::{FetchError, fetch_mirror},
    remote::{Remote, RemoteError, fetch_import, push_mirror},
};

use super::{Fixture, LOOPBACK, Reply, Server, name};
use crate::{mirrors::CreateMirror, outbound::scripted};

fn bare(dir: &Path) -> PathBuf {
    let path = dir.join("target.git");
    let status = std::process::Command::new("git").args(["init", "-q", "--bare"]).arg(&path).status().expect("git is installed");
    assert!(status.success());
    path
}

async fn mirror(fixture: &Fixture, actor: &rbit_core::User, name: &str, url: String) -> RepoId {
    let input =
        CreateMirror { name: name.to_owned(), description: String::new(), visibility: Visibility::Public, url, interval_minutes: None };
    fixture.services.mirrors.create(actor, "alice", input).await.unwrap().repo.id
}

async fn last_error(fixture: &Fixture, repo: RepoId) -> String {
    fixture.database.mirrors().find(repo).await.unwrap().unwrap().last_error.unwrap_or_default()
}

#[tokio::test]
async fn mirror_fetches_use_the_checked_answer_for_every_spelling_of_the_host() {
    let server = Server::always(Reply::status("403 Forbidden", "", b"")).await;
    let host = name("pin-alias");
    // The name exists only in the scripted resolver: Git can reach the listener only
    // through the answer rbit checked and pinned, never through a lookup of its own.
    let dns = scripted::serve(&host, &[&[LOOPBACK]]);
    let fixture = Fixture::new(|config| config.mirrors.allow_private_addresses = true).await;
    let alice = fixture.user("alice").await;
    let full_width = host.replacen('p', "\u{ff50}", 1);
    let spellings = [host.clone(), host.to_ascii_uppercase(), format!("{host}."), full_width];
    for (index, spelling) in spellings.iter().enumerate() {
        let repo =
            mirror(&fixture, &alice, &format!("alias{index}"), format!("http://{spelling}:{}/acme/rocket.git", server.address.port()))
                .await;
        let before = server.connections();
        fixture.services.mirrors.sync(repo).await.unwrap();
        let error = last_error(&fixture, repo).await;
        if spelling.is_ascii() || error.contains("403") {
            assert!(error.contains("403"), "{spelling}: the pinned listener answered: {error}");
            assert!(server.connections() > before, "{spelling} reached the checked address");
        } else {
            // curl builds without IDN support refuse the full-width spelling before
            // connecting anywhere, which is just as safe as reaching the pinned answer.
            assert!(!error.is_empty(), "{spelling}: an unconvertible host fails the fetch");
            assert_eq!(server.connections(), before, "{spelling}: a refused URL connects nowhere");
        }
        assert_eq!(dns.lookups(), 2 * (index + 1), "{spelling}: checked when saved and before the fetch");
    }
    for head in server.heads() {
        let lower = head.to_ascii_lowercase();
        assert!(lower.contains(&format!("host: {host}")), "the HTTP host stays the URL's: {head}");
    }
}

#[tokio::test]
async fn git_follows_no_redirect() {
    let elsewhere = Server::always(Reply::status("200 OK", "", b"")).await;
    let location = elsewhere.url("/acme/rocket.git/info/refs?service=git-upload-pack");
    let server = Server::always(Reply::redirect(&location)).await;
    let fixture = Fixture::new(|config| config.mirrors.allow_private_addresses = true).await;
    let alice = fixture.user("alice").await;
    let repo = mirror(&fixture, &alice, "redirected", server.url("/acme/rocket.git")).await;
    fixture.services.mirrors.sync(repo).await.unwrap();
    assert!(!last_error(&fixture, repo).await.is_empty(), "a redirect fails the fetch");

    let dir = tempfile::tempdir().unwrap();
    let target = bare(dir.path());
    let url = server.url("/acme/rocket.git");
    let remote = Remote { url: &url, authorization: Some("Basic c2VjcmV0"), allow_local: false, resolution: None };
    assert!(fetch_import(Path::new("git"), &target, remote, false, Duration::from_secs(30)).await.is_err());
    assert!(push_mirror(Path::new("git"), &target, remote, Duration::from_secs(30)).await.is_err());
    assert!(server.connections() >= 3);
    assert_eq!(elsewhere.connections(), 0, "no redirect was followed, and the credentials stayed where they were sent");
}

#[tokio::test]
async fn a_stalled_remote_does_not_outlive_the_time_limit() {
    let server = Server::always(Reply::Hang).await;
    let limit = Duration::from_secs(2);
    let fixture = Fixture::new(|config| {
        config.mirrors.allow_private_addresses = true;
        config.mirrors.fetch_timeout_seconds = limit.as_secs();
    })
    .await;
    let alice = fixture.user("alice").await;
    let repo = mirror(&fixture, &alice, "stalled", server.url("/acme/rocket.git")).await;
    let started = Instant::now();
    fixture.services.mirrors.sync(repo).await.unwrap();
    assert!(last_error(&fixture, repo).await.contains("longer than"), "the fetch timed out");

    let dir = tempfile::tempdir().unwrap();
    let target = bare(dir.path());
    let url = server.url("/acme/rocket.git");
    let remote = Remote { url: &url, authorization: None, allow_local: false, resolution: None };
    assert!(matches!(fetch_mirror(Path::new("git"), &target, &url, None, limit).await, Err(FetchError::TimedOut(_))));
    assert!(matches!(fetch_import(Path::new("git"), &target, remote, false, limit).await, Err(RemoteError::TimedOut(_))));
    assert!(matches!(push_mirror(Path::new("git"), &target, remote, limit).await, Err(RemoteError::TimedOut(_))));
    assert!(started.elapsed() < limit * 4 + Duration::from_secs(10), "each operation stopped at its limit");
    assert!(server.connections() >= 4, "every operation reached the stalled remote");
    // Killing `git` leaves its HTTP helper running; the helper must give up by itself.
    assert!(server.all_closed(limit * 3).await, "every connection to the stalled remote was closed");
}
