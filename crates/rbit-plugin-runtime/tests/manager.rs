//! Embedded controller behavior under dependency loss and interrupted lifecycle work.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rbit_plugin_api::{DesiredState, PluginId, PluginManifest, PluginState, ServiceDependency, ServiceProvided};
use rbit_plugin_runtime::{AdmissionError, ManagerError, Plugin, PluginFuture, PluginManager, PluginScope};
use semver::Version;
use tokio::sync::Notify;

type Events = Arc<Mutex<Vec<String>>>;

#[derive(Debug)]
struct TestPlugin {
    manifest: PluginManifest,
    events: Events,
    fail_start: Arc<AtomicBool>,
    fail_stop: Arc<AtomicBool>,
    close_on_start: bool,
    task_finish: Option<Arc<Notify>>,
    start_finish: Option<Arc<Notify>>,
    stop_finish: Option<Arc<Notify>>,
}

impl TestPlugin {
    fn new(id: &str, events: &Events) -> Self {
        Self {
            manifest: PluginManifest {
                id: id.parse().unwrap(),
                version: Version::new(1, 0, 0),
                host_api: "^1".parse().unwrap(),
                requires: Vec::new(),
                provides: Vec::new(),
                capabilities: Vec::new(),
            },
            events: Arc::clone(events),
            fail_start: Arc::default(),
            fail_stop: Arc::default(),
            close_on_start: false,
            task_finish: None,
            start_finish: None,
            stop_finish: None,
        }
    }

    fn provides(mut self, service: &str) -> Self {
        self.manifest.provides.push(ServiceProvided { id: service.parse().unwrap(), version: Version::new(1, 0, 0) });
        self
    }

    fn requires(mut self, service: &str) -> Self {
        self.manifest.requires.push(ServiceDependency { id: service.parse().unwrap(), version: "^1".parse().unwrap() });
        self
    }
}

impl Plugin for TestPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn start(&self, scope: PluginScope) -> PluginFuture {
        let events = Arc::clone(&self.events);
        let id = self.manifest.id.clone();
        let fail = Arc::clone(&self.fail_start);
        let start_finish = self.start_finish.clone();
        let task_finish = self.task_finish.clone();
        let close_on_start = self.close_on_start;
        Box::pin(async move {
            events.lock().unwrap().push(format!("start:{id}"));
            if let Some(finish) = start_finish {
                finish.notified().await;
            }
            if let Some(finish) = task_finish {
                drop(scope.spawn(|_| async move { finish.notified().await }).unwrap());
            }
            if close_on_start {
                scope.quiesce();
            }
            if fail.load(Ordering::SeqCst) { Err("startup rejected".into()) } else { Ok(()) }
        })
    }

    fn stop(&self) -> PluginFuture {
        let events = Arc::clone(&self.events);
        let id = self.manifest.id.clone();
        let finish = self.stop_finish.clone();
        let fail = Arc::clone(&self.fail_stop);
        Box::pin(async move {
            events.lock().unwrap().push(format!("stop:{id}"));
            if let Some(finish) = finish {
                finish.notified().await;
            }
            if fail.load(Ordering::SeqCst) { Err("cleanup rejected".into()) } else { Ok(()) }
        })
    }
}

fn id(value: &str) -> PluginId {
    value.parse().unwrap()
}
fn manager() -> PluginManager {
    PluginManager::new(Version::new(1, 0, 0), Duration::from_secs(1))
}

#[test]
fn installation_validates_without_starting_any_callbacks() {
    let events = Events::default();
    let plugin = Arc::new(TestPlugin::new("example.plugin", &events));
    let mut manager = manager();
    manager.install(plugin.clone()).unwrap();
    assert!(events.lock().unwrap().is_empty());
    assert_eq!(manager.status(&id("example.plugin")).unwrap().state, PluginState::Installed);
    assert!(matches!(manager.install(plugin), Err(ManagerError::AlreadyInstalled(_))));
    let mut incompatible = TestPlugin::new("example.incompatible", &events);
    incompatible.manifest.host_api = "^2".parse().unwrap();
    assert!(matches!(manager.install(Arc::new(incompatible)), Err(ManagerError::Manifest(_))));
    assert_eq!(manager.statuses().len(), 1);
}

