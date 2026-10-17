//! Fetching for imports, pushing push mirrors and finding LFS pointers, against local
//! repositories made with the `git` CLI (`file://` remotes, allowed explicitly) and a
//! listener that records what git sends over HTTP.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use rbit_git::remote::{Remote, RemoteError, fetch_import, lfs_pointers, push_mirror};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const TIMEOUT: Duration = Duration::from_secs(60);

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stderr(Stdio::inherit())
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn commit(dir: &Path, file: &str, content: &str) -> String {
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", content]);
    git(dir, &["rev-parse", "HEAD"])
}

fn file_url(path: &Path) -> String {
    format!("file:///{}", path.to_string_lossy().replace('\\', "/").trim_start_matches('/'))
}

fn bare(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    git(root, &["init", "-q", "--bare", name]);
    path
}

fn refs(repo: &Path) -> Vec<String> {
    git(repo, &["for-each-ref", "--format=%(refname)"]).lines().map(str::to_owned).collect()
}

fn local(url: &str) -> Remote<'_> {
    Remote { url, authorization: None, allow_local: true }
}

/// A work tree with `main`, a `feature` branch, a tag and a pull request head, pushed
/// to a bare "upstream".
fn upstream(root: &Path) -> (PathBuf, PathBuf) {
    let work = root.join("work");
    std::fs::create_dir(&work).unwrap();
    git(&work, &["init", "-q"]);
    commit(&work, "README.md", "hello");
    git(&work, &["tag", "v1"]);
    git(&work, &["checkout", "-q", "-b", "feature"]);
    let feature = commit(&work, "feature.txt", "feature");
    git(&work, &["checkout", "-q", "main"]);
    let upstream = bare(root, "upstream.git");
    git(&work, &["push", "-q", &file_url(&upstream), "main", "feature", "v1"]);
    git(&upstream, &["update-ref", "refs/pull/1/head", &feature]);
    git(&upstream, &["update-ref", "refs/pull/1/merge", &feature]);
    (work, upstream)
}

#[tokio::test]
async fn imports_fetch_branches_tags_and_pull_request_heads() {
    let dir = tempfile::tempdir().unwrap();
    let (_work, upstream) = upstream(dir.path());
    let url = file_url(&upstream);

    let target = bare(dir.path(), "target.git");
    fetch_import(Path::new("git"), &target, local(&url), true, TIMEOUT).await.unwrap();
    assert_eq!(refs(&target), ["refs/heads/feature", "refs/heads/main", "refs/pull/1/head", "refs/tags/v1"], "not refs/pull/*/merge");

    let without = bare(dir.path(), "without.git");
    fetch_import(Path::new("git"), &without, local(&url), false, TIMEOUT).await.unwrap();
    assert_eq!(refs(&without), ["refs/heads/feature", "refs/heads/main", "refs/tags/v1"]);

    let refused = Remote { allow_local: false, ..local(&url) };
    let result = fetch_import(Path::new("git"), &bare(dir.path(), "refused.git"), refused, true, TIMEOUT).await;
    assert!(matches!(result, Err(RemoteError::Failed(_))), "file:// needs the switch: {result:?}");
}

#[tokio::test]
async fn push_mirrors_force_push_and_prune_branches_and_tags_only() {
    let dir = tempfile::tempdir().unwrap();
    let (work, source) = upstream(dir.path());
    let remote = bare(dir.path(), "remote.git");
    let url = file_url(&remote);

    push_mirror(Path::new("git"), &source, local(&url), TIMEOUT).await.unwrap();
    assert_eq!(refs(&remote), ["refs/heads/feature", "refs/heads/main", "refs/tags/v1"], "pull request heads stay local");

    // The remote gets a branch of its own and diverges on main; the next push restores
    // the mirror's view exactly.
    let other = dir.path().join("other");
    git(dir.path(), &["clone", "-q", &url, "other"]);
    commit(&other, "stray.txt", "stray");
    git(&other, &["push", "-q", "origin", "HEAD:refs/heads/stray", "HEAD:refs/heads/main"]);
    git(&source, &["branch", "-D", "feature"]);
    push_mirror(Path::new("git"), &source, local(&url), TIMEOUT).await.unwrap();
    assert_eq!(refs(&remote), ["refs/heads/main", "refs/tags/v1"], "deleted and unknown branches are pruned");
    assert_eq!(git(&remote, &["rev-parse", "main"]), git(&work, &["rev-parse", "main"]), "main is force-pushed back");

    let missing = file_url(&dir.path().join("missing.git"));
    let result = push_mirror(Path::new("git"), &source, local(&missing), TIMEOUT).await;
    let Err(RemoteError::Failed(message)) = result else { panic!("expected a failure, got {result:?}") };
    assert_ne!(message, "");
}

#[tokio::test]
async fn credentials_travel_in_a_header_not_in_the_command_line() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let read = socket.read(&mut buffer).await.unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        socket.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        String::from_utf8_lossy(&request).into_owned()
    });

    let dir = tempfile::tempdir().unwrap();
    let target = bare(dir.path(), "target.git");
    let url = format!("http://{address}/a/b.git");
    let remote = Remote { url: &url, authorization: Some("Basic c2VjcmV0"), allow_local: false };
    let result = fetch_import(Path::new("git"), &target, remote, false, TIMEOUT).await;
    assert!(matches!(result, Err(RemoteError::Failed(_))), "{result:?}");
    let request = server.await.unwrap();
    assert!(request.to_ascii_lowercase().contains("authorization: basic c2vjcmv0"), "{request}");
    assert!(!git(&target, &["config", "--list", "--local"]).contains("c2VjcmV0"), "nothing is written to the repository");
    assert!(!format!("{remote:?}").contains("c2VjcmV0"), "debug output hides the header");
}

#[tokio::test]
async fn lfs_pointers_are_found_in_history() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir(&work).unwrap();
    git(&work, &["init", "-q"]);
    let oid = "4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393";
    let pointer = format!("version https://git-lfs.github.com/spec/v1\noid sha256:{oid}\nsize 12345\n");
    commit(&work, "big.bin", &pointer);
    commit(&work, "copy.bin", &pointer);
    commit(&work, "big.bin", &"not a pointer, but long enough to be considered at all\n".repeat(3));
    let pointers = lfs_pointers(Path::new("git"), &work.join(".git")).await.unwrap();
    assert_eq!(pointers.len(), 1, "each object once, also when it left the latest tree");
    assert_eq!((pointers[0].oid.as_str(), pointers[0].size), (oid, 12_345));

    let empty = bare(dir.path(), "empty.git");
    assert_eq!(lfs_pointers(Path::new("git"), &empty).await.unwrap().len(), 0);
}
