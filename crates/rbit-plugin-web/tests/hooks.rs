use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use rbit_plugin_api::Generation;
use rbit_plugin_runtime::PluginScope;
use rbit_plugin_web::{
    HookDecision, HookFuture, HookRegistration, MAX_CONCURRENT_PIPELINES, Rejection, RequestHook, RequestHookRegistry, RequestMetadata,
    RequestMethod, ResponseAnnotation,
};

#[derive(Debug)]
struct Hook {
    name: &'static str,
    calls: Arc<Mutex<Vec<&'static str>>>,
    drops: Arc<AtomicUsize>,
    delay: Duration,
    decision: HookDecision,
}
struct Completion(Arc<AtomicUsize>);
impl Drop for Completion {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl RequestHook for Hook {
    fn call<'a>(&'a self, _: &'a RequestMetadata) -> HookFuture<'a> {
        Box::pin(async move {
            let _completion = Completion(Arc::clone(&self.drops));
            self.calls.lock().unwrap().push(self.name);
            tokio::time::sleep(self.delay).await;
            self.decision.clone()
        })
    }
}
fn descriptor(
    name: &'static str,
    order: i16,
    delay: Duration,
    calls: &Arc<Mutex<Vec<&'static str>>>,
    drops: &Arc<AtomicUsize>,
    decision: HookDecision,
) -> HookRegistration {
    HookRegistration::new(
        name,
        order,
        Duration::from_millis(100),
        Arc::new(Hook { name, calls: Arc::clone(calls), drops: Arc::clone(drops), delay, decision }),
    )
    .unwrap()
}
fn metadata() -> RequestMetadata {
    RequestMetadata::new(RequestMethod::Get, "/repos/{owner}/{repo}").unwrap()
}
fn scope() -> PluginScope {
    PluginScope::new(Generation::INITIAL)
}

#[test]
fn descriptors_metadata_and_annotations_are_bounded_before_registration() {
    let calls = Arc::default();
    let drops = Arc::default();
    let callback = Arc::new(Hook { name: "test", calls, drops, delay: Duration::ZERO, decision: HookDecision::Continue(Vec::new()) });
    for id in ["", "UPPER", "invalid id"] {
        assert!(HookRegistration::new(id, 0, Duration::from_millis(10), callback.clone()).is_err());
    }
    assert!(HookRegistration::new("test", 0, Duration::from_millis(101), callback.clone()).is_err());
    assert!(HookRegistration::new("test", 1025, Duration::from_millis(1), callback).is_err());
    for name in ["set-cookie", "cache-control", "authorization", "content-security-policy", "x-rbit-plugin-", "x-rbit-plugin-UPPER"] {
        assert!(ResponseAnnotation::new(name, "value").is_err());
    }
    for value in ["bad\r\nvalue", "bad\tvalue"] {
        assert!(ResponseAnnotation::new("x-rbit-plugin-test", value).is_err());
    }
    assert!(ResponseAnnotation::new("x-rbit-plugin-test", &"x".repeat(257)).is_err());
    assert!(RequestMetadata::new(RequestMethod::Get, &"x".repeat(2049)).is_err());
    assert!(RequestMetadata::new(RequestMethod::Get, "/path?secret").is_err());
    assert_eq!(RequestMethod::from_http("private-extension-method"), RequestMethod::Other);
}

#[tokio::test(start_paused = true)]
async fn order_is_deterministic_and_replace_withdraws_retained_snapshots() {
    let registry = RequestHookRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    let drops = Arc::default();
    let registration = registry
        .stage(
            "rbit.test".parse().unwrap(),
            &scope,
            scope.generation(),
            vec![
                descriptor("later", 2, Duration::ZERO, &calls, &drops, HookDecision::Continue(Vec::new())),
                descriptor("first", -1, Duration::ZERO, &calls, &drops, HookDecision::Continue(Vec::new())),
            ],
        )
        .unwrap();
    assert!(registry.snapshot().is_empty(), "staging does not publish callbacks");
    registration.publish().unwrap();
    let old = registry.snapshot();
    assert_eq!(old.evaluate(&metadata()).await, HookDecision::Continue(Vec::new()));
    assert_eq!(*calls.lock().unwrap(), ["first", "later"]);
    registration.replace(vec![descriptor("replacement", 0, Duration::ZERO, &calls, &drops, HookDecision::Continue(Vec::new()))]).unwrap();
    assert_eq!(old.evaluate(&metadata()).await, HookDecision::Reject(Rejection::Unavailable));
    assert_eq!(registry.snapshot().evaluate(&metadata()).await, HookDecision::Continue(Vec::new()));
    assert_eq!(calls.lock().unwrap().last(), Some(&"replacement"));
    registration.unpublish();
    assert!(registry.snapshot().is_empty());
    assert_eq!(scope.in_flight(), 0);
}

#[tokio::test(start_paused = true)]
async fn per_hook_timeout_drops_the_future_before_releasing_admission() {
    let registry = RequestHookRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    let drops = Arc::default();
    let registration = registry
        .stage(
            "rbit.test".parse().unwrap(),
            &scope,
            scope.generation(),
            vec![descriptor("slow", 0, Duration::from_secs(1), &calls, &drops, HookDecision::Continue(Vec::new()))],
        )
        .unwrap();
    registration.publish().unwrap();
    let started = tokio::time::Instant::now();
    assert_eq!(registry.snapshot().evaluate(&metadata()).await, HookDecision::Reject(Rejection::Unavailable));
    assert_eq!(started.elapsed(), Duration::from_millis(100));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(scope.in_flight(), 0);
}

#[tokio::test(start_paused = true)]
async fn aggregate_budget_is_not_multiplied_by_the_number_of_live_hooks() {
    let registry = RequestHookRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    let drops = Arc::default();
    let hooks = (0..16)
        .map(|_| descriptor("one", 0, Duration::from_millis(80), &calls, &drops, HookDecision::Continue(Vec::new())))
        .collect::<Vec<_>>();
    assert!(registry.stage("rbit.duplicates".parse().unwrap(), &scope, scope.generation(), hooks).is_err());
    let names = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p"];
    let registration = registry
        .stage(
            "rbit.test".parse().unwrap(),
            &scope,
            scope.generation(),
            names
                .into_iter()
                .map(|name| descriptor(name, 0, Duration::from_millis(80), &calls, &drops, HookDecision::Continue(Vec::new())))
                .collect(),
        )
        .unwrap();
    registration.publish().unwrap();
    let started = tokio::time::Instant::now();
    assert_eq!(registry.snapshot().evaluate(&metadata()).await, HookDecision::Reject(Rejection::Unavailable));
    assert_eq!(started.elapsed(), Duration::from_millis(500));
    assert_eq!(calls.lock().unwrap().len(), 7);
    assert_eq!(scope.in_flight(), 0);
}

#[tokio::test]
async fn quiesce_cancels_an_invocation_and_cannot_leave_queued_leases() {
    let registry = RequestHookRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    let drops = Arc::default();
    let registration = registry
        .stage(
            "rbit.test".parse().unwrap(),
            &scope,
            scope.generation(),
            vec![descriptor("slow", 0, Duration::from_secs(10), &calls, &drops, HookDecision::Continue(Vec::new()))],
        )
        .unwrap();
    registration.publish().unwrap();
    let snapshot = registry.snapshot();
    let request = tokio::spawn(async move { snapshot.evaluate(&metadata()).await });
    while calls.lock().unwrap().is_empty() {
        tokio::task::yield_now().await;
    }
    scope.quiesce();
    assert_eq!(request.await.unwrap(), HookDecision::Reject(Rejection::Unavailable));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(scope.in_flight(), 0);
    assert!(registry.snapshot().is_empty());
}

#[tokio::test(start_paused = true)]
async fn annotation_overflow_or_shadowing_fails_closed() {
    for annotations in [
        vec![ResponseAnnotation::new("x-rbit-plugin-test", "one").unwrap(); 2],
        (0..5).map(|index| ResponseAnnotation::new(&format!("x-rbit-plugin-test-{index}"), "value").unwrap()).collect(),
    ] {
        let registry = RequestHookRegistry::new();
        let scope = scope();
        let calls = Arc::default();
        let drops = Arc::default();
        let registration = registry
            .stage(
                "rbit.test".parse().unwrap(),
                &scope,
                scope.generation(),
                vec![descriptor("test", 0, Duration::ZERO, &calls, &drops, HookDecision::Continue(annotations))],
            )
            .unwrap();
        registration.publish().unwrap();
        assert_eq!(registry.snapshot().evaluate(&metadata()).await, HookDecision::Reject(Rejection::Unavailable));
        assert_eq!(scope.in_flight(), 0);
    }
}

#[test]
fn capacity_counts_staged_entries_and_prevents_duplicate_replacements() {
    let registry = RequestHookRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    let drops = Arc::default();
    let hooks = (0..16)
        .map(|index| {
            HookRegistration::new(
                &format!("test-{index}"),
                0,
                Duration::from_millis(1),
                Arc::new(Hook {
                    name: "test",
                    calls: Arc::clone(&calls),
                    drops: Arc::clone(&drops),
                    delay: Duration::ZERO,
                    decision: HookDecision::Continue(Vec::new()),
                }),
            )
            .unwrap()
        })
        .collect();
    let registration = registry.stage("rbit.full".parse().unwrap(), &scope, scope.generation(), hooks).unwrap();
    let one = descriptor("one", 0, Duration::ZERO, &calls, &drops, HookDecision::Continue(Vec::new()));
    assert!(registry.stage("rbit.overflow".parse().unwrap(), &scope, scope.generation(), vec![one.clone()]).is_err());
    assert!(registration.replace(vec![one.clone(), one]).is_err());
    registration.publish().unwrap();
    assert_eq!(registry.snapshot().len(), 16);
}

#[derive(Debug)]
struct Prelude {
    calls: Arc<AtomicUsize>,
    close: Option<PluginScope>,
}
impl RequestHook for Prelude {
    fn call<'a>(&'a self, _: &'a RequestMetadata) -> HookFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(scope) = &self.close {
            scope.quiesce();
        }
        Box::pin(async { HookDecision::Continue(Vec::new()) })
    }
}

