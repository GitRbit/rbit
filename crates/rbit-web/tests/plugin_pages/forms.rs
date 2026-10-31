//! Real browser mutations keep host authentication, proofs and typed Askama output.

use super::*;
use rbit_services::plugin_pages::{
    FormAction, FormField, FormFieldKind, FormReference, FormSchema, FormValues, MutationAudience, MutationContext, MutationDescriptor,
    MutationOutcome, MutationRegistration, MutationRouteRegistration, PageRouteRegistration, PluginRedirect, compiled_form_page,
    compiled_mutation,
};

const OWNER: &str = "example.forms";

#[derive(Template)]
#[template(
    source = "{% match form %}{% when Some with (form) %}<form method=\"post\" action=\"{{ form.action() }}\" hx-boost=\"false\"><input type=\"hidden\" name=\"_csrf\" value=\"{{ form.csrf() }}\"><input name=\"title\" value=\"\"><button>Save</button></form>{% when None %}<p>No writable form.</p>{% endmatch %}{% if !message.is_empty() %}<p role=\"alert\">{{ message }}</p>{% endif %}",
    ext = "html"
)]
struct FormView {
    form: Option<FormAction>,
    message: String,
}

fn schema() -> FormSchema {
    FormSchema::new(vec![
        FormField { name: "title".into(), kind: FormFieldKind::Text { max_bytes: 80, multiline: false }, required: true },
        FormField { name: "count".into(), kind: FormFieldKind::Integer { min: 0, max: 10 }, required: false },
        FormField { name: "enabled".into(), kind: FormFieldKind::Boolean, required: false },
    ])
    .unwrap()
}

fn mutation(
    audience: MutationAudience,
    calls: &Arc<AtomicUsize>,
    observed: &Arc<Mutex<Vec<(MutationContext, FormValues)>>>,
) -> MutationRegistration {
    let calls = Arc::clone(calls);
    let observed = Arc::clone(observed);
    let page_scope = mutation_scope(audience);
    compiled_mutation(
        MutationDescriptor::new("save", page_scope, "save", audience, Duration::from_secs(1), schema()).unwrap(),
        move |context, values| {
            let calls = Arc::clone(&calls);
            let observed = Arc::clone(&observed);
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                observed.lock().unwrap().push((context.clone(), values.clone()));
                if values.text("title") == Some("invalid") {
                    return Ok(MutationOutcome::Invalid(CompiledPage::new(
                        "Check the form",
                        FormView { form: Some(context.form), message: "Invalid <script>reason</script>".into() },
                    )?));
                }
                Ok(MutationOutcome::Redirect(PluginRedirect::new("form")?))
            }
        },
    )
}

fn mutation_scope(audience: MutationAudience) -> PageScope {
    match audience {
        MutationAudience::SignedIn | MutationAudience::SiteAdmin => PageScope::Global,
        MutationAudience::RepositoryWriter | MutationAudience::RepositoryAdmin => PageScope::Repository,
    }
}

struct FormFixture {
    base: Fixture,
    scope: PluginScope,
    page_scope: PageScope,
    audience: MutationAudience,
    pages: PageRouteRegistration,
    mutations: MutationRouteRegistration,
    calls: Arc<AtomicUsize>,
    observed: Arc<Mutex<Vec<(MutationContext, FormValues)>>>,
}
impl FormFixture {
    async fn new(audience: MutationAudience) -> Self {
        let base = Fixture::new().await;
        let scope = super::scope(8300);
        let calls = Arc::default();
        let observed = Arc::default();
        let page_scope = mutation_scope(audience);
        let descriptor = PageDescriptor::new(
            "form",
            page_scope,
            "form",
            if page_scope == PageScope::Global { PageAudience::Public } else { PageAudience::RepositoryReader },
            Duration::from_secs(1),
        )
        .unwrap()
        .with_forms(vec![FormReference::new("save", "save").unwrap()])
        .unwrap();
        let pages = base
            .services
            .page_registry()
            .stage(
                OWNER.parse().unwrap(),
                &scope,
                scope.generation(),
                vec![compiled_form_page(descriptor, |_, actions| async move {
                    CompiledPage::new("Plugin form", FormView { form: actions.get("save").cloned(), message: String::new() })
                })],
            )
            .unwrap();
        let mutations = base
            .services
            .mutation_registry()
            .stage(OWNER.parse().unwrap(), &scope, scope.generation(), vec![mutation(audience, &calls, &observed)])
            .unwrap();
        pages.publish().unwrap();
        mutations.publish().unwrap();
        Self { base, scope, page_scope, audience, pages, mutations, calls, observed }
    }
    fn path(&self, repository: &str, leaf: &str) -> String {
        match self.page_scope {
            PageScope::Global => format!("/-/plugins/{OWNER}/{leaf}"),
            PageScope::Repository => format!("/alice/{repository}/-/plugins/{OWNER}/{leaf}"),
        }
    }
    async fn proof(&self, repository: &str, cookie: &str) -> String {
        let (status, headers, html) = self.base.get(&self.path(repository, "form"), Some(cookie)).await;
        assert_eq!(status, StatusCode::OK);
        assert_host_headers(&headers);
        assert!(html.contains("hx-history=\"false\" hx-boost=\"false\""));
        proof(&html).expect("authorized active form contains a host proof")
    }
}

