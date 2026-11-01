//! Browser registration follows Actions service admission and its actual generation.

use super::*;
#[cfg(feature = "actions")]
use crate::page_tests::administrator;
use crate::page_tests::{fixture, page};
use plugin_pages::{FormSchema, MutationAudience, MutationDescriptor, MutationOutcome, PageScope, PluginRedirect, compiled_mutation};
use std::time::Duration;

#[derive(Debug, askama::Template)]
#[template(source = "<p>Saved</p>", ext = "html")]
struct Saved;

fn bundle() -> ActionsRouteBundle {
    let descriptor = MutationDescriptor::new(
        "save",
        PageScope::Global,
        "save",
        MutationAudience::SignedIn,
        Duration::from_secs(1),
        FormSchema::new(Vec::new()).unwrap(),
    )
    .unwrap();
    ActionsRouteBundle {
        pages: vec![page("Actions")],
        mutations: vec![compiled_mutation(descriptor, |_, _| async {
            Ok(MutationOutcome::<Saved>::Redirect(PluginRedirect::new("example").unwrap()))
        })],
    }
}

fn page_handle(services: &Services) -> Option<plugin_pages::PageRouteHandle> {
    services.page_registry().snapshot().find(&"rbit.actions".parse().unwrap(), PageScope::Global, "example")
}

#[cfg(feature = "actions")]
#[tokio::test]
async fn actions_routes_revoke_before_disable_drain_and_reenable_with_fresh_authority() {
    let (_directory, database, services) = fixture().await;
    let admin = administrator(&database).await;
    services.bind_actions_routes(bundle()).await.unwrap();
    assert!(page_handle(&services).is_none());
    services.set_actions_enabled(&admin, true).await.unwrap();
    let current = page_handle(&services).unwrap();
    assert_eq!(Some(current.generation()), services.actions_status().await.unwrap().generation);
    let mutation = services.mutation_registry().snapshot().find(&"rbit.actions".parse().unwrap(), PageScope::Global, "save").unwrap();
    let lease = current.admit().unwrap();
    let closing = services.clone();
    let actor = admin.clone();
    let disable = tokio::spawn(async move { closing.set_actions_enabled(&actor, false).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while services.actions.is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!disable.is_finished(), "the active generation drains its admitted route work");
    assert!(page_handle(&services).is_none());
    assert!(current.admit().is_err());
    assert!(mutation.admit().is_err());
    drop(lease);
    tokio::time::timeout(Duration::from_secs(2), disable).await.unwrap().unwrap().unwrap();
    services.set_actions_enabled(&admin, true).await.unwrap();
    let next = page_handle(&services).unwrap();
    let next_mutation = services.mutation_registry().snapshot().find(&"rbit.actions".parse().unwrap(), PageScope::Global, "save").unwrap();
    assert_ne!(current.generation(), next.generation());
    assert_ne!(mutation.version(), next_mutation.version());
    services.prepare_shutdown();
    assert!(next.admit().is_err());
    assert!(next_mutation.admit().is_err());
    assert!(services.bind_actions_routes(bundle()).await.is_err());
    services.shutdown().await;
}

#[cfg(feature = "actions")]
#[tokio::test]
async fn actions_routes_attach_to_existing_generation_and_reject_a_second_host_binding() {
    let (_directory, database, services) = fixture().await;
    let admin = administrator(&database).await;
    services.set_actions_enabled(&admin, true).await.unwrap();
    let generation = services.actions_status().await.unwrap().generation.unwrap();
    services.bind_actions_routes(bundle()).await.unwrap();
    assert_eq!(page_handle(&services).unwrap().generation(), generation);
    assert!(services.bind_actions_routes(bundle()).await.is_err());
    assert!(services.actions.is_some());
    assert_eq!(page_handle(&services).unwrap().generation(), generation);
    services.shutdown().await;
    assert!(page_handle(&services).is_none());
}

#[cfg(not(feature = "actions"))]
#[tokio::test]
async fn actions_routes_never_publish_when_the_implementation_is_unavailable() {
    let (_directory, _database, services) = fixture().await;
    services.bind_actions_routes(bundle()).await.unwrap();
    assert!(page_handle(&services).is_none());
    assert!(services.mutation_registry().snapshot().is_empty());
    assert!(services.bind_actions_routes(bundle()).await.is_err());
    services.prepare_shutdown();
    assert!(services.bind_actions_routes(bundle()).await.is_err());
    services.shutdown().await;
}

#[cfg(feature = "actions")]
#[tokio::test]
async fn actions_routes_stage_failure_leaves_the_service_unpublished_and_can_retry_after_capacity_recovers() {
    use plugin_pages::{CompiledPage, PageAudience, PageDescriptor, compiled_page};
    use rbit_plugin_api::Generation;
    use rbit_plugin_runtime::PluginScope;
    let (_directory, database, services) = fixture().await;
    let admin = administrator(&database).await;
    services.bind_actions_routes(bundle()).await.unwrap();
    let scope = PluginScope::new(Generation::INITIAL);
    let mut registrations = Vec::new();
    for group in 0..4 {
        let pages = (0..32)
            .map(|index| {
                let path = format!("page{index}");
                let descriptor =
                    PageDescriptor::new(&path, PageScope::Global, &path, PageAudience::Public, Duration::from_secs(1)).unwrap();
                compiled_page(descriptor, |_| async { CompiledPage::new("Capacity", Saved) })
            })
            .collect();
        let registration =
            services.page_registry().stage(format!("example.capacity{group}").parse().unwrap(), &scope, scope.generation(), pages).unwrap();
        registration.publish().unwrap();
        registrations.push(registration);
    }
    assert!(services.set_actions_enabled(&admin, true).await.is_err());
    assert!(services.actions.is_none());
    assert!(page_handle(&services).is_none());
    assert!(services.mutation_registry().snapshot().is_empty());
    assert!(services.ui_snapshot().entries.is_empty());
    for registration in registrations {
        registration.unpublish();
    }
    services.set_actions_enabled(&admin, false).await.unwrap();
    services.set_actions_enabled(&admin, true).await.unwrap();
    assert!(services.actions.is_some());
    assert!(page_handle(&services).is_some());
    services.shutdown().await;
}