#[tokio::test]
async fn cancellation_is_checked_before_constructing_a_queued_callback_future() {
    let registry = RequestHookRegistry::new();
    let first = scope();
    let queued = scope();
    let first_calls = Arc::new(AtomicUsize::new(0));
    let queued_calls = Arc::new(AtomicUsize::new(0));
    let one = HookRegistration::new(
        "first",
        -1,
        Duration::from_millis(10),
        Arc::new(Prelude { calls: Arc::clone(&first_calls), close: Some(queued.clone()) }),
    )
    .unwrap();
    let two =
        HookRegistration::new("queued", 0, Duration::from_millis(10), Arc::new(Prelude { calls: Arc::clone(&queued_calls), close: None }))
            .unwrap();
    registry.stage("rbit.first".parse().unwrap(), &first, first.generation(), vec![one]).unwrap().publish().unwrap();
    registry.stage("rbit.queued".parse().unwrap(), &queued, queued.generation(), vec![two]).unwrap().publish().unwrap();
    assert_eq!(registry.snapshot().evaluate(&metadata()).await, HookDecision::Reject(Rejection::Unavailable));
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(queued_calls.load(Ordering::SeqCst), 0, "even synchronous trait preludes must not run after queued cancellation");
    assert_eq!(first.in_flight(), 0);
    assert_eq!(queued.in_flight(), 0);
}

