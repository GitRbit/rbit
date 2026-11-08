//! Redirects: webhook deliveries, sign-in providers and import API requests follow none;
//! import downloads follow a bounded number, each hop checked like the first, and
//! credentials never travel to another host.

use std::time::Duration;

use rbit_core::{
    Visibility,
    config::{ProviderConfig, ProviderKind},
    import::{ImportOptions, ImportSource, SourceKind},
    webhook::DeliveryStatus,
};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use super::{Fixture, LOOPBACK, Reply, Server, name};
use crate::{
    ServiceError,
    imports::{
        ImportInput,
        client::{MAX_DOWNLOAD_REDIRECTS, SourceClient, SourceError},
    },
    outbound::scripted,
};

fn source_client(server: &Server, allow_private: bool) -> SourceClient {
    let source = ImportSource::parse(SourceKind::Gitea, &server.url(""), "acme/rocket").unwrap();
    SourceClient::new(source, None, allow_private, Duration::from_secs(5), Duration::ZERO, CancellationToken::new(), true).unwrap()
}

async fn download(client: &SourceClient, url: &str, headers: &HeaderMap) -> Result<Vec<u8>, SourceError> {
    let mut body = Vec::new();
    client.download(url, headers, false).await?.read_to_end(&mut body).await.unwrap();
    Ok(body)
}

#[tokio::test]
async fn webhook_deliveries_record_a_redirect_without_following_it() {
    let elsewhere = Server::always(Reply::status("200 OK", "", b"")).await;
    let server = Server::always(Reply::redirect(&elsewhere.url("/hook"))).await;
    let fixture = Fixture::new(|config| config.webhooks.allow_private_addresses = true).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice, "alice", "demo").await;
    let delivery = fixture.deliver_ping(&alice, &repo, &server.url("/hook")).await;
    assert_eq!(delivery.response_status, Some(302));
    assert_ne!(delivery.status, DeliveryStatus::Succeeded, "a redirect is not a success");
    assert!(server.connections() >= 1);
    assert_eq!(elsewhere.connections(), 0, "no attempt followed the redirect");
}

#[tokio::test]
async fn sign_in_discovery_follows_no_redirect() {
    let elsewhere = Server::always(Reply::status("200 OK", "Content-Type: application/json\r\n", b"{}")).await;
    let server = Server::always(Reply::redirect(&elsewhere.url("/.well-known/openid-configuration"))).await;
    let issuer = server.url("");
    let fixture = Fixture::new(|config| {
        config.auth.allow_private_addresses = true;
        config.auth.providers = vec![ProviderConfig {
            kind: ProviderKind::Oidc,
            id: "corp".to_owned(),
            name: "Corp SSO".to_owned(),
            issuer: Some(issuer),
            client_id: "rbit".to_owned(),
            client_secret: "s3cret".to_owned(),
            ..ProviderConfig::default()
        }];
    })
    .await;
    assert!(matches!(fixture.services.external_auth.begin("corp", "/", None).await, Err(ServiceError::Invalid(_))));
    assert_eq!((server.connections(), elsewhere.connections()), (1, 0));
}

#[tokio::test]
async fn import_api_requests_follow_no_redirect() {
    let elsewhere = Server::always(Reply::status("200 OK", "", b"{}")).await;
    let target = elsewhere.url("");
    let server = Server::start(move |head| {
        let path = head.split_whitespace().nth(1).unwrap_or_default().to_owned();
        Reply::status("301 Moved Permanently", &format!("Location: {target}{path}\r\n"), b"")
    })
    .await;
    let fixture = Fixture::new(|config| config.imports.allow_private_addresses = true).await;
    let alice = fixture.user("alice").await;
    let input = ImportInput {
        kind: SourceKind::Gitea,
        host: server.url(""),
        repository: "acme/rocket".to_owned(),
        token: "secret-token".to_owned(),
        owner: "alice".to_owned(),
        name: String::new(),
        visibility: Visibility::Public,
        options: ImportOptions::default(),
    };
    assert!(matches!(fixture.services.imports.start(&alice, input).await, Err(ServiceError::Invalid(_))));
    assert_eq!(elsewhere.connections(), 0, "neither the request nor the token moved to another host");
}

#[tokio::test]
async fn import_downloads_follow_fewer_redirects_than_the_limit_and_only_to_http() {
    let server = Server::start(|head| {
        let path = head.split_whitespace().nth(1).unwrap_or_default();
        match path.strip_prefix("/hop/").and_then(|n| n.parse::<usize>().ok()) {
            Some(0) => Reply::status("200 OK", "", b"done"),
            Some(n) => Reply::redirect(&format!("/hop/{}", n - 1)),
            None if path == "/file" => Reply::redirect("file:///etc/passwd"),
            _ => Reply::redirect("ftp://example.com/x"),
        }
    })
    .await;
    let client = source_client(&server, true);
    let none = HeaderMap::new();
    // The limit counts the URLs visited: the redirect that would reach it fails.
    assert_eq!(download(&client, &server.url(&format!("/hop/{}", MAX_DOWNLOAD_REDIRECTS - 1)), &none).await.unwrap(), b"done");
    let too_many = download(&client, &server.url(&format!("/hop/{MAX_DOWNLOAD_REDIRECTS}")), &none).await;
    assert!(matches!(&too_many, Err(SourceError::Network(message)) if message.contains("too many redirects")), "{too_many:?}");
    assert_eq!(server.connections(), 2 * MAX_DOWNLOAD_REDIRECTS, "the longer chain stopped at the limit");
    for path in ["/file", "/ftp"] {
        let before = server.connections();
        assert!(download(&client, &server.url(path), &none).await.is_err(), "{path} is not followed");
        assert_eq!(server.connections(), before + 1);
    }
}

#[tokio::test]
async fn import_download_credentials_stay_on_their_host() {
    let elsewhere = Server::always(Reply::status("200 OK", "", b"moved")).await;
    let host = name("download-elsewhere");
    let _dns = scripted::serve(&host, &[&[LOOPBACK]]);
    let location = format!("http://{host}:{}/object", elsewhere.address.port());
    let server = Server::always(Reply::redirect(&location)).await;
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, HeaderValue::from_static("Basic c2VjcmV0"));
    assert_eq!(download(&source_client(&server, true), &server.url("/object"), &headers).await.unwrap(), b"moved");
    assert!(server.heads()[0].to_ascii_lowercase().contains("authorization: basic c2vjcmv0"));
    let moved = elsewhere.heads()[0].to_ascii_lowercase();
    assert!(!moved.contains("authorization") && !moved.contains("c2vjcmv0"), "{moved}");
}

#[tokio::test]
async fn strict_import_downloads_refuse_names_that_resolve_internally() {
    let server = Server::always(Reply::status("200 OK", "", b"")).await;
    let host = name("download-internal");
    let dns = scripted::serve(&host, &[&[LOOPBACK]]);
    let client = source_client(&server, false);
    let url = format!("http://{host}:{}/object", server.address.port());
    let refused = download(&client, &url, &HeaderMap::new()).await;
    assert!(matches!(&refused, Err(SourceError::Network(message)) if message.contains("internal addresses")), "{refused:?}");
    let literal = download(&client, &server.url("/object"), &HeaderMap::new()).await;
    assert!(matches!(&literal, Err(SourceError::Network(message)) if message.contains("not a public address")), "{literal:?}");
    assert!(dns.lookups() >= 1);
    assert_eq!(server.connections(), 0);
}