#[tokio::test]
async fn dependencies_wait_activate_and_unload_in_reverse_order() {
    let events = Events::default();
    let mut manager = manager();
    let provider = id("example.provider");
    let consumer = id("example.consumer");
    manager.install(Arc::new(TestPlugin::new(consumer.as_str(), &events).requires("example.service"))).unwrap();
    manager.enable(&consumer).await.unwrap();
    assert_eq!(manager.status(&consumer).unwrap().state, PluginState::Waiting);
    manager.install(Arc::new(TestPlugin::new(provider.as_str(), &events).provides("example.service"))).unwrap();
    manager.enable(&provider).await.unwrap();
    assert_eq!(*events.lock().unwrap(), ["start:example.provider", "start:example.consumer"]);
    manager.disable(&provider).await.unwrap();
    assert_eq!(
        *events.lock().unwrap(),
        ["start:example.provider", "start:example.consumer", "stop:example.consumer", "stop:example.provider"]
    );
    assert_eq!(manager.status(&consumer).unwrap().desired, DesiredState::Enabled);
    assert_eq!(manager.status(&consumer).unwrap().state, PluginState::Waiting);
    manager.enable(&provider).await.unwrap();
    assert_eq!(manager.status(&consumer).unwrap().state, PluginState::Active);
    manager.disable(&consumer).await.unwrap();
    manager.reload(&provider).await.unwrap();
    assert_eq!(manager.status(&consumer).unwrap().state, PluginState::Disabled);
    assert_eq!(manager.status(&consumer).unwrap().desired, DesiredState::Disabled);
}

#[tokio::test]
async fn reloading_retires_consumers_and_rejects_stale_handles() {
    let events = Events::default();
    let mut manager = manager();
    let provider = id("example.provider");
    let consumer = id("example.consumer");
    manager.install(Arc::new(TestPlugin::new(provider.as_str(), &events).provides("example.service"))).unwrap();
    manager.install(Arc::new(TestPlugin::new(consumer.as_str(), &events).requires("example.service"))).unwrap();
    manager.enable(&provider).await.unwrap();
    manager.enable(&consumer).await.unwrap();
    let old = manager.scope(&provider).unwrap();
    manager.reload(&provider).await.unwrap();
    let current = manager.scope(&provider).unwrap();
    assert!(current.generation() > old.generation());
    assert!(matches!(old.try_acquire(old.generation()), Err(AdmissionError::Closed)));
    assert!(matches!(current.try_acquire(old.generation()), Err(AdmissionError::GenerationMismatch { .. })));
    assert_eq!(
        *events.lock().unwrap(),
        [
            "start:example.provider",
            "start:example.consumer",
            "stop:example.consumer",
            "stop:example.provider",
            "start:example.provider",
            "start:example.consumer"
        ]
    );
}

#[tokio::test]
async fn failed_activation_is_cleaned_and_explicitly_retryable() {
    let events = Events::default();
    let mut manager = manager();
    let plugin = TestPlugin::new("example.plugin", &events);
    let fail = Arc::clone(&plugin.fail_start);
    fail.store(true, Ordering::SeqCst);
    manager.install(Arc::new(plugin)).unwrap();
    let plugin = id("example.plugin");
    assert!(matches!(manager.enable(&plugin).await, Err(ManagerError::Callback { phase: "start", .. })));
    let status = manager.status(&plugin).unwrap();
    assert_eq!(status.state, PluginState::Failed);
    assert!(status.generation.is_none());
    assert_eq!(status.in_flight, 0);
    fail.store(false, Ordering::SeqCst);
    manager.enable(&plugin).await.unwrap();
    assert_eq!(manager.status(&plugin).unwrap().state, PluginState::Active);
}

#[tokio::test]
async fn timeout_retains_surviving_work_until_disable_can_finish() {
    let events = Events::default();
    let mut manager = PluginManager::new(Version::new(1, 0, 0), Duration::from_millis(5));
    let finish = Arc::new(Notify::new());
    let mut plugin = TestPlugin::new("example.plugin", &events);
    plugin.task_finish = Some(Arc::clone(&finish));
    manager.install(Arc::new(plugin)).unwrap();
    let plugin = id("example.plugin");
    manager.enable(&plugin).await.unwrap();
    assert!(matches!(manager.disable(&plugin).await, Err(ManagerError::Drain { .. })));
    assert_eq!(manager.status(&plugin).unwrap().state, PluginState::RestartRequired);
    assert_eq!(manager.status(&plugin).unwrap().in_flight, 1);
    assert!(matches!(manager.enable(&plugin).await, Err(ManagerError::Incomplete { .. })));
    finish.notify_one();
    manager.disable(&plugin).await.unwrap();
    assert_eq!(manager.status(&plugin).unwrap().state, PluginState::Disabled);
    assert!(manager.status(&plugin).unwrap().generation.is_none());
    assert_eq!(events.lock().unwrap().iter().filter(|event| event.starts_with("stop:")).count(), 1);
}