#[derive(Debug)]
struct FactoryProbe {
    hook: Hook,
    constructions: Arc<AtomicUsize>,
}
impl RequestHook for FactoryProbe {
    fn call<'a>(&'a self, request: &'a RequestMetadata) -> HookFuture<'a> {
        self.constructions.fetch_add(1, Ordering::SeqCst);
        self.hook.call(request)
    }
}

#[tokio::test(start_paused = true)]
async fn shared_pipeline_capacity_rejects_before_callbacks_and_recovers_after_abort() {
    let registry = RequestHookRegistry::new();
    let empty = registry.snapshot();
    let scope = scope();
    let calls = Arc::default();
    let drops = Arc::default();
    let constructions = Arc::new(AtomicUsize::new(0));
    let slow = HookRegistration::new(
        "slow",
        0,
        Duration::from_millis(100),
        Arc::new(FactoryProbe {
            constructions: Arc::clone(&constructions),
            hook: Hook {
                name: "slow",
                calls: Arc::clone(&calls),
                drops: Arc::clone(&drops),
                delay: Duration::from_secs(1),
                decision: HookDecision::Continue(Vec::new()),
            },
        }),
    )
    .unwrap();
    let registration = registry.stage("rbit.test".parse().unwrap(), &scope, scope.generation(), vec![slow]).unwrap();
    registration.publish().unwrap();
    let retained = registry.snapshot();
    let clone = registry.clone();
    let mut requests = Vec::new();
    for index in 0..MAX_CONCURRENT_PIPELINES {
        let snapshot = if index % 2 == 0 { retained.clone() } else { clone.snapshot() };
        requests.push(tokio::spawn(async move { snapshot.evaluate(&metadata()).await }));
    }
    for _ in 0..MAX_CONCURRENT_PIPELINES * 2 {
        if calls.lock().unwrap().len() == MAX_CONCURRENT_PIPELINES {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(calls.lock().unwrap().len(), MAX_CONCURRENT_PIPELINES);
    assert_eq!(scope.in_flight(), MAX_CONCURRENT_PIPELINES);
    let started = tokio::time::Instant::now();
    assert_eq!(clone.snapshot().evaluate(&metadata()).await, HookDecision::Reject(Rejection::Unavailable));
    assert_eq!(started.elapsed(), Duration::ZERO, "overload never queues behind a semaphore");
    assert_eq!(calls.lock().unwrap().len(), MAX_CONCURRENT_PIPELINES, "no overloaded callback construction");
    assert_eq!(
        constructions.load(Ordering::SeqCst),
        MAX_CONCURRENT_PIPELINES,
        "even synchronous callback preludes are skipped on overload"
    );
    assert_eq!(empty.evaluate(&metadata()).await, HookDecision::Continue(Vec::new()), "empty snapshots need no permit");
    let aborted = requests.pop().unwrap();
    aborted.abort();
    assert!(aborted.await.unwrap_err().is_cancelled());
    assert_eq!(scope.in_flight(), MAX_CONCURRENT_PIPELINES - 1);
    let snapshot = retained.clone();
    requests.push(tokio::spawn(async move { snapshot.evaluate(&metadata()).await }));
    for _ in 0..MAX_CONCURRENT_PIPELINES * 2 {
        if calls.lock().unwrap().len() == MAX_CONCURRENT_PIPELINES + 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(calls.lock().unwrap().len(), MAX_CONCURRENT_PIPELINES + 1);
    assert_eq!(scope.in_flight(), MAX_CONCURRENT_PIPELINES);
    registration.replace(vec![descriptor("updated", 0, Duration::ZERO, &calls, &drops, HookDecision::Continue(Vec::new()))]).unwrap();
    assert_eq!(
        registry.snapshot().evaluate(&metadata()).await,
        HookDecision::Reject(Rejection::Unavailable),
        "replacement shares the existing permit pool until old invocations end"
    );
    assert_eq!(calls.lock().unwrap().len(), MAX_CONCURRENT_PIPELINES + 1);
    for request in &requests {
        request.abort();
    }
    for request in requests {
        let _ = request.await;
    }
    assert_eq!(scope.in_flight(), 0);
    assert_eq!(registry.snapshot().evaluate(&metadata()).await, HookDecision::Continue(Vec::new()));
    assert_eq!(calls.lock().unwrap().last(), Some(&"updated"));
    assert_eq!(scope.in_flight(), 0);
}

#[tokio::test]
async fn terminal_hook_registry_close_cancels_pending_callback_and_refuses_republication() {
    let registry = RequestHookRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    let drops = Arc::default();
    let make = || descriptor("pending", 0, Duration::from_secs(60), &calls, &drops, HookDecision::Continue(Vec::new()));
    let registration = registry.stage("example.close".parse().unwrap(), &scope, scope.generation(), vec![make()]).unwrap();
    registration.publish().unwrap();
    let retained = registry.snapshot();
    let active = retained.clone();
    let task = tokio::spawn(async move { active.evaluate(&metadata()).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while calls.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    registry.clone().close();
    assert!(registry.is_closed());
    assert_eq!(tokio::time::timeout(Duration::from_secs(2), task).await.unwrap().unwrap(), HookDecision::Reject(Rejection::Unavailable));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(scope.in_flight(), 0);
    assert!(!scope.is_closed());
    assert_eq!(retained.evaluate(&metadata()).await, HookDecision::Reject(Rejection::Unavailable));
    assert!(registry.snapshot().is_empty());
    assert!(registration.publish().is_err());
    assert!(registration.replace(vec![make()]).is_err());
    assert!(registry.stage("example.close".parse().unwrap(), &scope, scope.generation(), vec![make()]).is_err());
    let revision = registry.revision();
    registry.close();
    assert_eq!(registry.revision(), revision);
    scope.dispose(Duration::from_secs(1)).await.unwrap();
}

#[derive(Debug)]
struct LifetimeHook {
    scope: PluginScope,
    observed: Arc<AtomicUsize>,
    started: Arc<AtomicUsize>,
    deny_and_close: Option<RequestHookRegistry>,
}
impl Drop for LifetimeHook {
    fn drop(&mut self) {
        assert!(self.scope.in_flight() > 0, "native callback resources must drop before their admitted operation");
        self.observed.fetch_add(1, Ordering::SeqCst);
    }
}
impl RequestHook for LifetimeHook {
    fn call<'a>(&'a self, _: &'a RequestMetadata) -> HookFuture<'a> {
        Box::pin(async move {
            self.started.fetch_add(1, Ordering::SeqCst);
            if let Some(registry) = &self.deny_and_close {
                registry.close();
                return HookDecision::Reject(Rejection::Forbidden);
            }
            std::future::pending::<HookDecision>().await
        })
    }
}

#[tokio::test]
async fn final_native_hook_resources_drop_before_authority_on_close_abort_and_deny() {
    for mode in ["close", "abort", "deny"] {
        let registry = RequestHookRegistry::new();
        let scope = scope();
        let observed = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(AtomicUsize::new(0));
        let hook = LifetimeHook {
            scope: scope.clone(),
            observed: Arc::clone(&observed),
            started: Arc::clone(&started),
            deny_and_close: (mode == "deny").then(|| registry.clone()),
        };
        registry
            .stage(
                "example.lifetime".parse().unwrap(),
                &scope,
                scope.generation(),
                vec![HookRegistration::new("lifetime", 0, Duration::from_millis(100), Arc::new(hook)).unwrap()],
            )
            .unwrap()
            .publish()
            .unwrap();
        let snapshot = registry.snapshot();
        let task = tokio::spawn(async move { snapshot.evaluate(&metadata()).await });
        tokio::time::timeout(Duration::from_secs(2), async {
            while started.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        if mode != "deny" {
            registry.close();
        }
        if mode == "abort" {
            task.abort();
            assert!(tokio::time::timeout(Duration::from_secs(2), task).await.unwrap().unwrap_err().is_cancelled());
        } else {
            let decision = tokio::time::timeout(Duration::from_secs(2), task).await.unwrap().unwrap();
            assert_eq!(decision, HookDecision::Reject(if mode == "deny" { Rejection::Forbidden } else { Rejection::Unavailable }));
        }
        assert_eq!(observed.load(Ordering::SeqCst), 1);
        assert_eq!(scope.in_flight(), 0);
        scope.dispose(Duration::from_secs(1)).await.unwrap();
    }
}