fn proof(html: &str) -> Option<String> {
    Some(html.split("name=\"_csrf\" value=\"").nth(1)?.split('"').next()?.to_owned())
}
fn form_body(proof: &str, title: &str) -> String {
    format!("_csrf={proof}&title={}", percent_encoding::utf8_percent_encode(title, percent_encoding::NON_ALPHANUMERIC))
}
fn post_request(path: &str, cookie: Option<&str>, body: Body) -> Request<Body> {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::HOST, "localhost:3000")
        .header(header::ORIGIN, "https://localhost:3000")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    request.body(body).unwrap()
}
async fn post(fixture: &Fixture, path: &str, cookie: Option<&str>, body: &str, headers: &[(&str, &str)]) -> axum::response::Response {
    let mut request = post_request(path, cookie, Body::from(body.to_owned()));
    for (name, value) in headers {
        request.headers_mut().insert(axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(), value.parse().unwrap());
    }
    fixture.router.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn runtime_form_mutations_use_host_proofs_typed_fields_and_safe_prg_or_askama_errors() {
    let fixture = FormFixture::new(MutationAudience::SignedIn).await;
    let path = fixture.path("rocket", "save");
    let token = fixture.proof("rocket", &fixture.base.user_cookie).await;
    let body = format!("{}&count=7&enabled=true", form_body(&token, "<script>stored text</script>"));
    let response = post(&fixture.base, &path, Some(&fixture.base.user_cookie), &body, &[]).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], fixture.path("rocket", "form"));
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    drop(response);
    {
        let observed = fixture.observed.lock().unwrap();
        assert_eq!(observed[0].0.actor.account_id, fixture.base.user.id().0);
        assert!(observed[0].0.repository.is_none());
        assert_eq!(observed[0].1.text("title"), Some("<script>stored text</script>"));
        assert_eq!(observed[0].1.integer("count"), Some(7));
        assert_eq!(observed[0].1.boolean("enabled"), Some(true));
    }
    let response =
        post(&fixture.base, &path, Some(&fixture.base.user_cookie), &form_body(&token, "invalid"), &[("HX-Request", "true")]).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_host_headers(response.headers());
    let html = String::from_utf8(to_bytes(response.into_body(), 1024 * 1024).await.unwrap().to_vec()).unwrap();
    assert!(html.contains("<html lang=\"en\" dir=\"ltr\""));
    assert!(html.contains("Invalid &#60;script&#62;reason&#60;/script&#62;"));
    assert!(!html.contains("<script>reason</script>"));
    assert!(html.contains("hx-history=\"false\" hx-boost=\"false\""));
    assert!(proof(&html).is_some());
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 2);
    assert!(fixture.pages.unpublish());
    assert_eq!(fixture.base.get(&fixture.path("rocket", "form"), Some(&fixture.base.user_cookie)).await.0, StatusCode::NOT_FOUND);
    fixture.base.services.shutdown().await;
}

#[tokio::test]
async fn runtime_form_mutations_reject_untrusted_transport_and_invalid_fields_before_callbacks() {
    let fixture = FormFixture::new(MutationAudience::SignedIn).await;
    let path = fixture.path("rocket", "save");
    let token = fixture.proof("rocket", &fixture.base.user_cookie).await;
    for (body, extra, status) in [
        ("title=no-proof".to_owned(), vec![], StatusCode::FORBIDDEN),
        (form_body("wrong", "title"), vec![], StatusCode::FORBIDDEN),
        (form_body(&token, "title"), vec![("Origin", "https://evil.example"), ("Sec-Fetch-Site", "same-origin")], StatusCode::FORBIDDEN),
        (form_body(&token, "title"), vec![("Sec-Fetch-Site", "cross-site")], StatusCode::FORBIDDEN),
        (form_body(&token, "title"), vec![("Origin", "http://localhost:3000")], StatusCode::FORBIDDEN),
        (form_body(&token, "title"), vec![("Content-Type", "text/plain")], StatusCode::UNSUPPORTED_MEDIA_TYPE),
        (form_body(&token, "title"), vec![("Content-Type", "application/json")], StatusCode::UNSUPPORTED_MEDIA_TYPE),
        (form_body(&token, "title"), vec![("Content-Encoding", "gzip")], StatusCode::UNSUPPORTED_MEDIA_TYPE),
        (format!("{}&%74itle=duplicate", form_body(&token, "title")), vec![], StatusCode::BAD_REQUEST),
        (format!("{}&unknown=yes", form_body(&token, "title")), vec![], StatusCode::UNPROCESSABLE_ENTITY),
        (format!("{}&count=11", form_body(&token, "title")), vec![], StatusCode::UNPROCESSABLE_ENTITY),
        (format!("{}&enabled=on", form_body(&token, "title")), vec![], StatusCode::UNPROCESSABLE_ENTITY),
        (format!("_csrf={token}&title=%FF"), vec![], StatusCode::BAD_REQUEST),
        (form_body(&token, "line\ncontrol"), vec![], StatusCode::UNPROCESSABLE_ENTITY),
        (form_body(&token, &"x".repeat(81)), vec![], StatusCode::UNPROCESSABLE_ENTITY),
        (format!("_csrf={token}&title={}", "x".repeat(32 * 1024)), vec![], StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        let response = post(&fixture.base, &path, Some(&fixture.base.user_cookie), &body, &extra).await;
        assert_eq!(response.status(), status, "{status} for {extra:?}");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    }
    let mut request = post_request(&path, Some(&fixture.base.user_cookie), Body::from(form_body(&token, "token-without-origin")));
    request.headers_mut().remove(header::ORIGIN);
    assert_eq!(fixture.base.router.clone().oneshot(request).await.unwrap().status(), StatusCode::SEE_OTHER);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1, "missing Origin is accepted only with a valid session-bound proof");
    fixture.base.services.shutdown().await;
}

