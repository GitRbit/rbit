//! DNS rebinding: a name that resolves publicly when it is checked and internally when
//! rbit connects. HTTP clients classify the answer they connect with; Git checks every
//! answer and then connects only to what it checked (see `git.rs`).

use std::net::IpAddr;

use rbit_core::{
    Visibility,
    config::{ProviderConfig, ProviderKind},
    import::{ImportOptions, SourceKind},
    webhook::DeliveryStatus,
};

use super::{Fixture, LOOPBACK, PUBLIC, Reply, Server, name};
use crate::{
    ServiceError,
    imports::ImportInput,
    mirrors::CreateMirror,
    outbound::scripted,
    webhooks::{HookTarget, WebhookInput},
};

fn import(host: String) -> ImportInput {
    ImportInput {
        kind: SourceKind::Gitea,
        host,
        repository: "acme/rocket".to_owned(),
        token: String::new(),
        owner: "alice".to_owned(),
        name: String::new(),
        visibility: Visibility::Public,
        options: ImportOptions::default(),
    }
}

#[tokio::test]
async fn an_import_source_that_rebinds_after_its_check_is_refused_when_connecting() {
    let server = Server::always(Reply::status("200 OK", "", b"{}")).await;
    let host = name("import-rebind");
    let dns = scripted::serve(&host, &[&[PUBLIC], &[LOOPBACK]]);
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;

    let result = fixture.services.imports.start(&alice, import(format!("http://{host}:{}", server.address.port()))).await;
    let Err(ServiceError::Invalid(message)) = result else { panic!("expected a refusal, got {result:?}") };
    assert!(message.contains("internal addresses"), "{message}");
    assert!(dns.lookups() >= 2, "checked once, then resolved again when connecting: {}", dns.lookups());
    assert_eq!(server.connections(), 0, "the rebound address was never contacted");
    assert!(fixture.services.repositories.open(Some(&alice), "alice", "rocket").await.is_err(), "nothing was created");
}

#[tokio::test]
async fn one_internal_answer_refuses_a_git_host_entirely() {
    let server = Server::always(Reply::Hang).await;
    let host = name("mirror-mixed");
    let dns = scripted::serve(&host, &[&[PUBLIC, LOOPBACK]]);
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let mirror = CreateMirror {
        name: "mixed".to_owned(),
        description: String::new(),
        visibility: Visibility::Public,
        url: format!("http://{host}:{}/acme/rocket.git", server.address.port()),
        interval_minutes: None,
    };
    let created = fixture.services.mirrors.create(&alice, "alice", mirror).await;
    assert!(matches!(&created, Err(ServiceError::Invalid(message)) if message.contains("not a public address")), "{created:?}");
    assert_eq!((dns.lookups(), server.connections()), (1, 0));

    let import = fixture.services.imports.start(&alice, import(format!("http://{host}:{}", server.address.port()))).await;
    assert!(matches!(&import, Err(ServiceError::Invalid(message)) if message.contains("not a public address")), "{import:?}");
    assert_eq!((dns.lookups(), server.connections()), (2, 0));
}

#[tokio::test]
async fn webhook_names_are_classified_again_for_every_delivery() {
    let server = Server::always(Reply::status("200 OK", "", b"")).await;
    let host = name("hook-rebind");
    let mapped: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
    let dns = scripted::serve(&host, &[&[mapped], &[LOOPBACK, "::1".parse().unwrap()]]);
    let fixture = Fixture::new(|_| {}).await;
    let alice = fixture.user("alice").await;
    let repo = fixture.repo(&alice, "alice", "demo").await;
    let target = HookTarget::Repository(&repo);
    let input = WebhookInput {
        url: format!("http://{host}:{}/hook", server.address.port()),
        events: vec!["push".to_owned()],
        active: true,
        secret: rbit_storage::sqlite::webhooks::SecretUpdate::Keep,
    };
    // A name is accepted when saved: what it resolves to is decided per delivery.
    let hook = fixture.services.webhooks.create(&alice, target, input).await.unwrap();
    assert_eq!(dns.lookups(), 0);
    for attempt in 1..=2 {
        let ping = fixture.services.webhooks.ping(&alice, target, hook.id).await.unwrap();
        fixture.services.webhooks.deliver_due(rbit_core::Timestamp(rbit_core::Timestamp::now().seconds() + 86_400)).await.unwrap();
        let delivery = fixture.services.webhooks.delivery(target, hook.id, ping.id).await.unwrap();
        assert_eq!((delivery.status, delivery.attempts), (DeliveryStatus::Failed, 1), "refusals are not retried");
        assert!(delivery.error.as_deref().is_some_and(|error| error.contains("internal addresses")), "{:?}", delivery.error);
        assert!(dns.lookups() >= attempt, "resolved again for delivery {attempt}");
    }
    assert_eq!(server.connections(), 0);
}

#[tokio::test]
async fn sign_in_discovery_refuses_an_issuer_that_resolves_internally() {
    let server = Server::always(Reply::status("200 OK", "", b"{}")).await;
    let host = name("issuer-rebind");
    let dns = scripted::serve(&host, &[&[LOOPBACK]]);
    let issuer = format!("https://{host}:{}", server.address.port());
    let fixture = Fixture::new(|config| {
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
    let begun = fixture.services.external_auth.begin("corp", "/", None).await;
    assert!(matches!(begun, Err(ServiceError::Invalid(_))), "{begun:?}");
    assert!(dns.lookups() >= 1);
    assert_eq!(server.connections(), 0);
}