#[tokio::test]
async fn cancelled_activation_is_retained_and_quiesced_for_cleanup() {
    let events = Events::default();
    let mut manager = manager();
    let mut plugin = TestPlugin::new("example.plugin", &events);
    plugin.start_finish = Some(Arc::new(Notify::new()));
    manager.install(Arc::new(plugin)).unwrap();
    let plugin = id("example.plugin");
    assert!(tokio::time::timeout(Duration::ZERO, manager.enable(&plugin)).await.is_err());
    let status = manager.status(&plugin).unwrap();
    assert_eq!(status.state, PluginState::RestartRequired);
    assert!(status.generation.is_some());
    assert!(matches!(manager.enable(&plugin).await, Err(ManagerError::Incomplete { .. })));
    manager.disable(&plugin).await.unwrap();
    assert_eq!(manager.status(&plugin).unwrap().state, PluginState::Disabled);
}

#[tokio::test]
async fn cancelled_disable_resumes_the_same_stop_callback() {
    let events = Events::default();
    let mut manager = manager();
    let finish = Arc::new(Notify::new());
    let mut plugin = TestPlugin::new("example.plugin", &events);
    plugin.stop_finish = Some(Arc::clone(&finish));
    manager.install(Arc::new(plugin)).unwrap();
    let plugin = id("example.plugin");
    manager.enable(&plugin).await.unwrap();
    assert!(tokio::time::timeout(Duration::ZERO, manager.disable(&plugin)).await.is_err());
    assert_eq!(manager.status(&plugin).unwrap().state, PluginState::Quiescing);
    finish.notify_one();
    manager.disable(&plugin).await.unwrap();
    assert_eq!(manager.status(&plugin).unwrap().state, PluginState::Disabled);
    assert_eq!(events.lock().unwrap().iter().filter(|event| event.starts_with("stop:")).count(), 1);
}

#[tokio::test]
async fn graph_rejection_restores_administrative_intent() {
    let events = Events::default();
    let mut manager = manager();
    for name in ["example.first", "example.second"] {
        manager.install(Arc::new(TestPlugin::new(name, &events).provides("example.service"))).unwrap();
    }
    manager.enable(&id("example.first")).await.unwrap();
    assert!(matches!(manager.enable(&id("example.second")).await, Err(ManagerError::Graph(_))));
    let status = manager.status(&id("example.second")).unwrap();
    assert_eq!(status.desired, DesiredState::Disabled);
    assert_eq!(status.state, PluginState::Installed);
    assert_eq!(*events.lock().unwrap(), ["start:example.first"]);
}

#[tokio::test]
async fn uninstall_does_not_forget_incomplete_cleanup() {
    let events = Events::default();
    let mut manager = PluginManager::new(Version::new(1, 0, 0), Duration::ZERO);
    let finish = Arc::new(Notify::new());
    let mut plugin = TestPlugin::new("example.plugin", &events);
    plugin.task_finish = Some(Arc::clone(&finish));
    manager.install(Arc::new(plugin)).unwrap();
    let plugin = id("example.plugin");
    manager.enable(&plugin).await.unwrap();
    assert!(manager.uninstall(&plugin).await.is_err());
    assert!(manager.status(&plugin).is_some());
    finish.notify_one();
    tokio::task::yield_now().await;
    manager.uninstall(&plugin).await.unwrap();
    assert!(manager.status(&plugin).is_none());
}

#[tokio::test]
async fn provider_admission_closes_even_if_consumer_cleanup_times_out() {
    let events = Events::default();
    let mut manager = PluginManager::new(Version::new(1, 0, 0), Duration::ZERO);
    let finish = Arc::new(Notify::new());
    let provider = id("example.provider");
    let consumer = id("example.consumer");
    manager.install(Arc::new(TestPlugin::new(provider.as_str(), &events).provides("example.service"))).unwrap();
    let mut plugin = TestPlugin::new(consumer.as_str(), &events).requires("example.service");
    plugin.task_finish = Some(Arc::clone(&finish));
    manager.install(Arc::new(plugin)).unwrap();
    manager.enable(&provider).await.unwrap();
    manager.enable(&consumer).await.unwrap();
    let provider_handle = manager.scope(&provider).unwrap();
    assert!(manager.disable(&provider).await.is_err());
    assert!(provider_handle.is_closed());
    assert!(matches!(provider_handle.try_acquire(provider_handle.generation()), Err(AdmissionError::Closed)));
    finish.notify_one();
    tokio::task::yield_now().await;
    manager.disable(&provider).await.unwrap();
    assert_eq!(manager.status(&consumer).unwrap().state, PluginState::Waiting);
}