async fn wait_for_started(started: &AtomicUsize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while started.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
async fn wait_for_idle(scope: &PluginScope) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while scope.in_flight() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn runtime_form_mutations_bound_stalled_uploads_and_cancel_them_on_generation_close() {
    for generation_close in [false, true] {
        let fixture = FormFixture::new(MutationAudience::SignedIn).await;
        let started = Arc::new(AtomicUsize::new(0));
        let marker = Arc::clone(&started);
        let body = Body::from_stream(futures_util::stream::poll_fn(move |_| {
            marker.store(1, Ordering::SeqCst);
            std::task::Poll::<Option<Result<axum::body::Bytes, std::io::Error>>>::Pending
        }));
        let request = post_request(&fixture.path("rocket", "save"), Some(&fixture.base.user_cookie), body);
        let router = fixture.base.router.clone();
        let response = tokio::spawn(async move { router.oneshot(request).await.unwrap() });
        wait_for_started(&started).await;
        assert_eq!(fixture.scope.in_flight(), 1);
        if generation_close {
            fixture.scope.quiesce();
        }
        let response = tokio::time::timeout(Duration::from_secs(2), response).await.unwrap().unwrap();
        assert_eq!(response.status(), if generation_close { StatusCode::SERVICE_UNAVAILABLE } else { StatusCode::REQUEST_TIMEOUT });
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
        wait_for_idle(&fixture.scope).await;
        fixture.base.services.shutdown().await;
    }
}

#[tokio::test]
async fn runtime_form_mutations_keep_real_commits_tracked_after_http_abort_or_timeout() {
    for caller_abort in [false, true] {
        let fixture = FormFixture::new(MutationAudience::SignedIn).await;
        let started = Arc::new(AtomicUsize::new(0));
        let committed = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(tokio::sync::Notify::new());
        let callback = {
            let started = Arc::clone(&started);
            let committed = Arc::clone(&committed);
            let release = Arc::clone(&release);
            compiled_mutation(
                MutationDescriptor::new("save", PageScope::Global, "save", fixture.audience, Duration::from_millis(30), schema()).unwrap(),
                move |_, _| {
                    let started = Arc::clone(&started);
                    let committed = Arc::clone(&committed);
                    let release = Arc::clone(&release);
                    async move {
                        started.fetch_add(1, Ordering::SeqCst);
                        // Models a native transaction already committing: cancellation cannot roll it back.
                        release.notified().await;
                        committed.fetch_add(1, Ordering::SeqCst);
                        Ok(MutationOutcome::<FormView>::Redirect(PluginRedirect::new("form")?))
                    }
                },
            )
        };
        fixture.mutations.replace(vec![callback]).unwrap();
        let token = fixture.proof("rocket", &fixture.base.user_cookie).await;
        let request =
            post_request(&fixture.path("rocket", "save"), Some(&fixture.base.user_cookie), Body::from(form_body(&token, "commit")));
        let router = fixture.base.router.clone();
        let response = tokio::spawn(async move { router.oneshot(request).await.unwrap() });
        wait_for_started(&started).await;
        if caller_abort {
            response.abort();
            assert!(response.await.unwrap_err().is_cancelled());
        } else {
            let response = tokio::time::timeout(Duration::from_secs(2), response).await.unwrap().unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            assert_eq!(
                to_bytes(response.into_body(), 1024).await.unwrap(),
                "Plugin mutation outcome is unknown. Do not retry automatically."
            );
        }
        assert_eq!(fixture.scope.in_flight(), 1, "HTTP completion cannot release a still-running native commit");
        assert_eq!(committed.load(Ordering::SeqCst), 0);
        assert!(fixture.scope.dispose(Duration::ZERO).await.is_err(), "the lifecycle cannot claim a pending commit drained");
        release.notify_one();
        wait_for_idle(&fixture.scope).await;
        assert_eq!(committed.load(Ordering::SeqCst), 1);
        fixture.base.services.shutdown().await;
    }
}
