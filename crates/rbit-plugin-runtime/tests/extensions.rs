use rbit_plugin_api::{Generation, PluginId};
use rbit_plugin_runtime::{DrainError, ExtensionRegistryError, PluginScope, ScopedExtensionRegistry};
use std::{
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

fn owner() -> PluginId {
    "example.hooks".parse().unwrap()
}
fn scope(number: u64) -> PluginScope {
    PluginScope::new(Generation::new(number).unwrap())
}

#[tokio::test]
async fn staging_admission_replacement_and_operation_drain_are_generation_owned() {
    let registry = ScopedExtensionRegistry::default();
    let scope = scope(1);
    let handle = registry.stage(owner(), &scope, scope.generation(), vec!["first".to_owned()]).unwrap();
    assert!(registry.snapshot().entries.is_empty());
    handle.publish().unwrap();
    let old = registry.snapshot();
    assert!(Arc::ptr_eq(&old.entries, &registry.snapshot().entries));
    let lease = old.entries[0].admit().unwrap();
    assert_eq!(lease.value(), "first");
    let cancellation = lease.cancellation_token();
    handle.replace(vec!["second".to_owned()]).unwrap();
    assert!(old.entries[0].admit().is_err());
    assert!(cancellation.is_cancelled());
    let new = registry.snapshot();
    assert_eq!(new.revision, old.revision + 1);
    assert_eq!(new.entries[0].admit().unwrap().value(), "second");
    let child = new.entries[0].cancellation_token();
    child.cancel();
    assert!(new.entries[0].admit().is_ok());
    assert!(matches!(scope.dispose(Duration::ZERO).await, Err(DrainError::Timeout { in_flight: 1 })));
    drop(lease);
    scope.dispose(Duration::from_secs(1)).await.unwrap();
    assert!(registry.snapshot().entries.is_empty());
    assert!(new.entries[0].admit().is_err());
}

#[test]
fn withdrawing_or_dropping_registry_revokes_retained_snapshots_without_closing_scope() {
    let registry = ScopedExtensionRegistry::default();
    let scope = scope(2);
    let handle = registry.stage(owner(), &scope, scope.generation(), vec![1]).unwrap();
    handle.publish().unwrap();
    let snapshot = registry.snapshot();
    let lease = snapshot.entries[0].admit().unwrap();
    let cancellation = lease.cancellation_token();
    assert!(handle.unpublish());
    assert!(!scope.is_closed());
    assert!(snapshot.entries[0].admit().is_err());
    assert!(cancellation.is_cancelled());
    drop(lease);
    let new = registry.stage(owner(), &scope, scope.generation(), vec![2]).unwrap();
    new.publish().unwrap();
    let snapshot = registry.snapshot();
    drop(registry);
    assert!(snapshot.entries[0].admit().is_err());
    assert!(new.publish().is_err());
}

#[test]
fn total_capacity_includes_staged_entries_and_invalid_replace_is_atomic() {
    let registry = ScopedExtensionRegistry::with_capacity(16).unwrap();
    let scope = scope(3);
    let first = registry.stage(owner(), &scope, scope.generation(), vec![1; 8]).unwrap();
    let second = registry.stage("example.other".parse().unwrap(), &scope, scope.generation(), vec![2; 8]).unwrap();
    assert!(registry.stage("example.third".parse().unwrap(), &scope, scope.generation(), vec![3]).is_err());
    assert!(first.replace(vec![4; 9]).is_err());
    first.publish().unwrap();
    second.publish().unwrap();
    let snapshot = registry.snapshot();
    assert_eq!(snapshot.entries.len(), 16);
    assert!(first.replace(Vec::new()).is_err());
    assert_eq!(registry.snapshot().revision, snapshot.revision);
    assert!(first.replace(vec![0; 33]).is_err());
}

#[test]
fn stale_handles_cannot_replace_or_withdraw_same_generation_number_reuse() {
    let registry = ScopedExtensionRegistry::default();
    let first = scope(4);
    assert!(registry.stage(owner(), &first, Generation::new(5).unwrap(), vec![1]).is_err());
    let old = registry.stage(owner(), &first, first.generation(), vec![1]).unwrap();
    old.publish().unwrap();
    old.unpublish();
    let second = scope(4);
    let new = registry.stage(owner(), &second, second.generation(), vec![2]).unwrap();
    new.publish().unwrap();
    assert!(!old.unpublish());
    assert!(matches!(old.replace(vec![9]), Err(ExtensionRegistryError::Stale)));
    first.quiesce();
    assert_eq!(*registry.snapshot().entries[0].admit().unwrap().value(), 2);
}

struct Reentrant {
    registry: ScopedExtensionRegistry<Reentrant>,
    scope: PluginScope,
    drops: Arc<AtomicUsize>,
}
impl Drop for Reentrant {
    fn drop(&mut self) {
        let _snapshot = self.registry.snapshot();
        self.scope.quiesce();
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn destructors_reentering_registry_and_scope_run_outside_both_serialization_locks() {
    for action in ["unpublish", "replace", "quiesce", "rejected_stage"] {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let registry = ScopedExtensionRegistry::default();
            let scope = scope(10);
            let drops = Arc::new(AtomicUsize::new(0));
            let make = || Reentrant { registry: registry.clone(), scope: scope.clone(), drops: Arc::clone(&drops) };
            let handle = registry.stage(owner(), &scope, scope.generation(), vec![make()]).unwrap();
            handle.publish().unwrap();
            match action {
                "unpublish" => {
                    handle.unpublish();
                }
                "replace" => {
                    handle.replace(vec![make()]).unwrap();
                }
                "quiesce" => scope.quiesce(),
                _ => {
                    assert!(registry.stage(owner(), &scope, scope.generation(), vec![make()]).is_err());
                }
            }
            assert!(drops.load(Ordering::SeqCst) >= 1);
            sender.send(()).unwrap();
        });
        receiver.recv_timeout(Duration::from_secs(2)).expect("destructor must not deadlock a host lock");
        worker.join().unwrap();
    }
}

#[test]
fn racing_close_and_publish_never_leave_admissible_callbacks() {
    for generation in 20..120 {
        let registry = ScopedExtensionRegistry::default();
        let scope = scope(generation);
        let barrier = Arc::new(Barrier::new(2));
        let worker_registry = registry.clone();
        let worker_scope = scope.clone();
        let worker_barrier = Arc::clone(&barrier);
        let worker = thread::spawn(move || {
            worker_barrier.wait();
            if let Ok(handle) = worker_registry.stage(owner(), &worker_scope, worker_scope.generation(), vec![1]) {
                let _ = handle.publish();
            }
            worker_registry.snapshot()
        });
        barrier.wait();
        scope.quiesce();
        let snapshot = worker.join().unwrap();
        assert!(snapshot.entries.iter().all(|entry| entry.admit().is_err()));
        assert!(registry.snapshot().entries.is_empty());
    }
}

/// `cargo test --release -p rbit-plugin-runtime benchmark_warm_extension_snapshots -- --ignored --nocapture`
#[test]
#[ignore = "manual snapshot benchmark; no hardware-dependent timing assertion"]
fn benchmark_warm_extension_snapshots() {
    let registry = ScopedExtensionRegistry::with_capacity(16).unwrap();
    let scopes: Vec<_> = (1000..1016).map(scope).collect();
    for (index, scope) in scopes.iter().enumerate() {
        registry
            .stage(format!("example.perf-{index}").parse().unwrap(), scope, scope.generation(), vec![index])
            .unwrap()
            .publish()
            .unwrap();
    }
    let started = std::time::Instant::now();
    for _ in 0..100_000 {
        std::hint::black_box(registry.snapshot());
    }
    let elapsed = started.elapsed();
    println!("100000 warm snapshots,16 owners: {elapsed:?}; {} ns/snapshot", elapsed.as_nanos() / 100_000);
}

#[test]
fn selection_metadata_is_paired_with_callback_and_never_requires_admitting_other_entries() {
    let registry = ScopedExtensionRegistry::default();
    let scope = scope(2000);
    let registration = registry
        .stage_with_metadata(owner(), &scope, scope.generation(), vec![(10, "old-route".to_owned()), (20, "other-route".to_owned())])
        .unwrap();
    registration.publish().unwrap();
    let old = registry.snapshot();
    let selected = old.entries.iter().find(|entry| entry.metadata::<String>().is_some_and(|path| path == "old-route")).unwrap();
    assert_eq!(scope.in_flight(), 0);
    assert!(selected.metadata::<u64>().is_none());
    let lease = selected.admit().unwrap();
    assert_eq!(*lease.value(), 10);
    assert_eq!(scope.in_flight(), 1);
    drop(lease);
    registration.replace_with_metadata(vec![(30, "new-route".to_owned())]).unwrap();
    assert_eq!(selected.metadata::<String>().unwrap(), "old-route");
    assert!(selected.admit().is_err());
    let current = registry.snapshot();
    assert_eq!(current.entries[0].metadata::<String>().unwrap(), "new-route");
    assert_eq!(*current.entries[0].admit().unwrap().value(), 30);
}

struct ReentrantMetadata {
    registry: ScopedExtensionRegistry<u8>,
    scope: PluginScope,
    drops: Arc<AtomicUsize>,
}
impl Drop for ReentrantMetadata {
    fn drop(&mut self) {
        let _ = self.registry.snapshot();
        self.scope.quiesce();
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn metadata_destructors_also_run_outside_registry_and_scope_locks() {
    let (sender, receiver) = mpsc::channel();
    let worker = thread::spawn(move || {
        let registry = ScopedExtensionRegistry::default();
        let scope = scope(2001);
        let drops = Arc::new(AtomicUsize::new(0));
        let metadata = ReentrantMetadata { registry: registry.clone(), scope: scope.clone(), drops: Arc::clone(&drops) };
        registry.stage_with_metadata(owner(), &scope, scope.generation(), vec![(1, metadata)]).unwrap().publish().unwrap();
        scope.quiesce();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        sender.send(()).unwrap();
    });
    receiver.recv_timeout(Duration::from_secs(2)).expect("metadata cleanup cannot deadlock");
    worker.join().unwrap();
}
