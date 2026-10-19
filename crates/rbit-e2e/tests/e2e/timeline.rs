//! The shared timeline forms (T0) end to end: comments, title, description, state and
//! lock under `/{owner}/{repo}/issues/{n}/…` for issues and pull requests alike.
//!
//! There are no issue-creation pages yet (workstream I), so threads are seeded through
//! the store on the server's own database file; everything else goes over HTTP.
//!
//! The integrator owns this file.

use rbit_core::{IssueKind, IssueNumber, IssueState, IssueText, IssueTitle, StateReason, Timestamp};
use rbit_e2e::{Client, StatusCode, TestServer};
use rbit_storage::{Database, sqlite::issues::NewIssue};

/// The server's database, opened a second time (`SQLite` in WAL mode allows that).
struct Store {
    database: Database,
}

impl Store {
    async fn open(server: &TestServer) -> Self {
        Self { database: Database::open(&server.data_dir().join("rbit.db")).await.unwrap() }
    }

    /// Seeds thread `title` of `kind` in `owner/repo`, opened by `author`; returns its number.
    async fn seed(&self, owner: &str, repo: &str, author: &str, kind: IssueKind, title: &str) -> u32 {
        let author = self.database.accounts().find_account_by_name(author).await.unwrap().unwrap();
        let repo = self.database.repositories().find_by_path(owner, repo).await.unwrap().unwrap();
        let new = NewIssue {
            repo: repo.repo.id,
            kind,
            title: IssueTitle::parse(title).unwrap(),
            body: IssueText::body("Seeded").unwrap(),
            author: author.id,
            at: Timestamp::now(),
        };
        self.database.issues().create(&new).await.unwrap().number.get()
    }

    async fn issue(&self, owner: &str, repo: &str, number: u32) -> rbit_core::Issue {
        let repo = self.database.repositories().find_by_path(owner, repo).await.unwrap().unwrap();
        self.database.issues().find_by_number(repo.repo.id, IssueNumber::new(number).unwrap()).await.unwrap().unwrap()
    }

    /// Closes the second connection before the server (and its data directory) go away.
    async fn close(self) {
        self.database.close().await;
    }
}

fn comment_id(location: &str) -> String {
    location.rsplit_once("#issuecomment-").expect("a comment anchor").1.to_owned()
}

async fn redirect_to(client: &Client, path: &str, form: &[(&str, &str)], expected_prefix: &str) -> String {
    let response = client.post(path, form).await;
    response.expect_status(StatusCode::SEE_OTHER);
    let location = response.location().unwrap().to_owned();
    assert!(location.starts_with(expected_prefix), "{location} should start with {expected_prefix}");
    location
}

