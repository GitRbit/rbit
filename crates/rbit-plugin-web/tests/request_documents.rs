use askama::Template;
use rbit_messages::LocaleSnapshot;
use rbit_plugin_api::{Generation, PluginId};
use rbit_plugin_runtime::PluginScope;
use rbit_plugin_web::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
#[derive(Template)]
#[template(source = "<!doctype html><html><body><p>{{ value }}</p></body></html>", ext = "html")]
struct Document {
    value: String,
}
fn owner() -> PluginId {
    "example.documents".parse().unwrap()
}
fn scope() -> PluginScope {
    PluginScope::new(Generation::INITIAL)
}
fn context() -> PageContext {
    PageContext { actor: None, repository: None, locale: LocaleSnapshot::english(0) }
}
fn descriptor() -> PageDescriptor {
    PageDescriptor::new("read", PageScope::Global, "read", PageAudience::Public, Duration::from_secs(1)).unwrap()
}
#[tokio::test]
async fn bounded_declared_queries_reject_unknown_fields_before_factory_and_keep_documents_compiled() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let registry = PageRegistry::new();
    let scope = scope();
    let page = compiled_request_page(descriptor().with_query(&["run"]).unwrap().with_document(), move |_, _, query| {
        let seen = Arc::clone(&seen);
        async move {
            seen.fetch_add(1, Ordering::SeqCst);
            CompiledPage::new("Run", Document { value: query.get("run").unwrap().into() })
        }
    });
    registry.stage(owner(), &scope, scope.generation(), vec![page]).unwrap().publish().unwrap();
    let handle = registry.snapshot().find(&owner(), PageScope::Global, "read").unwrap();
    assert_eq!(handle.descriptor().presentation(), PagePresentation::Document);
    let mut lease = handle.admit().unwrap();
    assert!(
        lease
            .render_with_request(context(), FormActions::default(), PageQuery::new(vec![("other".into(), "value".into())]).unwrap())
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let body = lease
        .render_with_request(context(), FormActions::default(), PageQuery::new(vec![("run".into(), "<123>".into())]).unwrap())
        .await
        .unwrap();
    assert!(body.html().starts_with("<!doctype html>"));
    assert!(body.html().contains("&#60;123&#62;"));
    for pairs in [
        vec![("run".into(), "1".into()), ("run".into(), "2".into())],
        vec![("run".into(), "x".repeat(MAX_QUERY_VALUE_BYTES + 1))],
        vec![("run".into(), "bad\nvalue".into())],
    ] {
        assert!(PageQuery::new(pairs).is_err());
    }
    assert!(descriptor().with_query(&["run", "run"]).is_err());
    let redirect = PluginRedirect::new("read").unwrap().with_query(&[("run", "1&next=//evil")]).unwrap();
    assert_eq!(redirect.query_string(), "run=1%26next%3D%2F%2Fevil");
    assert_eq!(redirect.target(&owner(), PageScope::Global).as_str(), "/-/plugins/example.documents/read");
}
#[test]
fn declared_lists_prefixes_and_large_text_do_not_relax_normal_form_limits() {
    let value = FormField {
        name: "value".into(),
        kind: FormFieldKind::Text { max_bytes: MAX_LARGE_FORM_FIELD_BYTES, multiline: true },
        required: true,
    };
    assert!(FormSchema::new(vec![value.clone()]).is_err());
    let large = FormSchema::large_text(vec![value]).unwrap();
    assert_eq!(large.limits().max_encoded_bytes(), MAX_LARGE_FORM_BODY_BYTES);
    let secret = "&".repeat(MAX_LARGE_FORM_FIELD_BYTES);
    assert!(FormData::new(vec![("value".into(), secret.clone())]).is_err());
    let parsed = large.validate(FormData::for_schema(vec![("value".into(), secret.clone())], &large).unwrap()).unwrap();
    assert_eq!(parsed.text("value"), Some(secret.as_str()));
    assert!(!format!("{parsed:?}").contains(&secret));
    let schema = FormSchema::new(vec![
        FormField {
            name: "repository".into(),
            kind: FormFieldKind::TextList { max_items: 2, max_bytes: 20, multiline: false },
            required: false,
        },
        FormField { name: "ref".into(), kind: FormFieldKind::Text { max_bytes: 100, multiline: false }, required: true },
    ])
    .unwrap()
    .with_prefix("inputs[", "]", 2, 40)
    .unwrap();
    let pairs = vec![
        ("repository".into(), "1".into()),
        ("repository".into(), "2".into()),
        ("ref".into(), "main".into()),
        ("inputs[Build.Mode]".into(), "debug".into()),
    ];
    let values = schema.validate(FormData::for_schema(pairs, &schema).unwrap()).unwrap();
    assert_eq!(values.list("repository").unwrap(), ["1", "2"]);
    assert!(values.to_pairs().contains(&("inputs[Build.Mode]".into(), "debug".into())));
    for pairs in [
        vec![("ref".into(), "a".into()), ("ref".into(), "b".into())],
        vec![("inputs[x]".into(), "a".into()), ("inputs[x]".into(), "b".into())],
        vec![("repository".into(), "1".into()), ("repository".into(), "2".into()), ("repository".into(), "3".into())],
        vec![("_csrf".into(), "proof".into())],
    ] {
        assert!(FormData::for_schema(pairs, &schema).is_err());
    }
}
#[tokio::test]
async fn opt_in_large_document_budget_is_bounded_and_capacity_follows_response_operations() {
    assert_eq!(descriptor().body_limit(), MAX_PAGE_BODY_BYTES);
    assert!(descriptor().with_body_limit(0).is_err());
    assert!(descriptor().with_body_limit(MAX_LARGE_PAGE_BODY_BYTES + 1).is_err());
    let registry = PageRegistry::new();
    let scope = scope();
    let text = "&".repeat(MAX_LARGE_FORM_FIELD_BYTES);
    let page = compiled_page(descriptor().with_document().with_body_limit(MAX_LARGE_PAGE_BODY_BYTES).unwrap(), move |_| {
        let value = text.clone();
        async move { CompiledPage::new("Large", Document { value }) }
    });
    registry.stage(owner(), &scope, scope.generation(), vec![page]).unwrap().publish().unwrap();
    let handle = registry.snapshot().find(&owner(), PageScope::Global, "read").unwrap();
    let mut leases: Vec<_> = (0..MAX_CONCURRENT_LARGE_DOCUMENTS).map(|_| handle.admit().unwrap()).collect();
    assert!(handle.admit().is_err());
    let mut first = leases.pop().unwrap();
    let body = first.render(context()).await.unwrap();
    assert!(body.html().len() > MAX_PAGE_BODY_BYTES);
    let (operation, _) = first.into_parts();
    assert!(handle.admit().is_err());
    drop(operation);
    drop(handle.admit().unwrap());
    drop(leases);
    assert_eq!(scope.in_flight(), 0);
}
#[tokio::test]
async fn mutation_success_uses_declared_document_budget_and_retains_large_permit() {
    let descriptor = MutationDescriptor::new(
        "write",
        PageScope::Global,
        "write",
        MutationAudience::SignedIn,
        Duration::from_secs(1),
        FormSchema::new(Vec::new()).unwrap(),
    )
    .unwrap()
    .with_document()
    .with_body_limit(MAX_LARGE_PAGE_BODY_BYTES)
    .unwrap();
    let registry = MutationRegistry::new();
    let scope = scope();
    let mutation = compiled_mutation(descriptor, |_, _| async {
        Ok(MutationOutcome::Success(CompiledPage::new("Issued", Document { value: "&".repeat(MAX_LARGE_FORM_FIELD_BYTES) })?))
    });
    registry.stage(owner(), &scope, scope.generation(), vec![mutation]).unwrap().publish().unwrap();
    let handle = registry.snapshot().find(&owner(), PageScope::Global, "write").unwrap();
    let mut leases: Vec<_> = (0..MAX_CONCURRENT_LARGE_DOCUMENTS).map(|_| handle.admit().unwrap()).collect();
    assert!(handle.admit().is_err());
    let form = FormAction::new("write", "/-/plugins/example.documents/write", "proof").unwrap();
    let context = MutationContext::new(PageActor { account_id: 1, is_admin: false }, None, LocaleSnapshot::english(0), form);
    let result = leases.pop().unwrap().execute(context, FormData::new(Vec::new()).unwrap()).await.unwrap();
    let (result, operation, _) = result.into_parts();
    let MutationResponse::Success(body) = result else { panic!("compiled success expected") };
    assert!(body.html().len() > MAX_PAGE_BODY_BYTES);
    assert!(handle.admit().is_err());
    drop(operation);
    drop(leases);
    registry.wait_for_idle().await;
    assert_eq!(scope.in_flight(), 0);
}

#[test]
fn document_appearance_is_bounded_validated_and_preserved_in_owned_form_metadata() {
    assert!(FormActions::default().appearance().is_none());
    for theme in ["dark", "light-2", &"a".repeat(128)] {
        let appearance = PageAppearance::new(theme, true).unwrap();
        let mut forms = FormActions::default();
        forms.set_appearance(appearance);
        let owned = forms.clone();
        assert_eq!(owned.appearance().unwrap().theme(), theme);
        assert!(owned.appearance().unwrap().has_unread_notifications());
    }
    for theme in ["", "Dark", "dark mode", "dark\n", "<script>", "é", &"a".repeat(129)] {
        assert!(PageAppearance::new(theme, false).is_err(), "{theme:?}");
    }
    assert!(!PageAppearance::new("dark", false).unwrap().has_unread_notifications());
}
