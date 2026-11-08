//! Internal, loopback, link-local and metadata addresses in every spelling a URL parser
//! accepts are refused by every outbound entry point under the strict policy, before
//! anything is contacted.

use std::time::Duration;

use rbit_core::{Visibility, import::SourceKind};

use super::{Fixture, Reply, Server};
use crate::{
    ServiceError,
    external_auth::client::ProviderClient,
    imports::ImportInput,
    mirrors::CreateMirror,
    outbound::{check_public_host, refuses_literal},
    push_mirrors::PushMirrorInput,
    webhooks::{HookTarget, WebhookInput},
};

/// Spellings of internal addresses, as hosts (`{port}` is the test listener's port).
const INTERNAL_HOSTS: [&str; 22] = [
    "127.0.0.1",
    "2130706433",
    "0x7f000001",
    "0x7f.1",
    "127.1",
    "0177.0.0.1",
    "127.000.000.001",
    "127.0.0.1.",
    "\u{ff11}\u{ff12}\u{ff17}.\u{ff10}.\u{ff10}.\u{ff11}",
    "0",
    "[::]",
    "[::1]",
    "[::ffff:127.0.0.1]",
    "[::ffff:7f00:1]",
    "[::127.0.0.1]",
    "[64:ff9b::7f00:1]",
    "[2002:7f00:1::]",
    "localhost",
    "LOCALHOST",
    "localhost.",
    "Rbit.LocalHost",
    "\u{ff4c}\u{ff4f}\u{ff43}\u{ff41}\u{ff4c}\u{ff48}\u{ff4f}\u{ff53}\u{ff54}",
];

/// Cloud metadata and link-local spellings (nothing listens there; nothing may try).
const METADATA_HOSTS: [&str; 9] = [
    "169.254.169.254",
    "2852039166",
    "0xa9fea9fe",
    "0251.0376.0251.0376",
    "169.254.169.254.",
    "[::ffff:169.254.169.254]",
    "[::ffff:a9fe:a9fe]",
    "[fe80::a9fe:a9fe]",
    "[fd00:ec2::254]",
];

/// Address spellings, not names (names are checked when connecting).
fn is_literal(host: &str) -> bool {
    !host.to_ascii_lowercase().contains("local") && !host.contains('\u{ff4c}')
}

#[tokio::test]
async fn internal_hosts_are_refused_unless_allowed() {
    assert!(check_public_host("127.0.0.1", 443, false).await.is_err());
    assert!(check_public_host("::1", 443, false).await.is_err());
    assert!(check_public_host("10.1.2.3", 443, false).await.is_err());
    assert!(check_public_host("127.0.0.1", 443, true).await.is_ok());
    assert!(check_public_host("93.184.216.34", 443, false).await.is_ok());
    for host in ["2130706433", "0x7f000001", "127.1", "0177.0.0.1"] {
        assert!(check_public_host(host, 443, false).await.is_err(), "{host}");
        assert!(check_public_host(host, 443, true).await.is_ok(), "{host}");
    }
}

#[tokio::test]
async fn git_hosts_are_classified_by_their_canonical_address() {
    for host in INTERNAL_HOSTS.iter().chain(&METADATA_HOSTS) {
        let bare = host.trim_start_matches('[').trim_end_matches(']');
        assert!(check_public_host(bare, 443, false).await.is_err(), "{host}");
    }
    let pinned = check_public_host("\u{ff11}\u{ff12}\u{ff17}.\u{ff10}.\u{ff10}.\u{ff11}", 443, true).await.unwrap();
    assert!(format!("{pinned:?}").contains("host: \"127.0.0.1\""), "pinned by its canonical form: {pinned:?}");
}

#[test]
fn literals_are_refused_in_every_spelling() {
    for host in INTERNAL_HOSTS.iter().chain(&METADATA_HOSTS).filter(|host| is_literal(host)) {
        let url = reqwest::Url::parse(&format!("http://{host}/")).unwrap();
        assert!(refuses_literal(&url, false), "{host} -> {url}");
        assert!(!refuses_literal(&url, true), "{host} is allowed with private addresses");
    }
    for public in ["93.184.215.14", "1572395022", "[2606:4700::1111]", "[::ffff:93.184.215.14]", "sso.example.com"] {
        let url = reqwest::Url::parse(&format!("https://{public}/")).unwrap();
        assert!(!refuses_literal(&url, false), "{public} is public (or a name, checked when connecting)");
    }
}

#[tokio::test]
async fn provider_endpoints_refuse_internal_spellings_and_plain_http() {
    let strict = ProviderClient::new(false, Duration::from_secs(2), true).unwrap();
    for host in INTERNAL_HOSTS.iter().chain(&METADATA_HOSTS).filter(|host| is_literal(host)) {
        assert!(strict.endpoint(&format!("https://{host}/.well-known/openid-configuration")).is_err(), "{host}");
    }
    assert!(strict.endpoint("http://sso.example.com/token").is_err(), "plain http needs private addresses allowed");
    assert!(strict.endpoint("file:///etc/passwd").is_err());
    assert!(strict.endpoint("https://SSO.Example.COM./token").is_ok(), "names are checked when connecting");
}

#[tokio::test]
async fn every_entry_point_refuses_internal_spellings_without_contacting_them() {
    let server = Server::always(Reply::status("200 OK", "", b"{}")).await;
    let port = server.address.port();
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice, "alice", "demo").await;
    let services = &fixture.services;
    for host in INTERNAL_HOSTS.iter().map(|host| format!("{host}:{port}")).chain(METADATA_HOSTS.iter().map(|host| (*host).to_owned())) {
        let base = format!("http://{host}");
        let webhook = WebhookInput {
            url: format!("{base}/hook"),
            events: vec!["push".to_owned()],
            active: true,
            secret: rbit_storage::sqlite::webhooks::SecretUpdate::Keep,
        };
        let hook = services.webhooks.create(&alice, HookTarget::Repository(&repo), webhook).await;
        assert!(matches!(hook, Err(ServiceError::Invalid(_))), "webhook {host}: {hook:?}");

        let mirror = CreateMirror {
            name: "mirror".to_owned(),
            description: String::new(),
            visibility: Visibility::Public,
            url: format!("{base}/acme/rocket.git"),
            interval_minutes: None,
        };
        let created = services.mirrors.create(&alice, "alice", mirror).await;
        assert!(matches!(created, Err(ServiceError::Invalid(_))), "mirror {host}: {created:?}");

        let push = PushMirrorInput { url: format!("{base}/acme/rocket.git"), enabled: true, ..PushMirrorInput::default() };
        let pushed = services.push_mirrors.create(&repo, &alice, &push).await;
        assert!(matches!(pushed, Err(ServiceError::Invalid(_))), "push mirror {host}: {pushed:?}");

        let import = ImportInput {
            kind: SourceKind::Gitea,
            host: base.clone(),
            repository: "acme/rocket".to_owned(),
            token: String::new(),
            owner: "alice".to_owned(),
            name: String::new(),
            visibility: Visibility::Public,
            options: rbit_core::import::ImportOptions::default(),
        };
        let imported = services.imports.start(&alice, import).await;
        assert!(matches!(imported, Err(ServiceError::Invalid(_))), "import {host}: {imported:?}");
    }
    assert_eq!(server.connections(), 0, "nothing reached the internal listener");
    for name in ["mirror", "rocket"] {
        assert!(services.repositories.open(Some(&alice), "alice", name).await.is_err(), "no {name} repository was created");
    }
}