#[tokio::test]
async fn comments_state_and_lock_redirect_to_the_canonical_page() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    let bob = server.sign_up("bob").await;
    let store = Store::open(&server).await;
    let issue = store.seed("alice", "demo", "alice", IssueKind::Issue, "Crash on start").await;
    let pull = store.seed("alice", "demo", "alice", IssueKind::Pull, "Fix the crash").await;
    assert_eq!((issue, pull), (1, 2));

    // A reader comments on an issue and on a pull request; both post to `/issues/{n}/…`.
    let location = redirect_to(&bob, "/alice/demo/issues/1/comments", &[("body", "Same here")], "/alice/demo/issues/1#issuecomment-").await;
    let bobs_comment = comment_id(&location);
    redirect_to(&bob, "/alice/demo/issues/2/comments", &[("body", "Looks good")], "/alice/demo/pull/2#issuecomment-").await;
    assert_eq!(store.issue("alice", "demo", 1).await.comment_count, 1);

    // Editing and deleting comments.
    let edit = format!("/alice/demo/issues/1/comments/{bobs_comment}");
    bob.post(&edit, &[("body", "Same here, on Linux")]).await.expect_redirect(&format!("/alice/demo/issues/1#issuecomment-{bobs_comment}"));
    alice.post(&edit, &[("body", "Not yours")]).await.expect_status(StatusCode::FORBIDDEN);
    bob.post(&edit, &[("body", "  ")]).await.expect_status(StatusCode::BAD_REQUEST).expect_contains(&["comment must not be empty"]);
    bob.post("/alice/demo/issues/1/comments/999999", &[("body", "x")]).await.expect_status(StatusCode::NOT_FOUND);
    bob.post(&format!("/alice/demo/issues/2/comments/{bobs_comment}"), &[("body", "x")]).await.expect_status(StatusCode::NOT_FOUND);
    alice.post(&format!("{edit}/delete"), &[]).await.expect_redirect("/alice/demo/issues/1#new-comment");
    assert_eq!(store.issue("alice", "demo", 1).await.comment_count, 0, "the owner moderates");

    // Title and description: the author or writers only.
    bob.post("/alice/demo/issues/1/title", &[("title", "Mine now")]).await.expect_status(StatusCode::FORBIDDEN);
    alice.post("/alice/demo/issues/1/title", &[("title", "Crash on start-up")]).await.expect_redirect("/alice/demo/issues/1#issue-title");
    alice.post("/alice/demo/issues/2/body", &[("body", "Fixes the crash")]).await.expect_redirect("/alice/demo/pull/2#issue-body");
    let stored = store.issue("alice", "demo", 1).await;
    assert_eq!(stored.title.as_str(), "Crash on start-up");
    assert_eq!(store.issue("alice", "demo", 2).await.body.as_str(), "Fixes the crash");

    // State: the reader may not close alice's issue, with or without a comment, and the
    // refused "close with comment" leaves no comment behind.
    bob.post("/alice/demo/issues/1/state", &[("state", "close_completed")]).await.expect_status(StatusCode::FORBIDDEN);
    bob.post("/alice/demo/issues/1/comments", &[("body", "Closing"), ("state", "close_completed")])
        .await
        .expect_status(StatusCode::FORBIDDEN);
    assert_eq!(store.issue("alice", "demo", 1).await.comment_count, 0);
    alice.post("/alice/demo/issues/1/state", &[("state", "close_merged")]).await.expect_status(StatusCode::BAD_REQUEST);
    redirect_to(
        &alice,
        "/alice/demo/issues/1/comments",
        &[("body", "Fixed in main"), ("state", "close_not_planned")],
        "/alice/demo/issues/1#issuecomment-",
    )
    .await;
    let closed = store.issue("alice", "demo", 1).await;
    assert_eq!((closed.state, closed.state_reason, closed.comment_count), (IssueState::Closed, Some(StateReason::NotPlanned), 1));
    alice
        .post("/alice/demo/issues/1/comments", &[("body", ""), ("state", "reopen")])
        .await
        .expect_redirect("/alice/demo/issues/1#new-comment");
    assert_eq!(store.issue("alice", "demo", 1).await.state, IssueState::Open, "a state change without a comment");
    alice.post("/alice/demo/issues/2/state", &[("state", "close_completed")]).await.expect_redirect("/alice/demo/pull/2#new-comment");
    assert_eq!(store.issue("alice", "demo", 2).await.state, IssueState::Closed);
    alice.post("/alice/demo/issues/1/comments", &[("body", "")]).await.expect_status(StatusCode::BAD_REQUEST);

    // Locking: writers only; then readers can no longer comment.
    bob.post("/alice/demo/issues/1/lock", &[("locked", "true")]).await.expect_status(StatusCode::FORBIDDEN);
    alice.post("/alice/demo/issues/1/lock", &[("locked", "maybe")]).await.expect_status(StatusCode::BAD_REQUEST);
    alice.post("/alice/demo/issues/1/lock", &[("locked", "true")]).await.expect_redirect("/alice/demo/issues/1#new-comment");
    assert!(store.issue("alice", "demo", 1).await.locked);
    bob.post("/alice/demo/issues/1/comments", &[("body", "Hello?")]).await.expect_status(StatusCode::FORBIDDEN);
    redirect_to(&alice, "/alice/demo/issues/1/comments", &[("body", "Locked for now")], "/alice/demo/issues/1#issuecomment-").await;
    alice.post("/alice/demo/issues/1/lock", &[("locked", "false")]).await.expect_redirect("/alice/demo/issues/1#new-comment");

    // The Issues and Pull requests tabs count open threads (issue 1 open, pull 2 closed).
    let home = bob.get("/alice/demo").await;
    home.expect_status(StatusCode::OK).expect_contains(&[r#"<span class="badge tabular-nums">1</span>"#]);

    store.close().await;
}

#[tokio::test]
async fn hidden_repositories_unknown_numbers_and_foreign_origins_are_refused() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "secret", "private").await;
    alice.create_repo("alice", "demo", "public").await;
    let mallory = server.sign_up("mallory").await;
    let store = Store::open(&server).await;
    store.seed("alice", "secret", "alice", IssueKind::Issue, "Private matter").await;
    store.seed("alice", "demo", "alice", IssueKind::Issue, "Public matter").await;

    for path in [
        "/alice/secret/issues/1/comments",
        "/alice/secret/issues/1/state",
        "/alice/secret/issues/1/lock",
        "/alice/secret/issues/1/title",
        "/alice/secret/issues/1/comments/1",
        "/alice/secret/issues/1/comments/1/delete",
        "/alice/nothing/issues/1/comments",
        "/alice/demo/issues/2/comments",
        "/alice/demo/issues/0/comments",
        "/alice/demo/issues/01/comments",
        "/alice/demo/issues/abc/comments",
        "/alice/demo/issues/-1/comments",
        "/alice/demo/issues/1/comments/abc",
    ] {
        mallory
            .post(path, &[("body", "hi"), ("state", "reopen"), ("locked", "true"), ("title", "x")])
            .await
            .expect_status(StatusCode::NOT_FOUND);
    }
    // The owner gets through, so the 404s above were about visibility, not the routes.
    alice.post("/alice/secret/issues/1/comments", &[("body", "Note to self")]).await.expect_status(StatusCode::SEE_OTHER);

    // Anonymous visitors are sent to sign in; a foreign Origin is refused outright.
    let anonymous = server.client();
    let response = anonymous.post("/alice/demo/issues/1/comments", &[("body", "hi")]).await;
    response.expect_status(StatusCode::SEE_OTHER);
    assert!(response.location().unwrap().starts_with("/login?next="));
    mallory
        .post_with("/alice/demo/issues/1/comments", &[("body", "csrf")], &[("Origin", "http://evil.example")])
        .await
        .expect_status(StatusCode::FORBIDDEN);
    assert_eq!(store.issue("alice", "demo", 1).await.comment_count, 0);

    store.close().await;
}