#[tokio::test]
async fn controller_drop_revokes_retained_handles_and_recreation_uses_new_generation() {
    let events = Events::default();
    let plugin = id("example.plugin");
    let mut first = manager();
    first.install(Arc::new(TestPlugin::new(plugin.as_str(), &events))).unwrap();
    first.enable(&plugin).await.unwrap();
    let retained = first.scope(&plugin).unwrap();
    drop(first);
    assert!(retained.is_closed());
    let mut second = manager();
    second.install(Arc::new(TestPlugin::new(plugin.as_str(), &events))).unwrap();
    second.enable(&plugin).await.unwrap();
    let current = second.scope(&plugin).unwrap();
    assert!(current.generation() > retained.generation());
    assert!(matches!(current.try_acquire(retained.generation()), Err(AdmissionError::GenerationMismatch { .. })));
}

#[tokio::test]
async fn closed_startup_scope_is_never_published_as_active() {
    let events = Events::default();
    let mut manager = manager();
    let provider = id("example.provider");
    let consumer = id("example.consumer");
    let mut plugin = TestPlugin::new(provider.as_str(), &events).provides("example.service");
    plugin.close_on_start = true;
    manager.install(Arc::new(plugin)).unwrap();
    manager.install(Arc::new(TestPlugin::new(consumer.as_str(), &events).requires("example.service"))).unwrap();
    manager.enable(&consumer).await.unwrap();
    assert!(matches!(manager.enable(&provider).await, Err(ManagerError::Callback { phase: "start", .. })));
    assert_eq!(manager.status(&provider).unwrap().state, PluginState::Failed);
    assert_eq!(manager.status(&consumer).unwrap().state, PluginState::Waiting);
    assert!(manager.scope(&provider).is_none());
    assert_eq!(*events.lock().unwrap(), ["start:example.provider", "stop:example.provider"]);
}

#[tokio::test]
async fn unexpected_provider_closure_invalidates_consumers_on_reconciliation() {
    let events = Events::default();
    let mut manager = manager();
    let provider = id("example.provider");
    let consumer = id("example.consumer");
    manager.install(Arc::new(TestPlugin::new(provider.as_str(), &events).provides("example.service"))).unwrap();
    manager.install(Arc::new(TestPlugin::new(consumer.as_str(), &events).requires("example.service"))).unwrap();
    manager.enable(&provider).await.unwrap();
    manager.enable(&consumer).await.unwrap();
    manager.scope(&provider).unwrap().quiesce();
    assert!(manager.scope(&provider).is_none());
    assert_eq!(manager.status(&provider).unwrap().state, PluginState::RestartRequired);
    manager.enable(&consumer).await.unwrap();
    assert_eq!(manager.status(&provider).unwrap().state, PluginState::Failed);
    assert_eq!(manager.status(&consumer).unwrap().state, PluginState::Waiting);
    assert_eq!(
        *events.lock().unwrap(),
        ["start:example.provider", "start:example.consumer", "stop:example.consumer", "stop:example.provider"]
    );
}

#[tokio::test]
async fn stop_failure_retains_the_generation_and_requires_cleanup_retry() {
    let events = Events::default();
    let mut manager = manager();
    let plugin = TestPlugin::new("example.plugin", &events);
    let fail_stop = Arc::clone(&plugin.fail_stop);
    fail_stop.store(true, Ordering::SeqCst);
    manager.install(Arc::new(plugin)).unwrap();
    let plugin = id("example.plugin");
    manager.enable(&plugin).await.unwrap();
    let original = manager.scope(&plugin).unwrap();
    assert!(matches!(manager.disable(&plugin).await, Err(ManagerError::Callback { phase: "stop", .. })));
    assert_eq!(manager.status(&plugin).unwrap().generation, Some(original.generation()));
    assert_eq!(manager.status(&plugin).unwrap().state, PluginState::RestartRequired);
    assert!(matches!(manager.enable(&plugin).await, Err(ManagerError::Incomplete { .. })));
    fail_stop.store(false, Ordering::SeqCst);
    manager.disable(&plugin).await.unwrap();
    assert_eq!(manager.status(&plugin).unwrap().state, PluginState::Disabled);
    manager.enable(&plugin).await.unwrap();
    assert!(manager.scope(&plugin).unwrap().generation() > original.generation());
}
