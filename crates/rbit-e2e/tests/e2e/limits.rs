//! Resource limits of long-running requests: Git smart HTTP (process slots per instance
//! and per address, stalled exchanges, push size) and archive downloads.

use std::time::{Duration, Instant};

use rbit_e2e::{Git, PASSWORD, StatusCode, TestServer};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

/// Sends `request` on a raw connection and reads until the response headers are in (the
/// handler has answered, so it holds whatever slot it took), then stops reading.
async fn hold(server: &TestServer, request: &str) -> (TcpStream, String) {
    let mut stream = TcpStream::connect(server.addr()).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut head = Vec::new();
    let mut byte = [0; 1];
    while !head.ends_with(b"\r\n\r\n") {
        let read = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut byte)).await.expect("no response").unwrap();
        assert_eq!(read, 1, "connection closed: {}", String::from_utf8_lossy(&head));
        head.push(byte[0]);
    }
    (stream, String::from_utf8(head).unwrap())
}

/// An upload-pack request whose body never arrives: git waits for it.
async fn stalled_fetch(server: &TestServer) -> TcpStream {
    let request = format!(
        "POST /alice/demo.git/git-upload-pack HTTP/1.1\r\nHost: {}\r\nContent-Type: application/x-git-upload-pack-request\r\n\
         Content-Length: 1000\r\n\r\n",
        server.addr()
    );
    let (stream, head) = hold(server, &request).await;
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    stream
}

/// Polls the ref advertisement until it answers `200` again (the held slot was freed).
async fn wait_until_admitted(server: &TestServer) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let response = server.client().get("/alice/demo.git/info/refs?service=git-upload-pack").await;
        if response.status == StatusCode::OK {
            return;
        }
        assert!(Instant::now() < deadline, "the slot was never freed: {}", response.status);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn public_demo(server: &TestServer, git: &Git) {
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    git.push_sample(&git.sample(), &server.git_url_as("alice", "demo", "alice", PASSWORD));
}

#[tokio::test]
async fn git_requests_are_limited_per_address_and_stalled_ones_are_stopped() {
    let server = TestServer::start_with(|config| {
        config.repository.max_git_requests_per_ip = 1;
        config.repository.git_idle_timeout_seconds = 3;
    });
    let git = Git::new();
    public_demo(&server, &git).await;

    let _stalled = stalled_fetch(&server).await;
    let refused = server.client().get("/alice/demo.git/info/refs?service=git-upload-pack").await;
    refused.expect_status(StatusCode::TOO_MANY_REQUESTS);
    assert!(refused.header("retry-after").is_some());

    // The stalled exchange is stopped after a few seconds without traffic, freeing its slot.
    wait_until_admitted(&server).await;
    git.clone(&server.git_url("alice", "demo"));
}

#[tokio::test]
async fn a_busy_instance_refuses_git_requests_until_a_slot_frees() {
    let server = TestServer::start_with(|config| {
        config.repository.max_git_processes = 1;
        config.repository.git_idle_timeout_seconds = 3;
    });
    let git = Git::new();
    public_demo(&server, &git).await;

    let stalled = stalled_fetch(&server).await;
    let refused = server.client().get("/alice/demo.git/info/refs?service=git-upload-pack").await;
    refused.expect_status(StatusCode::SERVICE_UNAVAILABLE);
    assert!(refused.header("retry-after").is_some());

    // A client that goes away gives its slot back too.
    drop(stalled);
    wait_until_admitted(&server).await;
}

#[tokio::test]
async fn pushes_larger_than_the_limit_are_refused() {
    let server = TestServer::start_with(|config| config.repository.max_push_bytes = 64 * 1024);
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "private").await;
    let url = server.git_url_as("alice", "demo", "alice", PASSWORD);
    let sample = git.sample();
    // Small pushes are unaffected.
    git.push_sample(&sample, &url);

    git.commit_file(&sample.dir, "noise.txt", &noise(512 * 1024));
    git.fails(&sample.dir, &["push", "--quiet", &url, "main"]);
    let remote = git.ok(&sample.dir, &["ls-remote", &url, "refs/heads/main"]);
    assert!(remote.starts_with(&sample.head), "main must not move: {remote}");
}

#[tokio::test]
async fn archives_are_limited_per_address_and_stalled_downloads_are_stopped() {
    let server = TestServer::start_with(|config| {
        config.repository.max_archives_per_ip = 1;
        config.repository.archive_idle_timeout_seconds = 2;
    });
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "big", "public").await;
    let sample = git.sample();
    // Far more than the connection buffers take, so a client that stops reading stalls.
    git.commit_file(&sample.dir, "noise.txt", &noise(24 * 1024 * 1024));
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "big", "alice", PASSWORD), "main", "v1.0"]);

    let request = format!("GET /alice/big/archive/main.zip HTTP/1.1\r\nHost: {}\r\n\r\n", server.addr());
    let (_stalled, head) = hold(&server, &request).await;
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let refused = server.client().get("/alice/big/archive/main.tar.gz").await;
    refused.expect_status(StatusCode::TOO_MANY_REQUESTS);
    assert!(refused.header("retry-after").is_some());

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let response = server.client().get("/alice/big/archive/v1.0.tar.gz").await;
        if response.status == StatusCode::OK {
            break;
        }
        assert!(Instant::now() < deadline, "the stalled download kept its slot: {}", response.status);
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// `len` bytes of hexadecimal noise that compresses poorly.
fn noise(len: usize) -> String {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            char::from(b"0123456789abcdef"[usize::try_from(state & 15).unwrap()])
        })
        .collect()
}
