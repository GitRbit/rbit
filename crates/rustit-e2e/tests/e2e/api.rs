//! The REST API and webhook deliveries, end to end.
//!
//! Milestone 2 workstream W (API & webhooks) owns this file.
//!
//! Webhook deliveries go to a local receiver on 127.0.0.1, so these servers run with
//! `[webhooks] allow_private_addresses = true`.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{extract::State, http::HeaderMap, routing::post};
use hmac::{Hmac, KeyInit, Mac};
use rustit_e2e::{Git, PASSWORD, StatusCode, TestServer, numbers_between};
use sha2::Sha256;

// ------------------------------------------------------------------ a webhook receiver

/// One request the receiver accepted.
#[derive(Debug, Clone)]
struct Received {
    headers: HeaderMap,
    body: String,
}

impl Received {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("deliveries are JSON")
    }

    /// Checks `X-Hub-Signature-256` against `secret` with an independent HMAC.
    fn signed_with(&self, secret: &str) -> bool {
        let Some(hex) = self.header("x-hub-signature-256").and_then(|value| value.strip_prefix("sha256=")) else { return false };
        let Some(signature) = decode_hex(hex) else { return false };
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).expect("any key length");
        mac.update(self.body.as_bytes());
        mac.verify_slice(&signature).is_ok() && self.header("x-rustit-signature-256") == self.header("x-hub-signature-256")
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    (0..hex.len()).step_by(2).map(|i| hex.get(i..i + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok())).collect()
}

type Inbox = Arc<Mutex<Vec<Received>>>;

/// An HTTP endpoint on 127.0.0.1 that records every `POST /hook` and answers 204.
#[derive(Debug, Clone)]
struct Receiver {
    url: String,
    inbox: Inbox,
}

impl Receiver {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind the receiver");
        let url = format!("http://{}/hook", listener.local_addr().expect("receiver address"));
        let inbox = Inbox::default();
        let app = axum::Router::new().route("/hook", post(receive)).with_state(Arc::clone(&inbox));
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve the receiver") });
        Self { url, inbox }
    }

    /// Waits until `count` deliveries arrived and returns them in arrival order.
    async fn wait_for(&self, count: usize) -> Vec<Received> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let received = self.inbox.lock().expect("inbox").clone();
            if received.len() >= count {
                return received;
            }
            assert!(Instant::now() < deadline, "only {} of {count} deliveries arrived", received.len());
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn count(&self) -> usize {
        self.inbox.lock().expect("inbox").len()
    }
}

async fn receive(State(inbox): State<Inbox>, headers: HeaderMap, body: String) -> StatusCode {
    inbox.lock().expect("inbox").push(Received { headers, body });
    StatusCode::NO_CONTENT
}

fn server_for_webhooks() -> TestServer {
    TestServer::start_with(|config| config.webhooks.allow_private_addresses = true)
}

// ------------------------------------------------------------------ webhook settings UI

