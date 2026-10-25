//! Email digests over HTTP with the `log` mail transport: a user who chooses a daily digest
//! in the settings gets no single emails, then one digest grouped by repository that
//! leaves out what they read in the meantime and carries the one-click unsubscribe
//! headers; running the schedule again sends nothing more.

use std::time::Duration;

use rbit_core::{Timestamp, config::EmailDelivery, config::MailTransport};
use rbit_e2e::{Client, MailLog, StatusCode, TestServer};
use rbit_services::notification_email::period_start;

const DAY: i64 = 24 * 60 * 60;

/// Signs up `name` and follows the verification link the server emailed.
async fn verified_user(server: &TestServer, mail: MailLog, name: &str) -> Client {
    let client = server.sign_up(name).await;
    let sent = mail.wait_for(server, &format!("{name}@example.test"), 1).await;
    let link = sent[0].link_starting_with(&server.url("/email/verify/")).expect("a verification link");
    let path = link.strip_prefix(server.base_url()).unwrap().to_owned();
    client.get(&path).await.expect_status(StatusCode::OK).expect_contains(&["Email address verified"]);
    client
}

/// Runs the digest schedule as if it were `at`; returns how many digests were sent.
async fn send_digests_at(server: &TestServer, at: Timestamp) -> u64 {
    server
        .with_services(move |services| async move { services.notification_email.with_clock(move || at).send_digests().await.unwrap().sent })
        .await
}

#[tokio::test]
async fn a_daily_digest_replaces_single_emails() {
    let mail = MailLog::capture();
    let server = TestServer::start_with(|config| {
        config.mail.transport = MailTransport::Log;
        config.mail.from_address = "rbit@example.test".to_owned();
    });
    let pia = verified_user(&server, mail, "pia").await;
    let pim = verified_user(&server, mail, "pim").await;
    let pim_address = "pim@example.test";
    pia.create_repo("pia", "alpha", "public").await;
    pia.create_repo("pia", "beta", "public").await;

    pim.post("/settings/notifications", &[("email", "participating"), ("delivery", "daily"), ("digest_hour", "9")])
        .await
        .expect_redirect("/settings/notifications?saved=1");
    pim.get("/settings/notifications")
        .await
        .expect_contains(&[r#"value="daily" class="mt-1 accent-(--ink)" checked"#, r#"<option value="9" selected>09:00 UTC</option>"#]);

    for (repo, title) in [("beta", "Release notes"), ("alpha", "Flaky build"), ("alpha", "Read already")] {
        pia.post(&format!("/pia/{repo}/issues"), &[("title", title), ("body", "@pim can you look?")])
            .await
            .expect_status(StatusCode::SEE_OTHER);
    }
    // The inbox has all three; pim reads one of them on the site.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !pim.get("/notifications").await.contains("Read already") {
        assert!(std::time::Instant::now() < deadline, "timed out waiting for pim's notifications");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    pim.get("/pia/alpha/issues/2").await.expect_status(StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(mail.to(&server, pim_address).len(), 1, "only the verification email: the mentions wait for the digest");

    // Not yet due: the period that started last began before the notifications.
    let now = Timestamp::now();
    assert_eq!(send_digests_at(&server, now).await, 0);

    let due = Timestamp(period_start(EmailDelivery::Daily, 9, now).0 + DAY);
    assert_eq!(send_digests_at(&server, due).await, 1);
    let sent = mail.wait_for(&server, pim_address, 2).await;
    let digest = &sent[1];
    assert_eq!(digest.subject(), "Your daily digest: 2 notifications in 2 repositories");
    let alpha = digest.text.find("\npia/alpha\n").expect("a section per repository");
    let beta = digest.text.find("\npia/beta\n").unwrap();
    assert!(alpha < beta, "{}", digest.text);
    assert!(digest.text.contains("Issue #1: Flaky build\n- pia opened issue #1.\n  @pim can you look?\n"), "{}", digest.text);
    assert!(digest.text.contains(&server.url("/pia/beta/issues/1")));
    assert!(!digest.text.contains("Read already"), "threads read on the site are left out");
    assert_eq!(digest.header("List-Unsubscribe-Post"), Some("List-Unsubscribe=One-Click"));
    let unsubscribe = digest.header("List-Unsubscribe").unwrap().trim_matches(['<', '>']).to_owned();
    assert!(unsubscribe.starts_with(&server.url("/email/unsubscribe/")), "{unsubscribe}");
    assert!(digest.text.contains(&unsubscribe));

    // The schedule runs again (as after a restart): nothing is sent twice.
    assert_eq!(send_digests_at(&server, due).await, 0);
    assert_eq!(send_digests_at(&server, Timestamp(due.0 + 60 * 60)).await, 0);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(mail.to(&server, pim_address).len(), 2);
}
