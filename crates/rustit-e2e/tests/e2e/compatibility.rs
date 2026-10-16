//! Git client compatibility over smart HTTP: protocol versions, shallow and partial
//! clones, incremental fetches, ref updates and deletions, and large pushes.

use std::{fmt::Write as _, io::Write as _, path::Path, process::Stdio};

use rustit_e2e::{Git, PASSWORD, Sample, StatusCode, TestServer};

/// A server with `alice/demo` holding the sample history.
async fn seeded() -> (TestServer, Git, Sample) {
    let server = TestServer::start();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("alice", "demo", "alice", PASSWORD));
    (server, git, sample)
}

#[tokio::test]
async fn every_protocol_version_clones() {
    let (server, git, sample) = seeded().await;
    let url = server.git_url("alice", "demo");
    for version in ["0", "1", "2"] {
        let target = git.clone_with(&url, &[&format!("protocol.version={version}")], &[]);
        assert_eq!(git.ok(&target, &["rev-parse", "HEAD"]).trim(), sample.head, "protocol v{version}");
        assert_eq!(git.ok(&target, &["tag", "--list"]).trim(), Sample::TAG, "protocol v{version}");
    }

    // ls-remote lists every ref, peeled tags included.
    let refs = git.ok(&sample.dir, &["ls-remote", &url]);
    for name in ["HEAD", "refs/heads/main", "refs/heads/feature", "refs/tags/v1.0", "refs/tags/v1.0^{}"] {
        assert!(refs.lines().any(|line| line.ends_with(&format!("\t{name}"))), "{name} missing from:\n{refs}");
    }

    // The dumb protocol is refused with a clear message.
    let dumb = server.client().get("/alice/demo.git/info/refs").await;
    dumb.expect_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn shallow_partial_and_incremental_fetches() {
    let (server, git, sample) = seeded().await;
    let url = server.git_url("alice", "demo");

    let shallow = git.clone_with(&url, &[], &["--depth", "1"]);
    assert_eq!(git.ok(&shallow, &["rev-list", "--count", "HEAD"]).trim(), "1");
    git.ok(&shallow, &["fetch", "--quiet", "--unshallow"]);
    assert_eq!(git.ok(&shallow, &["rev-list", "--count", "HEAD"]).trim(), "2");

    let partial = git.clone_with(&url, &[], &["--filter=blob:none"]);
    assert_eq!(git.ok(&partial, &["config", "remote.origin.promisor"]).trim(), "true");
    assert_eq!(std::fs::read_to_string(partial.join("README.md")).expect("checked out"), Sample::README);

    // A second clone fetches only what is new.
    let follower = git.clone(&url);
    let pushed = git.commit_file(&sample.dir, "later.txt", "pushed after the clone\n");
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "main"]);
    git.ok(&follower, &["pull", "--quiet", "--ff-only"]);
    assert_eq!(git.ok(&follower, &["rev-parse", "HEAD"]).trim(), pushed);
    let web = server.client();
    web.get("/alice/demo/commits").await.expect_status(StatusCode::OK).expect_contains(&["Add later.txt"]);
}

#[tokio::test]
async fn ref_updates_force_pushes_and_deletions() {
    let (server, git, sample) = seeded().await;
    let url = server.git_url_as("alice", "demo", "alice", PASSWORD);

    // Non-fast-forward pushes are refused unless forced.
    git.ok(&sample.dir, &["checkout", "--quiet", "-b", "rewrite", "main~1"]);
    let rewritten = git.commit_file(&sample.dir, "rewrite.txt", "rewritten history\n");
    let refused = git.fails(&sample.dir, &["push", &url, "rewrite:main"]);
    assert!(refused.contains("rejected"), "{refused}");
    git.ok(&sample.dir, &["push", "--quiet", "--force", &url, "rewrite:main"]);
    assert_eq!(git.ok(&sample.dir, &["ls-remote", &url, "refs/heads/main"]).split('\t').next(), Some(rewritten.as_str()));

    // Deleting a branch and a tag.
    git.ok(&sample.dir, &["push", "--quiet", &url, "--delete", Sample::BRANCH, Sample::TAG]);
    let remaining = git.ok(&sample.dir, &["ls-remote", &url]);
    assert!(!remaining.contains("refs/heads/feature") && !remaining.contains("refs/tags/v1.0"), "{remaining}");
    let web = server.client();
    let branches = web.get("/alice/demo/branches").await;
    branches.expect_status(StatusCode::OK);
    assert!(!branches.contains(">feature<"), "the deleted branch is gone from the web");
    web.get("/alice/demo/tree/feature").await.expect_status(StatusCode::NOT_FOUND);
    web.get("/alice/demo/blob/main/rewrite.txt").await.expect_status(StatusCode::OK).expect_contains(&["rewritten history"]);
}

