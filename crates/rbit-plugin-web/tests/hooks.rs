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
    HookDecision, HookFuture, HookRegistration, Rejection, RequestHook, RequestHookRegistry, RequestMetadata, RequestMethod,
    ResponseAnnotation,
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
