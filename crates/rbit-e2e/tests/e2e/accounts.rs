//! Accounts: the first administrator, signing in and out, runtime registration and
//! suspension.

use rbit_e2e::{Client, Git, PASSWORD, StatusCode, TestServer, numbers_between};

/// The instance settings form with `registration` set to `open` or `closed`.
fn settings_form(registration: &str) -> [(&'static str, &str); 4] {
    [("site_name", "rbit"), ("registration", registration), ("org_creation", "any_user"), ("default_visibility", "public")]
}

/// The account id of `username`, read from the administrator's user list.
async fn user_id(admin: &Client, username: &str) -> i64 {
    let page = admin.get(&format!("/admin/users?q={username}")).await;
    page.expect_status(StatusCode::OK);
    let ids = numbers_between(&page.body, "/admin/users/", "/suspend");
    assert_eq!(ids.len(), 1, "exactly one suspendable user matches {username}");
    ids[0]
}

#[tokio::test]
async fn first_sign_up_becomes_administrator_and_later_ones_do_not() {
    let server = TestServer::start();

    let alice = server.sign_up("alice").await;
    alice.get("/admin").await.expect_status(StatusCode::OK);
    alice.get("/admin/users").await.expect_status(StatusCode::OK).expect_contains(&["alice"]);

    let bob = server.sign_up("bob").await;
    // The panel is hidden, not forbidden: non-administrators never learn it exists.
    bob.get("/admin").await.expect_status(StatusCode::NOT_FOUND);
    bob.get("/admin/users").await.expect_status(StatusCode::NOT_FOUND);
    bob.post("/admin/settings", &settings_form("closed")).await.expect_status(StatusCode::NOT_FOUND);

    // Anonymous visitors are asked to sign in first, like on every signed-in page.
    server.client().get("/admin").await.expect_redirect("/login?next=/admin");
}

#[tokio::test]
async fn the_first_sign_up_needs_the_setup_token() {
    let server = TestServer::start();
    server.client().get("/signup").await.expect_status(StatusCode::OK).expect_contains(&[r#"name="setup_token""#]);

    let anonymous = server.client();
    for token in ["", "wrong-token"] {
        let form = [("username", "mallory"), ("email", "mallory@example.test"), ("password", PASSWORD), ("setup_token", token)];
        anonymous.post("/signup", &form).await.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["setup token"]);
    }

    let alice = server.sign_up("alice").await;
    alice.get("/admin").await.expect_status(StatusCode::OK);
    // Later sign-ups neither show nor need it.
    let page = server.client().get("/signup").await;
    assert!(!page.body.contains(r#"name="setup_token""#));
    let form = [("username", "bob"), ("email", "bob@example.test"), ("password", PASSWORD)];
    server.client().post("/signup", &form).await.expect_redirect("/");
}

#[tokio::test]
async fn guessing_the_current_password_in_settings_is_throttled() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let form =
        [("current_password", "a wrong guess"), ("new_password", "another long password"), ("confirm_password", "another long password")];
    let mut statuses = Vec::new();
    for _ in 0..20 {
        let response = alice.post("/settings/account/password", &form).await;
        statuses.push(response.status);
        if response.status == StatusCode::TOO_MANY_REQUESTS {
            assert!(response.header("retry-after").is_some());
            break;
        }
    }
    assert_eq!(statuses.last(), Some(&StatusCode::TOO_MANY_REQUESTS), "{statuses:?}");
    assert!(statuses[..statuses.len() - 1].iter().all(|status| *status == StatusCode::UNPROCESSABLE_ENTITY));
    // The email form shares the budget, even with the right password.
    let email = [("current_password", PASSWORD), ("email", "alice@elsewhere.test")];
    alice.post("/settings/account/email", &email).await.expect_status(StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn sign_in_and_out() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    alice.get("/settings/profile").await.expect_status(StatusCode::OK);

    alice.sign_out().await.expect_redirect("/");
    let signed_out = alice.get("/settings/profile").await;
    signed_out.expect_status(StatusCode::SEE_OTHER);
    assert!(signed_out.location().is_some_and(|l| l.starts_with("/login?next=")), "signed-out visitors are sent to sign in");

    let wrong = alice.sign_in("alice", "not-the-password").await;
    wrong.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["Incorrect username or password."]);
    let unknown = alice.sign_in("nobody", PASSWORD).await;
    unknown.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["Incorrect username or password."]);
    alice.get("/settings/profile").await.expect_status(StatusCode::SEE_OTHER);

    // The email address works as a login too, and `next` is honoured.
    alice
        .post("/login", &[("login", "alice@example.test"), ("password", PASSWORD), ("next", "/settings/profile")])
        .await
        .expect_redirect("/settings/profile");
    alice.get("/settings/profile").await.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn signing_out_revokes_the_session_on_the_server() {
    let server = TestServer::start();
    let browser = server.client();
    let signed_up = browser.sign_up("alice").await;
    let cookie = signed_up.header("set-cookie").and_then(|c| c.split(';').next()).expect("a session cookie").to_owned();

    // The bare cookie works from anywhere while the session lives...
    let elsewhere = server.client();
    elsewhere.get_with("/settings/profile", &[("Cookie", &cookie)]).await.expect_status(StatusCode::OK);
    // ...and not after signing out, even if a copy was kept.
    browser.sign_out().await.expect_redirect("/");
    elsewhere.get_with("/settings/profile", &[("Cookie", &cookie)]).await.expect_status(StatusCode::SEE_OTHER);
    // Garbage cookies are ignored rather than failing the request.
    elsewhere.get_with("/", &[("Cookie", "rbit_session=not-a-token")]).await.expect_status(StatusCode::OK);
}

#[tokio::test]
async fn administrator_closes_and_reopens_registration() {
    let server = TestServer::start();
    let admin = server.sign_up("alice").await;

    admin.post("/admin/settings", &settings_form("closed")).await.expect_redirect("/admin/settings?saved=1");
    let refused = server.client().sign_up("bob").await;
    refused.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    // Nothing was created: the name is still free once registration reopens.
    let users = admin.get("/admin/users?q=bob").await;
    users.expect_status(StatusCode::OK);
    assert!(numbers_between(&users.body, "/admin/users/", "/suspend").is_empty(), "no account was created");

    admin.post("/admin/settings", &settings_form("open")).await.expect_redirect("/admin/settings?saved=1");
    server.sign_up("bob").await;
}

#[tokio::test]
async fn suspended_user_cannot_sign_in_or_use_git() {
    let server = TestServer::start();
    let git = Git::new();
    let admin = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    bob.create_repo("bob", "notes", "private").await;
    let token = bob.create_token("laptop", "write").await;
    let sample = git.sample();
    git.push_sample(&sample, &server.git_url_as("bob", "notes", "bob", PASSWORD));

    let id = user_id(&admin, "bob").await;
    admin.post(&format!("/admin/users/{id}/suspend"), &[]).await.expect_status(StatusCode::SEE_OTHER);

    // The live session ends, and signing in again is refused with an explanation.
    let ended = bob.get("/settings/profile").await;
    ended.expect_status(StatusCode::SEE_OTHER);
    bob.sign_in("bob", PASSWORD).await.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["suspended"]);

    // Git refuses the password (403, with a reason) and the token (as an unknown credential).
    let refs = "/bob/notes.git/info/refs?service=git-upload-pack";
    bob.get_basic(refs, "bob", PASSWORD).await.expect_status(StatusCode::FORBIDDEN).expect_contains(&["suspended"]);
    bob.get_basic(refs, "bob", &token).await.expect_status(StatusCode::UNAUTHORIZED);
    git.clone_fails(&server.git_url_as("bob", "notes", "bob", PASSWORD));
    git.clone_fails(&server.git_url_as("bob", "notes", "bob", &token));

    // Lifting the suspension restores access.
    admin.post(&format!("/admin/users/{id}/unsuspend"), &[]).await.expect_status(StatusCode::SEE_OTHER);
    server.sign_in("bob", PASSWORD).await;
    git.clone(&server.git_url_as("bob", "notes", "bob", &token));
}
