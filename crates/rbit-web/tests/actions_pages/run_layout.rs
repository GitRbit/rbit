//! The run layout: the run's navigation beside the run and job pages, its own refresh and
//! the run section's lean poll.
use super::*;

/// GET `path` as `cookie` with extra request headers; the status and body.
async fn get_with(fixture: &Fixture, path: &str, cookie: &str, headers: &[(&str, &str)]) -> (StatusCode, String) {
    let mut request = Request::builder().uri(path).header(header::HOST, "localhost:3000").header(header::COOKIE, cookie);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = fixture.router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn the_run_and_its_jobs_share_a_navigation_that_refreshes_while_the_run_runs() {
    let fixture = Fixture::new().await;
    fixture.database.actions_settings().set_enabled(fixture.repo_id().await, true, Timestamp::now()).await.unwrap();
    let run = fixture.run(".rbit/workflows/ci.yml", "Layout", RunEvent::Push).await;
    let jobs = fixture.database.actions_runs().jobs(run.id).await.unwrap();
    let url = format!("/alice/rocket/actions/runs/{}", run.number);

    let page = fixture.page(&url, &fixture.reader).await;
    assert!(page.contains(r#"id="run-sidebar-jobs""#) && page.contains("data-run-nav"));
    assert!(page.contains(&format!(r#"href="{url}" aria-current="page""#)), "the summary is the current page");
    for job in &jobs {
        assert!(page.matches(&format!(r#"href="{url}/jobs/{}""#, job.id.0)).count() >= 2, "listed beside the page and in it");
    }
    assert!(page.contains(r##"hx-select-oob="#run-sidebar-jobs""##), "the run section's poll refreshes the list");
    assert!(page.contains("/alice/rocket/blob/0123456789abcdef0123456789abcdef01234567/") && page.contains("Workflow file"));
    assert!(page.contains(r#"data-clock-ms="33m 44s""#), "ticking durations know the page's language");

    // The run section's poll gets the section and the list, without the page around them.
    let (status, live) = get_with(&fixture, &url, &fixture.reader, &[("hx-request", "true"), ("hx-target", "run")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(live.contains(r#"<section id="run""#) && live.contains(r#"id="run-sidebar-jobs""#));
    assert!(!live.contains("<html") && !live.contains("data-run-nav"));
    let (_, boosted) =
        get_with(&fixture, &url, &fixture.reader, &[("hx-request", "true"), ("hx-target", "run"), ("hx-boosted", "true")]).await;
    assert!(boosted.contains("<html"), "a navigation gets the page");

    let job = &jobs[1];
    let job_page = fixture.page(&format!("{url}/jobs/{}", job.id.0), &fixture.reader).await;
    let poll = format!("{url}/sidebar?job={}", job.id.0);
    assert!(job_page.contains(&format!(r#"hx-get="{poll}""#)), "a job page's list refreshes itself");
    assert!(job_page.contains(&format!(r#"href="{url}/jobs/{}" title="test" aria-current="page""#, job.id.0)));
    assert!(job_page.contains(&format!(r#"href="{url}""#)) && job_page.contains("Summary"));

    let (status, fragment) = get_with(&fixture, &poll, &fixture.reader, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(fragment.trim_start().starts_with(r#"<ul id="run-sidebar-jobs""#), "{fragment}");
    assert!(fragment.contains(r#"hx-trigger="every 3s""#) && fragment.contains(r#"aria-current="page""#));
    let (status, _) = get_with(&fixture, &format!("/alice/rocket/actions/runs/{}/sidebar", run.number + 1), &fixture.reader, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A finished run's list stays still.
    let repo = fixture.repo_id().await;
    fixture.database.actions_runs().cancel_run(repo, run.id, Timestamp::now()).await.unwrap();
    let (_, fragment) = get_with(&fixture, &poll, &fixture.reader, &[]).await;
    assert!(!fragment.contains("hx-trigger"), "{fragment}");
}
