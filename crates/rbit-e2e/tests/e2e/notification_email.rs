//! Email notifications (milestone 3, N) over HTTP with the `log` mail transport: a
//! mention is emailed to a verified address, threaded and with a one-click unsubscribe
//! link that a mail client can POST from its own origin; after it, no more email.

use std::time::Duration;

use rbit_core::config::MailTransport;
use rbit_e2e::{Client, MailLog, StatusCode, TestServer};

/// Whether the header bell shows the unread dot for `client`.
async fn has_unread(client: &Client) -> bool {
    client.get("/").await.contains("<span class=\"notification-dot\"")
}

/// Signs up `name` and follows the verification link the server emailed.
async fn verified_user(server: &TestServer, mail: MailLog, name: &str) -> Client {
    let client = server.sign_up(name).await;
    let sent = mail.wait_for(server, &format!("{name}@example.test"), 1).await;
    let link = sent[0].link_starting_with(&server.url("/email/verify/")).expect("a verification link");
    let path = link.strip_prefix(server.base_url()).unwrap().to_owned();
    client.get(&path).await.expect_status(StatusCode::OK).expect_contains(&["Email address verified"]);
    client
}

#[tokio::test]
async fn mentions_are_emailed_until_one_click_unsubscribe() {
    let mail = MailLog::capture();
    let server = TestServer::start_with(|config| {
        config.mail.transport = MailTransport::Log;
        config.mail.from_address = "rbit@example.test".to_owned();
    });
    let nell = verified_user(&server, mail, "nell").await;
    let ned = verified_user(&server, mail, "ned").await;
    let ned_address = "ned@example.test";
    nell.create_repo("nell", "inbox", "public").await;
    ned.get("/settings/notifications")
        .await
        .expect_status(StatusCode::OK)
        .expect_contains(&[r#"value="participating" class="mt-1 accent-(--ink)" checked"#, ned_address]);

    // A mention reaches ned's mailbox, says why and links to the thread.
    nell.post("/nell/inbox/issues", &[("title", "Flaky build"), ("body", "@ned can you look?")])
        .await
        .expect_redirect("/nell/inbox/issues/1");
    let sent = mail.wait_for(&server, ned_address, 2).await;
    let mention = &sent[1];
    assert_eq!(mention.subject(), "[nell/inbox] Flaky build (#1)");
    assert!(mention.text.starts_with("nell opened issue #1.\n\n@ned can you look?"), "{}", mention.text);
    assert!(mention.text.contains("You are receiving this because you were mentioned."));
    assert!(mention.text.contains(&server.url("/nell/inbox/issues/1")));
    assert!(mention.text.contains(&server.url("/settings/notifications")));
    assert_eq!(mention.header("Message-ID"), Some("<nell/inbox/issues/1@127.0.0.1>"));
    assert_eq!(mention.header("List-Unsubscribe-Post"), Some("List-Unsubscribe=One-Click"));
    let unsubscribe = mention.header("List-Unsubscribe").unwrap().trim_matches(['<', '>']).to_owned();
    assert!(unsubscribe.starts_with(&server.url("/email/unsubscribe/")), "{unsubscribe}");
    assert!(mail.to(&server, "nell@example.test").len() == 1, "the actor got nothing beyond her verification email");

    // The link itself only asks for confirmation (mail scanners follow links).
    let path = unsubscribe.strip_prefix(server.base_url()).unwrap().to_owned();
    let anonymous = server.client();
    let confirm = anonymous.get(&path).await;
    confirm.expect_status(StatusCode::OK).expect_contains(&["Stop notification emails", "ned", "Turn off notification emails"]);
    assert_eq!(confirm.header("cache-control"), Some("no-store"));
    assert_eq!(confirm.header("referrer-policy"), Some("no-referrer"));
    anonymous.get("/email/unsubscribe/1.forged").await.expect_status(StatusCode::NOT_FOUND).expect_contains(&["This link is not valid"]);

    // A mail client unsubscribes in one POST from its own origin (RFC 8058); other forms
    // stay protected against cross-site posts.
    let cross_site = [("Origin", "https://mail.example"), ("Sec-Fetch-Site", "cross-site")];
    anonymous
        .post_with(&path, &[("List-Unsubscribe", "One-Click")], &cross_site)
        .await
        .expect_status(StatusCode::OK)
        .expect_contains(&["Notification emails are off"]);
    ned.post_with("/settings/notifications", &[("email", "all")], &cross_site).await.expect_status(StatusCode::FORBIDDEN);
    ned.get("/settings/notifications").await.expect_contains(&[r#"value="off" class="mt-1 accent-(--ink)" checked"#]);

    // Nell comments: ned still gets the inbox notification, and no email.
    ned.get("/nell/inbox/issues/1").await.expect_status(StatusCode::OK);
    nell.post("/nell/inbox/issues/1/comments", &[("body", "Any news, @ned?")]).await.expect_status(StatusCode::SEE_OTHER);
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !has_unread(&ned).await {
        assert!(std::time::Instant::now() < deadline, "timed out waiting for ned's notification");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(mail.to(&server, ned_address).len(), 2, "no email after unsubscribing");

    // Turning emails back on in the settings works as usual.
    ned.post("/settings/notifications", &[("email", "all")]).await.expect_redirect("/settings/notifications?saved=1");
    ned.get("/settings/notifications?saved=1")
        .await
        .expect_contains(&["Preferences saved.", r#"value="all" class="mt-1 accent-(--ink)" checked"#]);
    ned.post("/settings/notifications", &[("email", "sometimes")]).await.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    nell.post("/nell/inbox/issues/1/comments", &[("body", "Found it")]).await.expect_status(StatusCode::SEE_OTHER);
    let sent = mail.wait_for(&server, ned_address, 3).await;
    assert_eq!(sent[2].subject(), "Re: [nell/inbox] Flaky build (#1)");
    assert_eq!(sent[2].header("In-Reply-To"), Some("<nell/inbox/issues/1@127.0.0.1>"), "threaded under the issue");
}