#[tokio::test]
async fn large_pushes_and_fetches_stream_through() {
    let (server, git, sample) = seeded().await;
    // Incompressible content well above Git's 1 MiB `http.postBuffer`, so the push uses
    // chunked transfer encoding.
    let mut state = 0x2545_f491_u32;
    let noise: String = (0..3 * 1024 * 1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            char::from(b'!' + u8::try_from(state % 90).expect("small"))
        })
        .collect();
    let big = git.commit_file(&sample.dir, "big.txt", &noise);
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "main"]);

    let clone = git.clone(&server.git_url("alice", "demo"));
    assert_eq!(git.ok(&clone, &["rev-parse", "HEAD"]).trim(), big);
    assert_eq!(std::fs::read_to_string(clone.join("big.txt")).expect("cloned"), noise);
}

/// Creates `count` branches `wide-{n}`, each with its own commit on top of `main`, in a
/// single `git fast-import` run.
fn create_wide_branches(git: &Git, dir: &Path, count: usize) {
    let mut stream = String::new();
    for n in 0..count {
        let message = format!("Wide commit {n}\n");
        let content = format!("branch {n}\n");
        write!(
            stream,
            "commit refs/heads/wide-{n}\ncommitter E2E Tester <e2e@example.test> 1700000000 +0000\ndata {}\n{message}from refs/heads/main\nM 644 inline wide.txt\ndata {}\n{content}\n",
            message.len(),
            content.len()
        )
        .expect("writing to a String cannot fail");
    }
    let mut child = git.command(dir).args(["fast-import", "--quiet"]).stdin(Stdio::piped()).spawn().expect("start git fast-import");
    child.stdin.take().expect("stdin").write_all(stream.as_bytes()).expect("feed git fast-import");
    assert!(child.wait().expect("wait for git fast-import").success(), "git fast-import failed");
}

#[tokio::test]
async fn wide_fetches_with_gzipped_requests() {
    let (server, git, sample) = seeded().await;
    // Sixty distinct tips make the clone's `want` list exceed 1 KiB, the size above
    // which Git gzips its upload-pack request.
    create_wide_branches(&git, &sample.dir, 60);
    git.ok(
        &sample.dir,
        &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "refs/heads/wide-*:refs/heads/wide-*"],
    );

    let clone = git.clone_with(&server.git_url("alice", "demo"), &["protocol.version=0"], &[]);
    let branches = git.ok(&clone, &["branch", "--remotes", "--list", "origin/wide-*"]);
    assert_eq!(branches.lines().count(), 60);

    // Confirm from Git's own trace that the request really was compressed.
    let target = git.fresh_path("traced");
    let output = git
        .command(&sample.dir)
        .args(["clone", "--quiet", &server.git_url("alice", "demo"), &target.to_string_lossy()])
        .env("GIT_TRACE_CURL", "1")
        .env("GIT_TRACE_CURL_NO_DATA", "1")
        .output()
        .expect("run git clone");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let trace = String::from_utf8_lossy(&output.stderr);
    assert!(trace.contains("Content-Encoding: gzip"), "the upload-pack request was gzipped");
    assert_eq!(git.ok(&target, &["show", "origin/wide-59:wide.txt"]), "branch 59\n");
}
