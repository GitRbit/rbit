//! The composition root shares page publication and cache identity across clones.

use super::*;
use plugin_pages::{CompiledPage, PageAudience, PageContext, PageDescriptor, PageScope, compiled_page};
use rbit_plugin_api::Generation;
use rbit_plugin_runtime::PluginScope;
use std::time::Duration;

#[derive(Debug, askama::Template)]
#[template(source = "<p>{{ message }}</p>", ext = "html")]
struct Example {
    message: &'static str,
}

fn page(message: &'static str) -> plugin_pages::PageRegistration {
    let descriptor = PageDescriptor::new("example", PageScope::Global, "example", PageAudience::Public, Duration::from_secs(1)).unwrap();
    compiled_page(descriptor, move |_| async move { CompiledPage::new("Example", Example { message }) })
}

#[tokio::test]
async fn page_registry_clones_share_publication_revocation_and_shell_revision() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = directory.path().join("data");
    config.updates.check_for_updates = false;
    config.actions.enabled = false;
    config.i18n.enabled = false;
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let cloned = services.clone();
    let scope = PluginScope::new(Generation::INITIAL);
    let owner = "example.pages".parse().unwrap();
    let initial_revision = services.plugin_revision();
    let registration = cloned.page_registry().stage(owner, &scope, scope.generation(), vec![page("<original>")]).unwrap();
    assert!(services.page_registry().snapshot().is_empty());
    assert_eq!(services.plugin_revision(), initial_revision, "preparation does not invalidate the published shell");

    registration.publish().unwrap();
    let published_revision = services.plugin_revision();
    assert!(published_revision > initial_revision);
    let lookup = || services.page_registry().snapshot().find(&"example.pages".parse().unwrap(), PageScope::Global, "example").unwrap();
    let old = lookup();
    registration.replace(vec![page("<replacement>")]).unwrap();
    assert!(old.admit().is_err(), "old lookup handles cannot enter a replacement batch");
    let current = lookup();
    let mut lease = current.admit().unwrap();
    let output =
        lease.render(PageContext { actor: None, repository: None, locale: rbit_messages::LocaleSnapshot::english(0) }).await.unwrap();
    assert_eq!(output.html(), "<p>&#60;replacement&#62;</p>");
    drop(lease);
    assert!(services.plugin_revision() > published_revision);

    let before_close = services.plugin_revision();
    scope.quiesce();
    assert!(cloned.page_registry().snapshot().is_empty());
    assert!(current.admit().is_err());
    assert!(services.plugin_revision() > before_close, "route-only cleanup changes the shell identity");
    services.shutdown().await;
}
