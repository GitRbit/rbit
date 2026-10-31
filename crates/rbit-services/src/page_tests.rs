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

async fn fixture() -> (tempfile::TempDir, Database, Services) {
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
    (directory, database, services)
}

async fn administrator(database: &Database) -> rbit_core::User {
    database
        .accounts()
        .create_user(rbit_storage::NewUser {
            name: "administrator".parse().unwrap(),
            email: "admin@example.com".parse().unwrap(),
            display_name: None,
            password_hash: "unused".into(),
            is_admin: true,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn page_registry_clones_share_publication_revocation_and_shell_revision() {
    let (_directory, _database, services) = fixture().await;
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

#[derive(Debug)]
struct PendingHook(Arc<tokio::sync::Notify>);

impl request_hooks::RequestHook for PendingHook {
    fn call<'a>(&'a self, _: &'a request_hooks::RequestMetadata) -> request_hooks::HookFuture<'a> {
        Box::pin(async move {
            self.0.notify_one();
            std::future::pending::<()>().await;
            request_hooks::HookDecision::Continue(Vec::new())
        })
    }
}

#[tokio::test]
async fn shutdown_preparation_is_terminal_cancels_extensions_and_keeps_core_open_until_drain() {
    let (_directory, database, services) = fixture().await;
    let admin = administrator(&database).await;
    #[cfg(feature = "actions")]
    services.set_actions_enabled(&admin, true).await.unwrap();
    #[cfg(feature = "i18n")]
    services.set_i18n_enabled(&admin, true).await.unwrap();
    #[cfg(feature = "actions")]
    let actions_lease = services.actions.snapshot().unwrap().admit().unwrap();

    let pages = services.page_registry();
    let hooks = services.request_hooks();
    let page_scope = PluginScope::new(Generation::INITIAL);
    let hook_scope = PluginScope::new(Generation::INITIAL);
    let page_started = Arc::new(tokio::sync::Notify::new());
    let ready = page_started.clone();
    let descriptor = PageDescriptor::new("pending", PageScope::Global, "pending", PageAudience::Public, Duration::from_secs(1)).unwrap();
    let pending = compiled_page(descriptor, move |_| {
        let ready = ready.clone();
        async move {
            ready.notify_one();
            std::future::pending::<()>().await;
            CompiledPage::new("Pending", Example { message: "unreachable" })
        }
    });
    let page_registration = pages.stage("example.pages".parse().unwrap(), &page_scope, page_scope.generation(), vec![pending]).unwrap();
    page_registration.publish().unwrap();
    let retained = pages.snapshot().find(&"example.pages".parse().unwrap(), PageScope::Global, "pending").unwrap();
    let mut page_lease = retained.admit().unwrap();
    let page_task = tokio::spawn(async move {
        page_lease.render(PageContext { actor: None, repository: None, locale: rbit_messages::LocaleSnapshot::english(0) }).await
    });
    let hook_started = Arc::new(tokio::sync::Notify::new());
    let make_hook = || {
        request_hooks::HookRegistration::new("pending", 0, Duration::from_millis(100), Arc::new(PendingHook(hook_started.clone()))).unwrap()
    };
    let hook_registration = hooks.stage("example.hooks".parse().unwrap(), &hook_scope, hook_scope.generation(), vec![make_hook()]).unwrap();
    hook_registration.publish().unwrap();
    let hook_snapshot = hooks.snapshot();
    let hook_task = tokio::spawn(async move {
        let context = request_hooks::RequestMetadata::new(request_hooks::RequestMethod::Get, "/example").unwrap();
        hook_snapshot.evaluate(&context).await
    });
    page_started.notified().await;
    hook_started.notified().await;

    services.prepare_shutdown();
    let revision = services.plugin_revision();
    services.clone().prepare_shutdown();
    assert_eq!(services.plugin_revision(), revision, "repeated preparation has no publication effect");
    assert!(pages.is_closed() && hooks.is_closed());
    assert!(services.page_registry().snapshot().is_empty());
    assert!(services.request_hooks().snapshot().is_empty());
    assert!(retained.admit().is_err());
    assert!(page_registration.publish().is_err());
    assert!(page_registration.replace(vec![page("replacement")]).is_err());
    assert!(hook_registration.publish().is_err());
    assert!(hook_registration.replace(vec![make_hook()]).is_err());
    let fresh = PluginScope::new(Generation::new(2).unwrap());
    assert!(pages.stage("example.fresh".parse().unwrap(), &fresh, fresh.generation(), vec![page("fresh")]).is_err());
    assert!(hooks.stage("example.fresh".parse().unwrap(), &fresh, fresh.generation(), vec![make_hook()]).is_err());
    assert!(matches!(page_task.await.unwrap(), Err(plugin_pages::PageError::Unavailable)));
    assert_eq!(hook_task.await.unwrap(), request_hooks::HookDecision::Reject(request_hooks::Rejection::Unavailable));
    assert_eq!((page_scope.in_flight(), hook_scope.in_flight()), (0, 0));
    let session = services.accounts.start_session(&admin).await.unwrap();
    assert!(services.accounts.resume_session(&session).await.unwrap().is_some(), "core and storage remain usable during HTTP drain");
    #[cfg(feature = "actions")]
    {
        assert!(services.actions.is_none());
        assert!(services.set_actions_enabled(&admin, true).await.is_err());
        assert_eq!(database.plugin_settings().desired_enabled("rbit.actions").await.unwrap(), Some(true));
    }
    #[cfg(feature = "i18n")]
    {
        assert!(!services.i18n_enabled());
        assert!(services.set_i18n_enabled(&admin, true).await.is_err());
        assert_eq!(database.plugin_settings().desired_enabled("rbit.i18n").await.unwrap(), Some(true));
    }
    let closing = services.clone();
    let shutdown = tokio::spawn(async move { closing.shutdown().await });
    #[cfg(feature = "actions")]
    let shutdown = {
        let mut shutdown = shutdown;
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut shutdown).await.is_err(),
            "final shutdown drains admitted built-in work"
        );
        drop(actions_lease);
        shutdown
    };
    tokio::time::timeout(Duration::from_secs(2), shutdown).await.unwrap().unwrap();
    #[cfg(feature = "actions")]
    assert_eq!(database.plugin_settings().desired_enabled("rbit.actions").await.unwrap(), Some(true));
    #[cfg(feature = "i18n")]
    assert_eq!(database.plugin_settings().desired_enabled("rbit.i18n").await.unwrap(), Some(true));
}

#[tokio::test]
async fn mutation_commit_survives_waiter_abort_and_blocks_global_shutdown_until_finished() {
    use plugin_mutations::{
        FormAction, FormData, FormSchema, MutationAudience, MutationContext, MutationDescriptor, MutationOutcome, PageActor,
        PluginRedirect, compiled_mutation,
    };
    let (_directory, database, services) = fixture().await;
    let admin = administrator(&database).await;
    let scope = PluginScope::new(Generation::INITIAL);
    let owner = "example.forms".parse().unwrap();
    let mutations = services.mutation_registry();
    let started = Arc::new(tokio::sync::Notify::new());
    let released = Arc::new(tokio::sync::Notify::new());
    let descriptor = MutationDescriptor::new(
        "save",
        PageScope::Global,
        "save",
        MutationAudience::SignedIn,
        Duration::from_secs(1),
        FormSchema::new(Vec::new()).unwrap(),
    )
    .unwrap();
    let commit_services = services.clone();
    let commit_actor = admin.clone();
    let ready = started.clone();
    let release = released.clone();
    let callback = compiled_mutation(descriptor, move |context, _| {
        let (ready, release, core, actor) = (ready.clone(), release.clone(), commit_services.clone(), commit_actor.clone());
        async move {
            ready.notify_one();
            context.cancellation_token().cancelled().await;
            release.notified().await;
            // The owned commit can still use domain services after its HTTP waiter ended.
            core.accounts.start_session(&actor).await.unwrap();
            Ok(MutationOutcome::<Example>::Redirect(PluginRedirect::new("edit").unwrap()))
        }
    });
    let before = services.plugin_revision();
    let registration = mutations.stage(owner, &scope, scope.generation(), vec![callback]).unwrap();
    assert_eq!(services.plugin_revision(), before);
    registration.publish().unwrap();
    assert!(services.plugin_revision() > before);
    let handle = mutations.snapshot().find(&"example.forms".parse().unwrap(), PageScope::Global, "save").unwrap();
    let lease = handle.admit().unwrap();
    let actor = PageActor { account_id: admin.id().0, is_admin: true };
    let context = MutationContext::new(
        actor,
        None,
        rbit_messages::LocaleSnapshot::english(0),
        FormAction::new("save", "/-/plugins/example.forms/save", "host-proof").unwrap(),
    );
    let request = tokio::spawn(async move { lease.execute(context, FormData::new(Vec::new()).unwrap()).await });
    started.notified().await;
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert_eq!(scope.in_flight(), 1);

    let closing = services.clone();
    let mut shutdown = tokio::spawn(async move { closing.shutdown().await });
    assert!(tokio::time::timeout(Duration::from_millis(20), &mut shutdown).await.is_err());
    assert!(mutations.is_closed());
    assert!(handle.admit().is_err());
    assert!(registration.publish().is_err());
    let session = services.accounts.start_session(&admin).await.unwrap();
    assert!(services.accounts.resume_session(&session).await.unwrap().is_some());
    released.notify_one();
    tokio::time::timeout(Duration::from_secs(2), shutdown).await.unwrap().unwrap();
    assert_eq!(scope.in_flight(), 0, "global shutdown waited for the actual commit worker");
}
