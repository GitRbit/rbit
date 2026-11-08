//! Response size, decompression and time limits. A limit must stop the underlying work:
//! the client closes the connection instead of reading (or waiting) on behind the result.

use std::{
    io::Write,
    time::{Duration, Instant},
};

use rbit_core::{
    import::{ImportSource, SourceKind},
    webhook::{DeliveryId, WebhookEvent, WebhookId},
};
use rbit_storage::sqlite::webhooks::ClaimedDelivery;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::{Reply, Server};
use crate::{
    external_auth::client::{ProviderClient, ProviderError},
    imports::client::{MAX_API_RESPONSE_BYTES, SourceClient, SourceError},
    webhooks::delivery::{MAX_RESPONSE_BODY, attempt, build_client},
};

/// Far more than any limit; a client that kept reading would take many seconds.
const HUGE: u64 = 1 << 34;
const SETTLE: Duration = Duration::from_secs(5);

fn ok_head(extra: &str) -> String {
    format!("HTTP/1.1 200 OK\r\n{extra}Connection: close\r\n\r\n")
}

fn delivery(url: String) -> ClaimedDelivery {
    let guid = "00000000-0000-4000-8000-000000000000".to_owned();
    let (id, hook_id, event) = (DeliveryId(1), WebhookId(1), WebhookEvent::Ping);
    ClaimedDelivery { id, hook_id, guid, event, payload: "{}".to_owned(), attempts: 0, url, secret: None }
}

/// 8 MiB of zeros, gzip-compressed to a few kilobytes.
fn gzip_bomb() -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(&vec![0; 8 << 20]).unwrap();
    encoder.finish().unwrap()
}

fn source(server: &Server, timeout: Duration) -> SourceClient {
    let source = ImportSource::parse(SourceKind::Gitea, &server.url(""), "acme/rocket").unwrap();
    SourceClient::new(source, None, true, timeout, Duration::ZERO, CancellationToken::new(), true).unwrap()
}

#[tokio::test]
async fn webhook_responses_stop_at_the_log_limit() {
    let server = Server::always(Reply::Large(ok_head(""), HUGE)).await;
    let client = build_client(true, Duration::from_secs(60), true).unwrap();
    let started = Instant::now();
    let result = attempt(Some(&client), &delivery(server.url("/hook")), true, true).await;
    assert!(result.succeeded, "{:?}", result.record.error);
    assert_eq!(result.record.response_body.map(|body| body.len()), Some(MAX_RESPONSE_BODY));
    assert!(server.all_closed(SETTLE).await, "the rest of the body was not read");
    assert!(started.elapsed() < Duration::from_secs(30), "well before the time limit");
}

#[tokio::test]
async fn webhook_time_limits_close_the_connection() {
    let server = Server::always(Reply::Hang).await;
    let client = build_client(true, Duration::from_secs(1), true).unwrap();
    let started = Instant::now();
    let result = attempt(Some(&client), &delivery(server.url("/hook")), true, true).await;
    assert!(result.record.error.as_deref().is_some_and(|error| error.contains("timed out")), "{:?}", result.record.error);
    assert!(!result.permanent, "a slow receiver may answer next time");
    assert!(started.elapsed() < SETTLE);
    assert!(server.all_closed(SETTLE).await);
}

#[tokio::test]
async fn compressed_responses_are_never_inflated() {
    let bomb = gzip_bomb();
    let server = Server::always(Reply::status("200 OK", "Content-Encoding: gzip\r\nContent-Type: application/json\r\n", &bomb)).await;
    let client = build_client(true, Duration::from_secs(10), true).unwrap();
    let result = attempt(Some(&client), &delivery(server.url("/hook")), true, true).await;
    let body = result.record.response_body.unwrap();
    assert!(body.chars().count() <= bomb.len(), "the receiver's bytes are kept as sent: {} chars", body.chars().count());

    let providers = ProviderClient::new(true, Duration::from_secs(10), true).unwrap();
    let refused = providers.get_json(&server.url("/.well-known/openid-configuration"), None, &[]).await;
    assert!(matches!(&refused, Err(ProviderError::Response { message, .. }) if message.contains("not JSON")), "{refused:?}");
    let imported: Result<Value, _> = source(&server, Duration::from_secs(10)).get(&server.url("/api/v1/repos/acme/rocket")).await;
    assert!(matches!(imported, Err(SourceError::Invalid(_))), "{imported:?}");
    for head in server.heads() {
        assert!(!head.to_ascii_lowercase().contains("accept-encoding"), "no compression is offered: {head}");
    }
}

