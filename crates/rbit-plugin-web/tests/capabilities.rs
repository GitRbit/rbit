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
#[template(source = "<p>Invalid</p>", ext = "html")]
struct Invalid;
fn scope() -> PluginScope {
    PluginScope::new(Generation::INITIAL)
}
fn owner() -> PluginId {
    "example.capabilities".parse().unwrap()
}
fn form() -> FormAction {
    FormAction::new("save", "/-/plugins/example.capabilities/save", "proof").unwrap()
}
fn context() -> MutationContext {
    MutationContext::new(PageActor { account_id: 1, is_admin: true }, None, LocaleSnapshot::english(0), form())
}
fn upload(max_bytes: usize) -> MutationDescriptor {
    MutationDescriptor::new(
        "save",
        PageScope::Global,
        "save",
        MutationAudience::SiteAdmin,
        Duration::from_millis(100),
        FormSchema::new(Vec::new()).unwrap(),
    )
    .unwrap()
    .with_upload(max_bytes)
    .unwrap()
}
fn preference() -> MutationDescriptor {
    MutationDescriptor::locale_preference("save", "save", Duration::from_millis(100)).unwrap()
}
fn uploaded(descriptor: MutationDescriptor, calls: &Arc<AtomicUsize>) -> MutationRegistration {
    let calls = Arc::clone(calls);
    compiled_upload(descriptor, move |_, bytes| {
        let calls = Arc::clone(&calls);
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(bytes.bytes(), [1]);
            Ok(MutationOutcome::<Invalid>::Redirect(PluginRedirect::new("edit").unwrap()))
        }
    })
}
fn preferred(descriptor: MutationDescriptor, calls: &Arc<AtomicUsize>) -> MutationRegistration {
    let calls = Arc::clone(calls);
    compiled_locale_preference(descriptor, move |context, choice| {
        let calls = Arc::clone(&calls);
        async move {
            assert!(context.actor.is_none());
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(PreferenceOutcome::<Invalid>::Selected { choice, redirect: PluginRedirect::new("edit").unwrap() })
        }
    })
}
async fn wait_for(value: &AtomicUsize, count: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while value.load(Ordering::SeqCst) != count {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[test]
fn binary_and_locale_declarations_cannot_open_standard_mutations_to_guests() {
    assert!(UploadData::new(vec![0; MAX_UPLOAD_BYTES + 1]).is_err());
    for audience in [MutationAudience::SignedIn, MutationAudience::LocalePreference] {
        assert!(
            MutationDescriptor::new(
                "save",
                PageScope::Global,
                "save",
                audience,
                Duration::from_millis(10),
                FormSchema::new(Vec::new()).unwrap()
            )
            .and_then(|d| d.with_upload(1))
            .is_err()
        );
    }
    assert!(
        MutationDescriptor::new(
            "save",
            PageScope::Global,
            "save",
            MutationAudience::SiteAdmin,
            Duration::from_millis(10),
            FormSchema::new(Vec::new()).unwrap()
        )
        .unwrap()
        .with_upload(MAX_UPLOAD_BYTES + 1)
        .is_err()
    );
    let registry = MutationRegistry::new();
    let scope = scope();
    let wrong =
        compiled_mutation(preference(), |_, _| async { Ok(MutationOutcome::<Invalid>::Redirect(PluginRedirect::new("edit").unwrap())) });
    assert!(registry.stage(owner(), &scope, scope.generation(), vec![wrong]).is_err());
    let calls = Arc::default();
    assert!(registry.stage(owner(), &scope, scope.generation(), vec![uploaded(preference(), &calls)]).is_err());
    assert!(registry.stage(owner(), &scope, scope.generation(), vec![preferred(upload(1), &calls)]).is_err());
    assert!(registry.snapshot().is_empty());
    assert_eq!(scope.in_flight(), 0);
    for invalid in ["../en", "fr_FR", "fr--FR", "é", "en\r\n", "-fr", "fr-", "123"] {
        assert!(LocaleChoice::new(invalid).is_err(), "{invalid}");
    }
    for valid in ["en", "en-XA", "ar-XB", "zh-Hant", "de-DE"] {
        assert_eq!(LocaleChoice::new(valid).unwrap().preference(), Some(valid));
    }
    let mut actions = FormActions::default();
    assert!(actions.locale_preference().is_none());
    actions.set_locale_preference(LocaleChoice::Automatic);
    assert_eq!(actions.locale_preference().unwrap().preference(), None);
}
#[tokio::test]
async fn upload_capacity_is_shared_and_retained_through_completed_response() {
    let registry = MutationRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    registry.stage(owner(), &scope, scope.generation(), vec![uploaded(upload(8), &calls)]).unwrap().publish().unwrap();
    let route = registry.snapshot().find(&owner(), PageScope::Global, "save").unwrap();
    let mut leases: Vec<_> = (0..MAX_CONCURRENT_UPLOADS).map(|_| route.admit().unwrap()).collect();
    assert!(registry.clone().snapshot().find(&owner(), PageScope::Global, "save").unwrap().admit().is_err());
    let lease = leases.pop().unwrap();
    let outcome = lease.execute_upload(context(), UploadData::new(vec![1]).unwrap()).await.unwrap();
    let (_, operation, _) = outcome.into_parts();
    assert!(route.admit().is_err());
    drop(operation);
    let reused = route.admit().unwrap();
    drop(reused);
    drop(leases);
    assert!(route.admit().unwrap().execute_upload(context(), UploadData::new(vec![0; 9]).unwrap()).await.is_err());
    assert!(route.admit().unwrap().execute(context(), FormData::new(Vec::new()).unwrap()).await.is_err());
    let mut unauthorized = context();
    unauthorized.actor.is_admin = false;
    assert!(route.admit().unwrap().execute_upload(unauthorized, UploadData::new(vec![1]).unwrap()).await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    registry.wait_for_idle().await;
    assert_eq!(scope.in_flight(), 0);
}
struct WorkDrop {
    scope: PluginScope,
    drops: Arc<AtomicUsize>,
}
impl Drop for WorkDrop {
    fn drop(&mut self) {
        assert!(self.scope.in_flight() > 0);
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
#[tokio::test]
async fn aborted_uploads_keep_all_four_permits_and_scope_until_real_commit_completion() {
    let registry = MutationRegistry::new();
    let scope = scope();
    let started = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let factory = {
        let started = Arc::clone(&started);
        let drops = Arc::clone(&drops);
        let release = Arc::clone(&release);
        let scope = scope.clone();
        compiled_upload(upload(8), move |_, bytes| {
            let started = Arc::clone(&started);
            let release = Arc::clone(&release);
            let guard = WorkDrop { scope: scope.clone(), drops: Arc::clone(&drops) };
            async move {
                let _guard = guard;
                started.fetch_add(1, Ordering::SeqCst);
                release.acquire().await.unwrap().forget();
                assert_eq!(bytes.bytes(), [1]);
                Ok(MutationOutcome::<Invalid>::Redirect(PluginRedirect::new("edit").unwrap()))
            }
        })
    };
    registry.stage(owner(), &scope, scope.generation(), vec![factory]).unwrap().publish().unwrap();
    let route = registry.snapshot().find(&owner(), PageScope::Global, "save").unwrap();
    let mut tasks = Vec::new();
    for _ in 0..MAX_CONCURRENT_UPLOADS {
        let lease = route.admit().unwrap();
        tasks.push(tokio::spawn(async move { lease.execute_upload(context(), UploadData::new(vec![1]).unwrap()).await }));
    }
    wait_for(&started, 4).await;
    for task in tasks {
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }
    assert!(route.admit().is_err());
    assert_eq!(scope.in_flight(), 4);
    assert!(matches!(scope.dispose(Duration::ZERO).await, Err(DrainError::Timeout { in_flight: 4 })));
    let drain_registry = registry.clone();
    let drain = tokio::spawn(async move {
        drain_registry.wait_for_idle().await;
    });
    tokio::task::yield_now().await;
    assert!(!drain.is_finished());
    release.add_permits(4);
    tokio::time::timeout(Duration::from_secs(2), drain).await.unwrap().unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 4);
    assert_eq!(scope.in_flight(), 0);
}
#[tokio::test]
async fn anonymous_preferences_only_emit_typed_locale_intent_and_validate_exact_input() {
    let registry = MutationRegistry::new();
    let scope = scope();
    let calls = Arc::default();
    registry.stage(owner(), &scope, scope.generation(), vec![preferred(preference(), &calls)]).unwrap().publish().unwrap();
    let route = registry.snapshot().find(&owner(), PageScope::Global, "save").unwrap();
    for choice in ["", "fr"] {
        let context = MutationContext::for_preference(None, LocaleSnapshot::english(0), form());
        let input = FormData::new(vec![("locale".into(), choice.into())]).unwrap();
        let response = route.admit().unwrap().execute(context, input).await.unwrap();
        let (response, guard, _) = response.into_parts();
        let MutationResponse::LocalePreference { choice: selected, .. } = response else {
            panic!("only a typed preference intent expected")
        };
        assert_eq!(selected.preference(), (!choice.is_empty()).then_some(choice));
        drop(guard);
    }
    assert!(route.admit().unwrap().execute(context(), FormData::new(vec![("locale".into(), "fr".into())]).unwrap()).await.is_err());
    for fields in
        [Vec::new(), vec![("locale".into(), "../en".into())], vec![("locale".into(), "fr".into()), ("extra".into(), "bad".into())]]
    {
        assert!(
            route
                .admit()
                .unwrap()
                .execute(MutationContext::for_preference(None, LocaleSnapshot::english(0), form()), FormData::new(fields).unwrap())
                .await
                .is_err()
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    registry.wait_for_idle().await;
}
