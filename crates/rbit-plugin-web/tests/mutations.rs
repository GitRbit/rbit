use askama::Template;
use rbit_messages::LocaleSnapshot;
use rbit_plugin_api::{Generation, PluginId};
use rbit_plugin_runtime::{DrainError, PluginScope};
use rbit_plugin_web::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
#[derive(Template)]
#[template(
    source = "<form method=\"post\" action=\"{{ form.action() }}\"><input name=\"_csrf\" value=\"{{ form.csrf() }}\">{{ text }}</form>",
    ext = "html"
)]
struct Editor {
    form: FormAction,
    text: String,
}
fn owner() -> PluginId {
    "example.forms".parse().unwrap()
}
fn scope() -> PluginScope {
    PluginScope::new(Generation::INITIAL)
}
fn action() -> FormAction {
    FormAction::new("save", "/-/plugins/example.forms/save", "opaque-proof").unwrap()
}
fn context() -> MutationContext {
    MutationContext::new(PageActor { account_id: 1, is_admin: false }, None, LocaleSnapshot::english(0), action())
}
fn schema() -> FormSchema {
    FormSchema::new(vec![FormField { name: "name".into(), kind: FormFieldKind::Text { max_bytes: 64, multiline: false }, required: true }])
        .unwrap()
}
fn input() -> FormData {
    FormData::new(vec![("name".into(), "<script>secret</script>".into())]).unwrap()
}
fn descriptor() -> MutationDescriptor {
    MutationDescriptor::new("save", PageScope::Global, "save", MutationAudience::SignedIn, Duration::from_millis(100), schema()).unwrap()
}
fn registration(calls: &Arc<AtomicUsize>) -> MutationRegistration {
    let calls = Arc::clone(calls);
    compiled_mutation(descriptor(), move |context, values| {
        let calls = Arc::clone(&calls);
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(MutationOutcome::Invalid(CompiledPage::new(
                "Edit",
                Editor { form: context.form, text: values.text("name").unwrap().into() },
            )?))
        }
    })
}
async fn wait_count(count: &AtomicUsize, value: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while count.load(Ordering::SeqCst) != value {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[test]
fn strict_schemas_and_input_reject_unbounded_unknown_duplicate_and_reserved_values() {
    for fields in [
        vec![("_csrf".into(), "secret".into())],
        vec![("name".into(), "one".into()), ("name".into(), "two".into())],
        vec![("name".into(), "x".repeat(MAX_FORM_FIELD_BYTES + 1))],
        vec![("na\0me".into(), "x".into())],
    ] {
        assert!(FormData::new(fields).is_err());
    }
    assert!(FormData::new((0..33).map(|n| (format!("field{n}"), String::new())).collect()).is_err());
    assert!(FormData::new((0..9).map(|n| (format!("field{n}"), "x".repeat(MAX_FORM_FIELD_BYTES))).collect()).is_err());
    for action in ["/admin", "//evil/save", "/-/plugins/example.forms/a%2fb", "/alice/rocket/-/plugins/example.forms/../save"] {
        assert!(FormAction::new("save", action, "proof").is_err());
    }
    assert!(schema().validate(FormData::new(vec![("other".into(), "value".into())]).unwrap()).is_err());
    assert!(schema().validate(FormData::new(Vec::new()).unwrap()).is_err());
    assert!(schema().validate(FormData::new(vec![("name".into(), "bad\nline".into())]).unwrap()).is_err());
    assert!(!format!("{:?}", input()).contains("secret"));
    let typed = FormSchema::new(vec![
        FormField { name: "count".into(), kind: FormFieldKind::Integer { min: 1, max: 3 }, required: true },
        FormField { name: "enabled".into(), kind: FormFieldKind::Boolean, required: false },
    ])
    .unwrap();
    let result = typed.validate(FormData::new(vec![("count".into(), "2".into()), ("enabled".into(), "true".into())]).unwrap()).unwrap();
    assert_eq!(result.integer("count"), Some(2));
    assert_eq!(result.boolean("enabled"), Some(true));
    assert!(typed.validate(FormData::new(vec![("count".into(), "4".into())]).unwrap()).is_err());
    assert!(FormSchema::new(vec![FormField { name: "_csrf".into(), kind: FormFieldKind::Boolean, required: false }]).is_err());
    assert!(
        FormSchema::new(vec![FormField { name: "count".into(), kind: FormFieldKind::Integer { min: 2, max: 1 }, required: true }]).is_err()
    );
}
#[tokio::test]
async fn typed_validation_rerender_and_versions_are_atomic_and_generation_owned() {
    let registry = MutationRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    let registration = registry.stage(owner(), &scope, scope.generation(), vec![registration(&calls)]).unwrap();
    assert!(registry.snapshot().is_empty());
    registration.publish().unwrap();
    let stale = registry.snapshot().find(&owner(), PageScope::Global, "save").unwrap();
    let version = stale.version();
    let execution = stale.admit().unwrap().execute(context(), input()).await.unwrap();
    let (result, operation, _) = execution.into_parts();
    let MutationResponse::Invalid(body) = result else { panic!("expected validation model") };
    assert!(!body.html().contains("<script>"));
    assert!(body.html().contains("opaque-proof"));
    assert_eq!(scope.in_flight(), 1);
    drop(operation);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    registration.replace(vec![crate::registration(&calls)]).unwrap();
    assert!(stale.admit().is_err());
    assert!(registry.snapshot().find(&owner(), PageScope::Global, "save").unwrap().version() > version);
    let current = registry.snapshot().find(&owner(), PageScope::Global, "save").unwrap().version();
    assert!(registration.replace(Vec::new()).is_err());
    assert_eq!(registry.snapshot().find(&owner(), PageScope::Global, "save").unwrap().version(), current);
    registry.close();
    assert!(registry.is_closed());
    assert!(registration.publish().is_err());
}
#[tokio::test]
async fn authorization_and_invalid_forms_never_invoke_callback() {
    let registry = MutationRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    registry.stage(owner(), &scope, scope.generation(), vec![registration(&calls)]).unwrap().publish().unwrap();
    let route = registry.snapshot().find(&owner(), PageScope::Global, "save").unwrap();
    let mut invalid = context();
    invalid.actor.account_id = 0;
    assert!(route.admit().unwrap().execute(invalid, input()).await.is_err());
    assert!(route.admit().unwrap().execute(context(), FormData::new(Vec::new()).unwrap()).await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(scope.in_flight(), 0);
    for audience in [MutationAudience::RepositoryWriter, MutationAudience::RepositoryAdmin] {
        assert!(MutationDescriptor::new("bad", PageScope::Global, "save", audience, Duration::from_millis(10), schema()).is_err());
    }
    for path in ["/admin", "../admin", "save?next=//evil", "https://evil"] {
        assert!(PluginRedirect::new(path).is_err());
    }
}
struct FutureDrop {
    scope: PluginScope,
    drops: Arc<AtomicUsize>,
}
impl Drop for FutureDrop {
    fn drop(&mut self) {
        assert!(self.scope.in_flight() > 0);
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
#[tokio::test]
async fn commits_survive_waiter_abort_timeout_and_quiesce_until_actual_completion() {
    for mode in ["abort", "timeout", "quiesce"] {
        let registry = MutationRegistry::new();
        let scope = scope();
        let started = Arc::new(AtomicUsize::new(0));
        let finished = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(tokio::sync::Notify::new());
        let cancelled = Arc::new(AtomicUsize::new(0));
        let factory = {
            let started = Arc::clone(&started);
            let finished = Arc::clone(&finished);
            let drops = Arc::clone(&drops);
            let release = Arc::clone(&release);
            let cancelled = Arc::clone(&cancelled);
            let scope = scope.clone();
            compiled_mutation(descriptor(), move |context, _| {
                let started = Arc::clone(&started);
                let finished = Arc::clone(&finished);
                let release = Arc::clone(&release);
                let cancelled = Arc::clone(&cancelled);
                let guard = FutureDrop { scope: scope.clone(), drops: Arc::clone(&drops) };
                async move {
                    let _guard = guard;
                    started.fetch_add(1, Ordering::SeqCst);
                    context.cancellation_token().cancelled().await;
                    cancelled.fetch_add(1, Ordering::SeqCst);
                    release.notified().await;
                    finished.fetch_add(1, Ordering::SeqCst);
                    Ok(MutationOutcome::<Editor>::Redirect(PluginRedirect::new("edit").unwrap()))
                }
            })
        };
        registry.stage(owner(), &scope, scope.generation(), vec![factory]).unwrap().publish().unwrap();
        let lease = registry.snapshot().find(&owner(), PageScope::Global, "save").unwrap().admit().unwrap();
        let request = tokio::spawn(async move { lease.execute(context(), input()).await });
        wait_count(&started, 1).await;
        match mode {
            "abort" => {
                request.abort();
                assert!(request.await.unwrap_err().is_cancelled());
            }
            "quiesce" => {
                scope.quiesce();
                assert!(tokio::time::timeout(Duration::from_secs(2), request).await.unwrap().unwrap().is_err());
            }
            _ => {
                assert!(matches!(
                    tokio::time::timeout(Duration::from_secs(2), request).await.unwrap().unwrap(),
                    Err(MutationError::Timeout)
                ));
            }
        }
        wait_count(&cancelled, 1).await;
        assert_eq!(finished.load(Ordering::SeqCst), 0);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        assert_eq!(scope.in_flight(), 1);
        assert!(matches!(scope.dispose(Duration::ZERO).await, Err(DrainError::Timeout { in_flight: 1 })));
        let draining = registry.clone();
        let drain = tokio::spawn(async move {
            draining.wait_for_idle().await;
        });
        tokio::task::yield_now().await;
        assert!(!drain.is_finished(), "host storage shutdown must wait for the live commit worker");
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(2), drain).await.unwrap().unwrap();
        wait_count(&drops, 1).await;
        scope.dispose(Duration::from_secs(1)).await.unwrap();
        assert_eq!(finished.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn scoped_get_bridge_supplies_explicit_host_forms_without_changing_page_context() {
    let registry = PageRegistry::new();
    let scope = scope();
    let descriptor = PageDescriptor::new("edit", PageScope::Global, "edit", PageAudience::SignedIn, Duration::from_secs(1))
        .unwrap()
        .with_forms(vec![FormReference::new("save", "save").unwrap()])
        .unwrap();
    registry
        .stage(
            owner(),
            &scope,
            scope.generation(),
            vec![compiled_form_page(descriptor, |_, forms| async move {
                CompiledPage::new("Edit", Editor { form: forms.get("save").unwrap().clone(), text: "hello".into() })
            })],
        )
        .unwrap()
        .publish()
        .unwrap();
    let mut lease = registry.snapshot().find(&owner(), PageScope::Global, "edit").unwrap().admit().unwrap();
    let context =
        PageContext { actor: Some(PageActor { account_id: 1, is_admin: false }), repository: None, locale: LocaleSnapshot::english(0) };
    let body = lease.render_with_forms(context, FormActions::new(vec![action()]).unwrap()).await.unwrap();
    assert!(body.html().contains("opaque-proof"));
    assert!(!format!("{:?}", action()).contains("opaque-proof"));
}

#[tokio::test]
async fn queued_revoked_mutations_never_construct_callbacks_and_capacity_is_shared() {
    use std::future::Future as _;
    let registry = MutationRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    let registration = registry.stage(owner(), &scope, scope.generation(), vec![registration(&calls)]).unwrap();
    registration.publish().unwrap();
    let route = registry.snapshot().find(&owner(), PageScope::Global, "save").unwrap();
    let mut leases: Vec<_> = (0..MAX_CONCURRENT_MUTATIONS).map(|_| route.admit().unwrap()).collect();
    assert!(registry.clone().snapshot().find(&owner(), PageScope::Global, "save").unwrap().admit().is_err());
    let lease = leases.pop().unwrap();
    drop(leases);
    let mut execution = Box::pin(lease.execute(context(), input()));
    let mut task_context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(execution.as_mut().poll(&mut task_context).is_pending());
    registry.close();
    assert!(execution.await.is_err());
    scope.dispose(Duration::from_secs(1)).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(scope.in_flight(), 0);
}

#[tokio::test]
async fn repository_writer_floor_does_not_override_read_only_policy_for_administrators() {
    let registry = MutationRegistry::new();
    let scope = scope();
    let calls = Arc::new(AtomicUsize::new(0));
    let called = Arc::clone(&calls);
    let descriptor = MutationDescriptor::new(
        "save",
        PageScope::Repository,
        "save",
        MutationAudience::RepositoryWriter,
        Duration::from_secs(1),
        schema(),
    )
    .unwrap();
    let factory = compiled_mutation(descriptor, move |_, _| {
        let called = Arc::clone(&called);
        async move {
            called.fetch_add(1, Ordering::SeqCst);
            Ok(MutationOutcome::<Editor>::Redirect(PluginRedirect::new("edit").unwrap()))
        }
    });
    registry.stage(owner(), &scope, scope.generation(), vec![factory]).unwrap().publish().unwrap();
    let route = registry.snapshot().find(&owner(), PageScope::Repository, "save").unwrap();
    let mut context = MutationContext::new(
        PageActor { account_id: 1, is_admin: true },
        Some(MutationRepository { repository_id: 1, owner: "alice".into(), name: "rocket".into(), can_write: false, can_admin: true }),
        LocaleSnapshot::english(0),
        FormAction::new("save", "/alice/rocket/-/plugins/example.forms/save", "proof").unwrap(),
    );
    assert!(matches!(route.admit().unwrap().execute(context.clone(), input()).await, Err(MutationError::Forbidden)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    context.repository.as_mut().unwrap().can_write = true;
    let result = route.admit().unwrap().execute(context, input()).await.unwrap();
    drop(result);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    registry.wait_for_idle().await;
}

#[tokio::test]
async fn aborted_waiter_before_dispatch_does_not_start_mutation_code() {
    use std::future::Future as _;
    let registry = MutationRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    registry.stage(owner(), &scope, scope.generation(), vec![registration(&calls)]).unwrap().publish().unwrap();
    let lease = registry.snapshot().find(&owner(), PageScope::Global, "save").unwrap().admit().unwrap();
    let mut execution = Box::pin(lease.execute(context(), input()));
    let mut task_context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(execution.as_mut().poll(&mut task_context).is_pending());
    drop(execution);
    tokio::time::timeout(Duration::from_secs(2), async {
        while scope.in_flight() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!registry.is_closed());
    registry.wait_for_idle().await;
}
