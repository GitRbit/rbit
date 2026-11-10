//! Real runner-backed job views: bounded output, native controls and localized polls.
use super::*;
use axum::{Extension, Router};
use rbit_localization::{Locale, LocaleResolver};
use rbit_storage::{Work, count_work};
use std::fmt::Write as _;

async fn localized(router: &Router, path: &str, cookie: &str, language: &str, htmx: bool) -> (String, axum::http::HeaderMap, Work) {
    let mut request = Request::builder()
        .uri(path)
        .header(header::HOST, "localhost:3000")
        .header(header::COOKIE, cookie)
        .header(header::ACCEPT_LANGUAGE, language);
    if htmx {
        let target = if path.contains("/logs?step=") { "step" } else { "job" };
        request = request.header("HX-Request", "true").header("HX-Target", target);
    }
    let ((body, headers), work) = count_work(async {
        let response = router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers().clone();
        let body = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
        (body, headers)
    })
    .await;
    (body, headers, work)
}

#[tokio::test]
async fn presentation_locale_preserves_raw_redirects_and_the_default_error_catalog() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let path = format!("/alice/rocket/actions/runs/{}/jobs/{}", run.number, job.id.0);
    let task = spec(&poll(&fixture, &credential, Some("raw-locale")).await.1).task_id.0;
    logs(&fixture, &credential, task, 0, "raw <log>\n").await;
    let token = fixture.services.accounts.start_session(&fixture.alice).await.unwrap();
    let router = fixture.router.clone().layer(Extension(LocaleResolver::new(Locale::English, true).unwrap()));
    for (suffix, expected) in [("/logs", StatusCode::SEE_OTHER), ("/logs?step=999", StatusCode::NOT_FOUND), ("/logs/raw", StatusCode::OK)] {
        let request = Request::builder()
            .uri(format!("{path}{suffix}"))
            .header(header::HOST, "localhost:3000")
            .header(header::COOKIE, format!("rbit_session={}", token.encode()))
            .header(header::ACCEPT_LANGUAGE, "ar-XB")
            .body(Body::empty())
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), expected);
        if expected == StatusCode::NOT_FOUND {
            assert_eq!(response.headers()[header::CONTENT_LANGUAGE], "en", "HTML errors use the shared shell's default catalog");
            assert_eq!(response.headers()["rbit-catalog-key"], "1.en");
            assert!(response.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/html"));
        } else {
            assert!(!response.headers().contains_key(header::CONTENT_LANGUAGE));
            assert!(!response.headers().contains_key("rbit-catalog-key"));
        }
        assert!(!response.headers().contains_key("hx-refresh"));
        if suffix == "/logs" {
            assert_eq!(response.headers()[header::LOCATION], path.as_str());
        }
        if suffix == "/logs/raw" {
            assert_eq!(response.headers()[header::CONTENT_TYPE], "text/plain; charset=utf-8");
            assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], "sandbox; default-src 'none'");
            assert_eq!(to_bytes(response.into_body(), usize::MAX).await.unwrap().as_ref(), b"raw <log>\n");
        }
    }
}

#[tokio::test]
async fn bounded_job_previews_keep_step_anchors_and_the_full_download() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let mut build = template("build", &["linux"], &[]);
    build.name = "Build / matrix <script> & long name".into();
    build.steps[0].name = Some("Compile <img src=x> & dependencies".into());
    let run = seed(&fixture, &[payload(&build)]).await;
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let path = format!("/alice/rocket/actions/runs/{}/jobs/{}", run.number, job.id.0);
    let task = spec(&poll(&fixture, &credential, Some("presentation")).await.1).task_id.0;
    let mut output = String::with_capacity(1205 * 9);
    for number in 1..=1205 {
        writeln!(&mut output, "LOG-{number:04}").unwrap();
    }
    logs(&fixture, &credential, task, 0, &output).await;
    state(
        &fixture,
        &credential,
        task,
        &json!({
            "steps": [step(0, "failure", 0, 1205), step(1, "running", 1205, 0)], "outputs": {}, "result": null,
        }),
    )
    .await;
    let (status, _, html) = get(&fixture, &path, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Job details") && html.contains("Steps and logs") && html.contains("2 steps"));
    assert!(!html.contains("<script>") && !html.contains("<img src=x>"));
    assert!(html.contains("Build / matrix") && html.contains("dependencies"));
    assert!(html.contains("LOG-0206") && html.contains("LOG-1205") && !html.contains("LOG-0001"));
    assert_eq!(html.matches("class=\"log-line\"").count(), 1000);
    assert!(html.contains("id=\"step-0-line-206\"") && html.contains("href=\"#step-0-line-1205\""), "rows keep their step line anchors");
    assert!(html.contains("data-line=\"206\"") && html.contains("data-line=\"1205\""));
    assert!(html.contains("Earlier lines are left out.") && html.contains(&format!("{path}/logs?step=0")));
    assert!(html.contains("<details open data-step=\"1\" data-state=\"running\">"));
    let (_, _, longer) = get(&fixture, &format!("{path}/logs?step=0"), false).await;
    assert!(longer.contains("LOG-0001") && longer.contains("LOG-1205"));
    let (_, _, raw) = get(&fixture, &format!("{path}/logs/raw"), false).await;
    assert_eq!(raw, output);
}