#[tokio::test]
async fn provider_responses_stop_at_their_limit_and_time() {
    let large = Server::always(Reply::Large(ok_head("Content-Type: application/json\r\n"), HUGE)).await;
    let providers = ProviderClient::new(true, Duration::from_secs(60), true).unwrap();
    let refused = providers.get_json(&large.url("/keys"), None, &[]).await;
    assert!(matches!(&refused, Err(ProviderError::Response { message, .. }) if message.contains("too large")), "{refused:?}");
    assert!(large.all_closed(SETTLE).await);
    assert!(large.sent() < HUGE);

    let stalled = Server::always(Reply::Hang).await;
    let quick = ProviderClient::new(true, Duration::from_secs(1), true).unwrap();
    let started = Instant::now();
    let timed_out = quick.post_form(&stalled.url("/token"), &[("grant_type", "authorization_code")], None).await;
    assert!(matches!(&timed_out, Err(ProviderError::Http { message, .. }) if message.contains("timed out")), "{timed_out:?}");
    assert!(started.elapsed() < SETTLE);
    assert!(stalled.all_closed(SETTLE).await);
}

#[tokio::test]
async fn import_api_responses_stop_at_their_limit() {
    // A little more than the limit, then silence: a reader without a limit would read it
    // all and then wait for the end of the body until the time limit.
    let past_limit = MAX_API_RESPONSE_BYTES as u64 + (1 << 20);
    let timeout = Duration::from_secs(20);
    let server = Server::always(Reply::Large(ok_head("Content-Type: application/json\r\n"), past_limit)).await;
    let started = Instant::now();
    let refused: Result<Value, _> = source(&server, timeout).get(&server.url("/api/v1/repos/acme/rocket")).await;
    assert!(matches!(&refused, Err(SourceError::Invalid(message)) if message.contains("larger than")), "{refused:?}");
    assert!(started.elapsed() < timeout / 2, "stopped at the limit, not the time limit: {:?}", started.elapsed());
    assert!(server.all_closed(SETTLE).await);

    // Error bodies are read only as far as their summary needs.
    let head = "HTTP/1.1 418 I'm a teapot\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n".to_owned();
    let failing = Server::always(Reply::Large(head, past_limit)).await;
    let started = Instant::now();
    let failed: Result<Value, _> = source(&failing, timeout).get(&failing.url("/api/v1/repos/acme/rocket")).await;
    assert!(matches!(&failed, Err(SourceError::Status { status: 418, .. })), "{failed:?}");
    assert!(started.elapsed() < timeout / 2, "{:?}", started.elapsed());
    assert!(failing.all_closed(SETTLE).await);
}

#[tokio::test]
async fn stopping_an_import_stops_a_response_being_read() {
    let server = Server::always(Reply::Large(ok_head("Content-Type: application/json\r\n"), 1 << 20)).await;
    let cancel = CancellationToken::new();
    let source = ImportSource::parse(SourceKind::Gitea, &server.url(""), "acme/rocket").unwrap();
    let client = SourceClient::new(source, None, true, Duration::from_secs(60), Duration::ZERO, cancel.clone(), true).unwrap();
    let stop = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        cancel.cancel();
    });
    let started = Instant::now();
    let stopped: Result<Value, _> = client.get(&server.url("/api/v1/repos/acme/rocket")).await;
    assert!(matches!(stopped, Err(SourceError::Cancelled)), "{stopped:?}");
    assert!(started.elapsed() < SETTLE);
    assert!(server.all_closed(SETTLE).await);
    stop.await.unwrap();
}
