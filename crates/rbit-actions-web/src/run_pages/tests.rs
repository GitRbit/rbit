use super::*;
use rbit_messages::LocaleSnapshot;
use rbit_plugin_api::{Generation, PluginId};
use rbit_plugin_runtime::PluginScope;
use rbit_plugin_web::{PagePresentation, PageRegistry, PageRepository};
use std::sync::{Arc, Mutex, Weak};

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Connect(RunsPageKind),
    Render(RunsQuery, Option<String>),
}
struct State {
    allowed: bool,
    calls: Arc<Mutex<Vec<Call>>>,
}
#[derive(Clone)]
struct Host(Weak<State>);
struct Request(Arc<State>);
#[derive(Template)]
#[template(source = "<!doctype html><html><body><main>Existing host document</main></body></html>", ext = "html")]
struct Document;
impl RunsHost for Host {
    type Request = Request;
    async fn connect(&self, kind: RunsPageKind, _: PageContext, _: FormActions) -> Result<Request, PageError> {
        let state = self.0.upgrade().ok_or(PageError::Unavailable)?;
        state.calls.lock().unwrap().push(Call::Connect(kind));
        if !state.allowed {
            return Err(PageError::Unavailable);
        }
        Ok(Request(state))
    }
}
impl RunsRequest for Request {
    type Document = Document;
    async fn render(self, query: RunsQuery, workflow: Option<&str>) -> Result<CompiledPage<Document>, PageError> {
        self.0.calls.lock().unwrap().push(Call::Render(query, workflow.map(str::to_owned)));
        CompiledPage::new("Actions", Document)
    }
}
fn host(allowed: bool) -> (Arc<State>, Host, Arc<Mutex<Vec<Call>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(State { allowed, calls: Arc::clone(&calls) });
    let host = Host(Arc::downgrade(&state));
    (state, host, calls)
}
fn registry(host: Host) -> (PageRegistry, PluginScope, PluginId) {
    let registry = PageRegistry::new();
    let scope = PluginScope::new(Generation::INITIAL);
    let owner: PluginId = "rbit.actions".parse().unwrap();
    registry.stage(owner.clone(), &scope, scope.generation(), pages(host).unwrap().into()).unwrap().publish().unwrap();
    (registry, scope, owner)
}
fn context() -> PageContext {
    PageContext {
        actor: None,
        repository: Some(PageRepository { repository_id: 1, owner: "owner".into(), name: "repo".into(), can_admin: false }),
        locale: LocaleSnapshot::english(0),
    }
}
fn query(values: &[(&str, &str)]) -> PageQuery {
    PageQuery::new(values.iter().map(|(key, value)| ((*key).into(), (*value).into())).collect()).unwrap()
}
#[test]
fn exactly_two_descriptors_preserve_native_scope_forms_query_deadline_and_document() {
    let (_state, host, _) = host(true);
    let pages = pages(host).unwrap();
    for (page, expected) in pages.iter().zip(["runs", "workflow"]) {
        let descriptor = page.descriptor();
        assert_eq!(descriptor.id(), expected);
        assert_eq!(descriptor.path(), expected);
        assert_eq!(descriptor.scope(), PageScope::Repository);
        assert_eq!(descriptor.audience(), PageAudience::RepositoryReader);
        assert_eq!(descriptor.deadline(), Duration::from_secs(1));
        assert_eq!(descriptor.presentation(), PagePresentation::Document);
        assert_eq!(
            descriptor.forms(),
            [FormReference::new("dispatch", "dispatch").unwrap(), FormReference::new("switch", "switch").unwrap()]
        );
        for key in ["workflow", "branch", "actor", "event", "status", "before", "ref"] {
            assert!(descriptor.query().allows(key));
        }
        assert!(!descriptor.query().allows("extra"));
    }
}
#[test]
fn query_projection_borrows_workflow_and_preserves_existing_permissive_machine_filter_parsing() {
    let values = query(&[
        ("workflow", ".rbit/workflows/ci.yml"),
        ("branch", "feature/a&b"),
        ("event", "unknown-event"),
        ("status", "unknown-status"),
        ("before", "-1"),
        ("ref", "refs/tags/v1"),
    ]);
    let (filters, workflow) = parse(&values, RunsPageKind::Workflow).unwrap();
    assert_eq!(workflow.unwrap().as_ptr(), values.get("workflow").unwrap().as_ptr());
    assert_eq!(
        filters,
        RunsQuery {
            branch: "feature/a&b".into(),
            event: "unknown-event".into(),
            status: "unknown-status".into(),
            before: Some(-1),
            reference: "refs/tags/v1".into(),
            ..RunsQuery::default()
        }
    );
    assert!(parse(&query(&[("before", "not-an-integer")]), RunsPageKind::Runs).is_err());
    assert!(parse(&PageQuery::default(), RunsPageKind::Workflow).is_err());
    assert_eq!(parse(&PageQuery::default(), RunsPageKind::Runs).unwrap(), (RunsQuery::default(), None));
}
#[tokio::test]
async fn connection_precedes_query_projection_and_host_errors_precede_malformed_before() {
    let (_state, host, calls) = host(false);
    let (registry, _scope, owner) = registry(host);
    let mut route = registry.snapshot().find(&owner, PageScope::Repository, "workflow").unwrap().admit().unwrap();
    let result = route.render_with_request(context(), FormActions::default(), query(&[("before", "bad")])).await;
    assert!(matches!(result, Err(PageError::Unavailable)));
    assert_eq!(*calls.lock().unwrap(), [Call::Connect(RunsPageKind::Workflow)]);
}
#[tokio::test]
async fn malformed_filters_never_render_and_valid_filters_reach_one_host_document() {
    let (_state, host, calls) = host(true);
    let (registry, scope, owner) = registry(host);
    let handle = registry.snapshot().find(&owner, PageScope::Repository, "workflow").unwrap();
    let mut route = handle.admit().unwrap();
    assert!(matches!(
        route.render_with_request(context(), FormActions::default(), query(&[("before", "bad")])).await,
        Err(PageError::Forbidden)
    ));
    assert_eq!(*calls.lock().unwrap(), [Call::Connect(RunsPageKind::Workflow)]);
    calls.lock().unwrap().clear();
    let body = route
        .render_with_request(
            context(),
            FormActions::default(),
            query(&[("workflow", ".rbit/workflows/ci.yml"), ("actor", "person"), ("before", "42")]),
        )
        .await
        .unwrap();
    assert!(body.html().starts_with("<!doctype html>"));
    assert_eq!(
        *calls.lock().unwrap(),
        [
            Call::Connect(RunsPageKind::Workflow),
            Call::Render(
                RunsQuery { actor: "person".into(), before: Some(42), ..RunsQuery::default() },
                Some(".rbit/workflows/ci.yml".into())
            )
        ]
    );
    drop(route);
    scope.quiesce();
    assert!(handle.admit().is_err());
}
#[tokio::test]
async fn blueprints_keep_only_weak_host_and_framework_rejects_unknown_query_before_connect() {
    let (state, host, calls) = host(true);
    let weak = Arc::downgrade(&state);
    let (registry, _scope, owner) = registry(host);
    let mut route = registry.snapshot().find(&owner, PageScope::Repository, "runs").unwrap().admit().unwrap();
    assert!(route.render_with_request(context(), FormActions::default(), query(&[("extra", "value")])).await.is_err());
    assert_eq!(*calls.lock().unwrap(), Vec::<Call>::new());
    drop(state);
    assert!(weak.upgrade().is_none());
    assert!(matches!(route.render(context()).await, Err(PageError::Unavailable)));
}