#[tokio::test]
async fn repository_webhooks_are_managed_in_settings_and_deliver_signed_events() {
    let server = server_for_webhooks();
    let git = Git::new();
    let alice = server.sign_up("alice").await;
    let bob = server.sign_up("bob").await;
    alice.create_repo("alice", "demo", "private").await;
    let receiver = Receiver::start().await;
    let hooks = "/alice/demo/settings/hooks";

    alice.get("/alice/demo/settings").await.expect_status(StatusCode::OK).expect_contains(&[hooks, "Webhooks"]);
    alice.get(hooks).await.expect_status(StatusCode::OK).expect_contains(&["No webhooks", "Add webhook"]);
    alice.get(&format!("{hooks}/new")).await.expect_status(StatusCode::OK).expect_contains(&["Payload URL", "name=\"event_push\""]);
    for outsider in [&bob, &server.client()] {
        outsider.get(hooks).await.expect_status(StatusCode::NOT_FOUND);
    }

    // Validation re-renders the form with what was entered (but never the secret).
    let rejected = alice.post(hooks, &[("url", &receiver.url), ("secret", "topsecret"), ("active", "on")]).await;
    rejected.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["Choose at least one event.", &receiver.url]);
    assert!(!rejected.contains("topsecret"));
    let internal = TestServer::start();
    let strict = internal.sign_up("carol").await;
    strict.create_repo("carol", "demo", "public").await;
    strict
        .post("/carol/demo/settings/hooks", &[("url", "http://127.0.0.1:9/hook"), ("event_push", "on"), ("active", "on")])
        .await
        .expect_status(StatusCode::UNPROCESSABLE_ENTITY)
        .expect_contains(&["internal address"]);
    drop(internal);

    let created = alice.post(hooks, &[("url", &receiver.url), ("secret", "topsecret"), ("event_push", "on"), ("active", "on")]).await;
    created.expect_status(StatusCode::SEE_OTHER);
    let hook_url = created.location().expect("redirect to the hook").to_owned();
    assert!(hook_url.starts_with(&format!("{hooks}/")), "{hook_url}");

    // The ping arrives, signed.
    let ping = &receiver.wait_for(1).await[0];
    assert_eq!(ping.header("x-rustit-event"), Some("ping"));
    assert_eq!(ping.header("x-github-event"), Some("ping"));
    assert!(ping.signed_with("topsecret"), "the HMAC verifies");
    assert!(!ping.signed_with("wrong secret"));
    assert_eq!(ping.json()["repository"]["full_name"], "alice/demo");

    // The hook page shows the secret as configured and the delivery once recorded.
    let page = wait_for_page(&alice, &hook_url, |body| body.contains("Delivered · 204")).await;
    page.expect_contains(&["Recent deliveries", ">ping<", "Configured — leave blank to keep it", "Send ping"]);
    assert!(!page.contains("topsecret"), "the secret is write-only");
    alice.get(hooks).await.expect_contains(&["127.0.0.1", "Pushes", "Delivered · 204"]);

    // A push is delivered as one signed `push` event.
    let sample = git.sample();
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "main"]);
    let push = &receiver.wait_for(2).await[1];
    assert_eq!(push.header("x-rustit-event"), Some("push"));
    assert!(push.signed_with("topsecret"));
    let payload = push.json();
    assert_eq!(payload["ref"], "refs/heads/main");
    assert_eq!(payload["after"], sample.head);
    assert_eq!(payload["total_commits"], 2);
    assert_eq!(payload["pusher"]["name"], "alice");

    // The delivery page shows what was sent and received; redelivery sends it again.
    let page = wait_for_page(&alice, &hook_url, |body| body.contains(">push<") && body.matches("Delivered · 204").count() == 2).await;
    let ids = numbers_between(&page.body, &format!("{hook_url}/deliveries/"), "\"");
    let push_page = alice.get(&format!("{hook_url}/deliveries/{}", ids[0])).await;
    push_page.expect_status(StatusCode::OK).expect_contains(&["X-Rustit-Signature-256", "refs/heads/main", "Redeliver", "204"]);
    assert!(!push_page.contains("topsecret"));
    let again = alice.post(&format!("{hook_url}/deliveries/{}/redeliver", ids[0]), &[]).await;
    again.expect_status(StatusCode::SEE_OTHER);
    assert_eq!(receiver.wait_for(3).await[2].body, push.body, "the same payload");
    alice.post(&format!("{hook_url}/ping"), &[]).await.expect_redirect(&format!("{hook_url}?pinged=1"));
    receiver.wait_for(4).await;
    bob.post(&format!("{hook_url}/ping"), &[]).await.expect_status(StatusCode::NOT_FOUND);

    // Editing keeps the secret unless it is removed; deactivated hooks stop delivering.
    let edited = alice.post(&hook_url, &[("url", &receiver.url), ("event_push", "on"), ("event_release", "on")]).await;
    edited.expect_redirect(&format!("{hook_url}?saved=1"));
    alice.get(&format!("{hook_url}?saved=1")).await.expect_contains(&["Webhook saved.", "Configured — leave blank to keep it"]);
    let before = receiver.count();
    git.commit_file(&sample.dir, "later.txt", "inactive hooks hear nothing\n");
    git.ok(&sample.dir, &["push", "--quiet", &server.git_url_as("alice", "demo", "alice", PASSWORD), "main"]);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(receiver.count(), before, "inactive hooks receive nothing");

    // Deleting needs the confirmation box.
    let delete = format!("{hook_url}/delete");
    alice.post(&delete, &[]).await.expect_status(StatusCode::UNPROCESSABLE_ENTITY).expect_contains(&["Tick the box to confirm."]);
    alice.post(&delete, &[("confirm", "on")]).await.expect_redirect(hooks);
    alice.get(hooks).await.expect_contains(&["No webhooks"]);
    alice.get(&hook_url).await.expect_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn organisation_webhooks_fire_for_new_repositories() {
    let server = server_for_webhooks();
    let olivia = server.sign_up("olivia").await;
    let mason = server.sign_up("mason").await;
    olivia.create_org("acme").await;
    olivia.add_member("acme", "mason", "member").await;
    let receiver = Receiver::start().await;
    let hooks = "/acme/settings/hooks";

    olivia.get("/acme/settings").await.expect_status(StatusCode::OK).expect_contains(&[hooks]);
    olivia.get(hooks).await.expect_status(StatusCode::OK).expect_contains(&["No webhooks", "or any of its repositories"]);
    mason.get(hooks).await.expect_status(StatusCode::NOT_FOUND);

    olivia.post(hooks, &[("url", &receiver.url), ("event_repository", "on"), ("active", "on")]).await.expect_status(StatusCode::SEE_OTHER);
    assert_eq!(receiver.wait_for(1).await[0].json()["organization"]["login"], "acme");
    mason.create_repo("acme", "rocket", "private").await;
    let created = receiver.wait_for(2).await[1].json();
    assert_eq!(created["action"], "created");
    assert_eq!(created["repository"]["full_name"], "acme/rocket");
    assert_eq!(created["sender"]["login"], "mason");
    assert!(receiver.wait_for(2).await[1].header("x-hub-signature-256").is_none(), "no secret, no signature");
}

/// Polls `path` until its body satisfies `ready` (deliveries are recorded asynchronously).
async fn wait_for_page(client: &rustit_e2e::Client, path: &str, ready: impl Fn(&str) -> bool) -> rustit_e2e::Response {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let page = client.get(path).await;
        page.expect_status(StatusCode::OK);
        if ready(&page.body) || Instant::now() > deadline {
            return page;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
