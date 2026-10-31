//! Installed administration and active presentation have independent lifetimes.

use super::*;
#[cfg(feature = "i18n")]
use crate::page_tests::administrator;
use crate::page_tests::{fixture, page};
#[cfg(feature = "i18n")]
use plugin_pages::PageContext;
use plugin_pages::{FormSchema, MutationAudience, MutationDescriptor, MutationOutcome, PageScope, PluginRedirect, compiled_mutation};
use std::time::Duration;

#[derive(Debug, askama::Template)]
#[template(source = "<p>Saved</p>", ext = "html")]
struct Saved;

fn bundle() -> I18nRouteBundle {
    let descriptor = MutationDescriptor::new(
        "save",
        PageScope::Global,
        "save",
        MutationAudience::SignedIn,
        Duration::from_secs(1),
        FormSchema::new(Vec::new()).unwrap(),
    )
    .unwrap();
    I18nRouteBundle {
        pages: vec![page("language")],
        mutations: vec![compiled_mutation(descriptor, |_, _| async {
            Ok(MutationOutcome::<Saved>::Redirect(PluginRedirect::new("example").unwrap()))
        })],
    }
}

fn page_handle(services: &Services, owner: &str) -> Option<plugin_pages::PageRouteHandle> {
    services.page_registry().snapshot().find(&owner.parse().unwrap(), PageScope::Global, "example")
}

#[cfg(feature = "i18n")]
#[tokio::test]
async fn i18n_routes_follow_activation_while_management_remains_installed() {
    let (_directory, database, services) = fixture().await;
    let admin = administrator(&database).await;
    services.bind_i18n_routes(bundle(), bundle()).await.unwrap();
    assert!(page_handle(&services, "rbit.i18n").is_none());
    let management = page_handle(&services, "rbit.i18n-admin").unwrap();
    services.set_i18n_enabled(&admin, true).await.unwrap();
    let current = page_handle(&services, "rbit.i18n").unwrap();
    assert_eq!(Some(current.generation()), services.i18n.status().unwrap().generation);
    let mutation = services.mutation_registry().snapshot().find(&"rbit.i18n".parse().unwrap(), PageScope::Global, "save").unwrap();
    services.set_i18n_enabled(&admin, false).await.unwrap();
    assert!(page_handle(&services, "rbit.i18n").is_none());
    assert!(current.admit().is_err());
    assert!(mutation.admit().is_err());
    assert!(management.admit().is_ok());
    services.set_i18n_enabled(&admin, true).await.unwrap();
    let next = page_handle(&services, "rbit.i18n").unwrap();
    let next_mutation = services.mutation_registry().snapshot().find(&"rbit.i18n".parse().unwrap(), PageScope::Global, "save").unwrap();
    assert_ne!(next.generation(), current.generation());
    assert_ne!(next_mutation.version(), mutation.version());
    services.prepare_shutdown();
    assert!(management.admit().is_err());
    assert!(next.admit().is_err());
    assert!(services.bind_i18n_routes(bundle(), bundle()).await.is_err());
    services.shutdown().await;
}

#[cfg(feature = "i18n")]
#[tokio::test]
async fn i18n_routes_attach_to_current_generation_and_management_can_disable_without_self_drain() {
    let (_directory, database, services) = fixture().await;
    let admin = administrator(&database).await;
    services.set_i18n_enabled(&admin, true).await.unwrap();
    let generation = services.i18n.status().unwrap().generation.unwrap();
    services.bind_i18n_routes(bundle(), bundle()).await.unwrap();
    let mut active = page_handle(&services, "rbit.i18n").unwrap().admit().unwrap();
    let body =
        active.render(PageContext { actor: None, repository: None, locale: services.locale_snapshot(None, None, None) }).await.unwrap();
    assert_eq!(body.html(), "<p>language</p>");
    drop(active);
    assert_eq!(page_handle(&services, "rbit.i18n").unwrap().generation(), generation);
    assert!(services.bind_i18n_routes(bundle(), bundle()).await.is_err());
    let admin_operation = page_handle(&services, "rbit.i18n-admin").unwrap().admit().unwrap();
    tokio::time::timeout(Duration::from_secs(2), services.set_i18n_enabled(&admin, false)).await.unwrap().unwrap();
    assert!(page_handle(&services, "rbit.i18n-admin").is_some());
    drop(admin_operation);
    services.shutdown().await;
}

#[cfg(not(feature = "i18n"))]
#[tokio::test]
async fn i18n_routes_never_publish_when_implementation_is_unavailable() {
    let (_directory, _database, services) = fixture().await;
    services.bind_i18n_routes(bundle(), bundle()).await.unwrap();
    assert!(page_handle(&services, "rbit.i18n").is_none());
    assert!(page_handle(&services, "rbit.i18n-admin").is_none());
    assert!(services.mutation_registry().snapshot().is_empty());
    services.prepare_shutdown();
    assert!(services.bind_i18n_routes(bundle(), bundle()).await.is_err());
    services.shutdown().await;
}

#[cfg(feature = "i18n")]
#[tokio::test]
async fn i18n_routes_keep_failed_provider_repair_and_retry_available() {
    use rbit_storage::sqlite::language_packs::NewLanguagePack;
    let (_directory, database, services) = fixture().await;
    let admin = administrator(&database).await;
    services.bind_i18n_routes(bundle(), bundle()).await.unwrap();
    database
        .language_packs()
        .put(&NewLanguagePack {
            id: "example.corrupt",
            locale: "fr",
            version: "1.0.0",
            bytes: b"bad",
            digest: &"0".repeat(64),
            message_count: 0,
            message_total: 1,
            installed_by: Some(admin.id()),
            updated_at: rbit_core::Timestamp::now(),
        })
        .await
        .unwrap();
    assert!(services.set_i18n_enabled(&admin, true).await.is_err());
    assert!(!services.i18n_enabled());
    assert!(page_handle(&services, "rbit.i18n").is_none());
    let management = page_handle(&services, "rbit.i18n-admin").unwrap().admit().unwrap();
    assert!(services.remove_language_pack(&admin, "example.corrupt").await.unwrap());
    services.set_i18n_enabled(&admin, false).await.unwrap();
    services.set_i18n_enabled(&admin, true).await.unwrap();
    assert!(page_handle(&services, "rbit.i18n").is_some());
    drop(management);
    services.shutdown().await;
}

#[cfg(feature = "i18n")]
#[tokio::test]
async fn i18n_routes_reject_partial_management_binding_without_leaking_or_consuming_the_one_shot() {
    let (_directory, _database, services) = fixture().await;
    let mut invalid = bundle();
    invalid.mutations.push(invalid.mutations[0].clone());
    assert!(services.bind_i18n_routes(bundle(), invalid).await.is_err());
    assert!(page_handle(&services, "rbit.i18n-admin").is_none());
    assert!(services.mutation_registry().snapshot().is_empty());
    services.bind_i18n_routes(bundle(), bundle()).await.unwrap();
    assert!(page_handle(&services, "rbit.i18n-admin").is_some());
    services.shutdown().await;
}
