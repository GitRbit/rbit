//! Real router fixtures for the run layout: controls remain server-authorized.
use super::*;
use axum::Extension;
use rbit_localization::{Locale, LocaleResolver};

struct Seed<'a> {
    title: &'a str,
    jobs: &'a [NewJob<'a>],
    start: RunStart,
    trusted: bool,
    error: Option<&'a str>,
}

fn build_job(labels: &[String]) -> NewJob<'_> {
    NewJob {
        key: "build",
        name: "build",
        matrix: None,
        needs: &[],
        runs_on: labels,
        permissions: None,
        payload: b"{}",
        condition: false,
        max_parallel: None,
        group: None,
        environment: None,
    }
}

async fn seed(fixture: &Fixture, input: Seed<'_>) -> rbit_core::actions::Run {
    fixture.database.actions_settings().set_enabled(fixture.repo_id().await, true, Timestamp::now()).await.unwrap();
    fixture
        .database
        .actions_runs()
        .create_run(&NewRun {
            repo: fixture.repo_id().await,
            workflow_path: ".rbit/workflows/ci.yml",
            workflow_name: "CI <matrix>",
            title: input.title,
            event: RunEvent::Push,
            reference: "refs/heads/feature/مرحبا",
            sha: "0123456789abcdef0123456789abcdef01234567",
            actor: Some(fixture.alice.id()),
            trigger_actor: Some(fixture.alice.id()),
            pull_number: None,
            is_fork: !input.trusted,
            trusted: input.trusted,
            concurrency_group: None,
            cancel_in_progress: false,
            event_payload: "{}",
            dispatch_inputs: None,
            start: input.start,
            jobs: input.jobs,
            error: input.error,
            now: Timestamp::now(),
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn matrix_run_keeps_real_links_polling_post_controls_and_escaped_data() {
    let fixture = Fixture::new().await;
    let labels = vec!["windows-latest".to_owned()];
    let needs = vec!["build".to_owned()];
    let matrix = NewJob {
        key: "test",
        name: "Tests · Windows / stable <script> & long matrix name",
        matrix: Some(r#"{"os":"windows-latest","rust":"stable"}"#),
        needs: &needs,
        runs_on: &labels,
        permissions: None,
        payload: b"{}",
        condition: false,
        max_parallel: None,
        group: None,
        environment: None,
    };
    let jobs = [NewJob { key: "build", name: "build", needs: &[], matrix: None, ..matrix.clone() }, matrix];
    let run = seed(
        &fixture,
        Seed { title: "Fix <img src=x onerror=alert(1)> & details", jobs: &jobs, start: RunStart::Runnable, trusted: true, error: None },
    )
    .await;
    let url = format!("/alice/rocket/actions/runs/{}", run.number);
    let owner = fixture.page(&url, &fixture.owner).await;
    assert!(owner.contains("Fix &#60;img src=x onerror=alert(1)&#62; &#38; details"));
    assert!(!owner.contains("<img src=x"));
    assert!(owner.contains("Tests · Windows / stable &#60;script&#62; &#38; long matrix name"));
    assert!(owner.contains("<dl ") && owner.contains("Run details"));
    assert!(owner.contains("<bdi dir=\"ltr\">0123456</bdi>"));
    assert!(owner.contains("feature/مرحبا"));
    assert!(owner.contains(&format!("hx-get=\"{url}\"")));
    assert!(owner.contains("hx-trigger=\"every 3s\""));
    assert!(owner.contains(&format!("method=\"post\" action=\"{url}/cancel\"")));
    let jobs = fixture.database.actions_runs().jobs(run.id).await.unwrap();
    for job in jobs {
        assert!(owner.contains(&format!("href=\"{url}/jobs/{}\"", job.id.0)));
    }
    let reader = fixture.page(&url, &fixture.reader).await;
    assert!(!reader.contains(&format!("action=\"{url}/cancel\"")));
    let (status, ..) = fixture.send(Method::POST, &format!("{url}/cancel"), &fixture.reader, "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn fork_approval_retains_native_post_fields_and_writer_only_permission() {
    let fixture = Fixture::new().await;
    let labels = vec!["ubuntu-latest".to_owned()];
    let jobs = [build_job(&labels)];
    let run =
        seed(&fixture, Seed { title: "Fork review", jobs: &jobs, start: RunStart::AwaitingApproval, trusted: false, error: None }).await;
    let url = format!("/alice/rocket/actions/runs/{}", run.number);
    let writer = fixture.page(&url, &fixture.owner).await;
    for needle in [
        "data-run-review=\"approval\"",
        "method=\"post\"",
        "name=\"trust_author\"",
        "Approve and run",
        "read-only token and without secrets or variables",
    ] {
        assert!(writer.contains(needle), "missing {needle}");
    }
    assert!(writer.contains(&format!("action=\"{url}/approve\"")));
    let reader = fixture.page(&url, &fixture.reader).await;
    assert!(!reader.contains("data-run-review=\"approval\""));
    assert!(!reader.contains(&format!("action=\"{url}/approve\"")));
    let (status, ..) = fixture.send(Method::POST, &format!("{url}/approve"), &fixture.reader, "trust_author=true").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, ..) = fixture.send(Method::GET, "/alice/missing/actions/runs/1", &fixture.reader, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn workflow_error_is_escaped_and_has_no_active_poll_or_fabricated_job_summary() {
    let fixture = Fixture::new().await;
    let run = seed(
        &fixture,
        Seed {
            title: "Invalid workflow",
            jobs: &[],
            start: RunStart::Runnable,
            trusted: true,
            error: Some("Invalid <script>alert('x')</script> workflow"),
        },
    )
    .await;
    let page = fixture.page(&format!("/alice/rocket/actions/runs/{}", run.number), &fixture.owner).await;
    assert!(page.contains("role=\"alert\""));
    assert!(page.contains("Invalid &#60;script&#62;"));
    assert!(!page.contains("Invalid <script>"));
    assert!(!page.contains("hx-trigger=\"every 3s\""));
    assert!(!page.contains("All checks passed") && !page.contains("One job needs attention"));
}

#[tokio::test]
async fn rtl_and_expanded_run_headers_use_existing_locale_negotiation_and_cache_dimensions() {
    let fixture = Fixture::new().await;
    let labels = vec!["ubuntu-latest".to_owned()];
    let jobs = [build_job(&labels)];
    let run = seed(&fixture, Seed { title: "A <safe> run", jobs: &jobs, start: RunStart::Runnable, trusted: true, error: None }).await;
    let url = format!("/alice/rocket/actions/runs/{}", run.number);
    let router = fixture.router.clone().layer(Extension(LocaleResolver::new(Locale::English, true).unwrap()));
    for language in ["ar-XB", "en-XA"] {
        for htmx in [false, true] {
            let mut request = Request::builder()
                .uri(&url)
                .header(header::HOST, "localhost:3000")
                .header(header::COOKIE, &fixture.reader)
                .header(header::ACCEPT_LANGUAGE, language);
            if htmx {
                request = request.header("HX-Request", "true").header("HX-Target", "run");
            }
            let response = router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CONTENT_LANGUAGE], language);
            let vary = response.headers().get_all(header::VARY).iter().map(|value| value.to_str().unwrap()).collect::<Vec<_>>().join(",");
            assert!(vary.contains("Accept-Language") && vary.contains("Cookie"));
            let body = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
            assert!(body.contains("A &#60;safe&#62; run"));
            if language == "ar-XB" {
                assert!(body.contains("dir=\"rtl\""));
            }
            assert!(body.contains("hx-trigger=\"every 3s\""));
        }
    }
}