#[tokio::test]
async fn selected_job_and_step_roots_keep_locale_privacy_and_the_same_storage_work() {
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let path = format!("/alice/rocket/actions/runs/{}/jobs/{}", run.number, job.id.0);
    let task = spec(&poll(&fixture, &credential, Some("locale")).await.1).task_id.0;
    logs(&fixture, &credential, task, 0, "unchanged <log>\n").await;
    state(&fixture, &credential, task, &json!({"steps": [step(0, "running", 0, 0)], "outputs": {}, "result": null})).await;
    let token = fixture.services.accounts.start_session(&fixture.alice).await.unwrap();
    let cookie = format!("rbit_session={}", token.encode());
    let router = fixture.router.clone().layer(Extension(LocaleResolver::new(Locale::English, true).unwrap()));
    for (url, selected) in [(path.clone(), "job"), (format!("{path}/logs?step=0"), "step")] {
        localized(&router, &url, &cookie, "en", false).await;
        let (_, _, baseline) = localized(&router, &url, &cookie, "en", false).await;
        assert!(baseline.statements > 0 && baseline.acquisitions > 0);
        for language in ["en", "ar-XB", "en-XA"] {
            for htmx in [false, true] {
                let (body, headers, work) = localized(&router, &url, &cookie, language, htmx).await;
                assert_eq!(work.statements, baseline.statements);
                assert_eq!(work.acquisitions, baseline.acquisitions);
                assert_eq!(headers[header::CONTENT_LANGUAGE], language);
                assert_eq!(headers[header::CACHE_CONTROL], "private, no-cache");
                assert!(headers.contains_key(header::CONTENT_SECURITY_POLICY));
                let vary = headers.get_all(header::VARY).iter().map(|value| value.to_str().unwrap()).collect::<Vec<_>>().join(",");
                assert!(vary.contains("Accept-Language") && vary.contains("Cookie"));
                let root_start = format!("<section id=\"{selected}\"");
                let root = body.split(&root_start).nth(1).unwrap().split('>').next().unwrap();
                assert!(root.contains(&format!("lang=\"{language}\"")));
                let direction = if language == "ar-XB" { "rtl" } else { "ltr" };
                assert!(root.contains(&format!("dir=\"{direction}\"")));
                let polls = if selected == "job" { "data-live-url=" } else { "hx-trigger=\"every 3s\"" };
                assert!(root.contains(polls), "{root}");
                assert!(body.contains("unchanged &lt;log&gt;") && !body.contains("unchanged <log>"));
            }
        }
    }
}

#[tokio::test]
async fn finished_job_rerun_remains_a_writer_only_native_post() {
    use rbit_core::membership::Permission;
    use rbit_services::SignUp;
    let fixture = Fixture::new().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let path = format!("/alice/rocket/actions/runs/{}/jobs/{}", run.number, job.id.0);
    let task = spec(&poll(&fixture, &credential, Some("finished")).await.1).task_id.0;
    state(&fixture, &credential, task, &json!({"steps": [step(0, "success", 0, 0)], "outputs": {}, "result": "success"})).await;
    let (_, _, owner) = get(&fixture, &path, false).await;
    assert!(owner.contains(&format!("method=\"post\" action=\"{path}/rerun\"")));
    assert!(!owner.contains("data-live-url"), "a finished job is not tailed");
    let reader = fixture
        .services
        .accounts
        .create_user(
            SignUp { username: "bob".into(), email: "bob@example.com".into(), password: "correct horse battery".into() },
            Some(false),
        )
        .await
        .unwrap();
    fixture.services.collaborators.add(&fixture.alice, &fixture.repo().await, "bob", Permission::Read).await.unwrap();
    let token = fixture.services.accounts.start_session(&reader).await.unwrap();
    let cookie = format!("rbit_session={}", token.encode());
    let (body, _, _) = localized(&fixture.router, &path, &cookie, "en", false).await;
    assert!(!body.contains(&format!("action=\"{path}/rerun\"")));
    assert!(body.contains(&format!("href=\"{path}/logs/raw\"")));
    let request = Request::builder()
        .method("POST")
        .uri(format!("{path}/rerun"))
        .header(header::HOST, "localhost:3000")
        .header(header::ORIGIN, "https://git.example.com")
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap();
    assert_eq!(fixture.router.clone().oneshot(request).await.unwrap().status(), StatusCode::FORBIDDEN);
}
