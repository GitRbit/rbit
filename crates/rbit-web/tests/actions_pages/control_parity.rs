//! Existing reader/writer control parity, retained alongside run presentation fixtures.
use super::*;

#[tokio::test]
async fn writers_cancel_and_run_again_and_readers_only_look() {
    let fixture = Fixture::new().await;
    fixture.database.actions_settings().set_enabled(fixture.repo_id().await, true, Timestamp::now()).await.unwrap();
    let run = fixture.run(".rbit/workflows/ci.yml", "Change", RunEvent::Push).await;
    let url = format!("/alice/rocket/actions/runs/{}", run.number);

    let page = fixture.page(&url, &fixture.owner).await;
    assert!(page.contains("needs build"));
    assert!(page.find(">build<").unwrap() < page.find(">test<").unwrap(), "jobs follow their needs");
    assert!(page.contains(&format!("{url}/cancel")));
    assert!(page.contains(r#"hx-trigger="every 3s""#), "a running run refreshes itself");
    let list = fixture.page("/alice/rocket/actions?event=push", &fixture.reader).await;
    assert!(
        list.contains(r#"id="runs" hx-get="/alice/rocket/actions?event=push" hx-trigger="every 3s""#),
        "the list follows its runs, filtered"
    );
    let seen = fixture.page(&url, &fixture.reader).await;
    assert!(!seen.contains(&format!("{url}/cancel")), "readers get no buttons");

    let (status, ..) = fixture.send(Method::POST, &format!("{url}/cancel"), &fixture.reader, "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, location) = fixture.send(Method::POST, &format!("{url}/cancel"), &fixture.owner, "").await;
    assert!(status.is_redirection());
    assert_eq!(location.as_deref(), Some(url.as_str()));
    let page = fixture.page(&url, &fixture.owner).await;
    assert!(!page.contains(r#"hx-trigger="every 3s""#), "a finished run stays still");
    assert!(page.contains(&format!("{url}/rerun")) && page.contains(&format!("{url}/rerun-failed")));
    let list = fixture.page("/alice/rocket/actions", &fixture.reader).await;
    assert!(list.contains(r#"id="runs" hx-get="/alice/rocket/actions" hx-trigger="every 10s""#), "the newest runs look for new ones");
    let older = fixture.page(&format!("/alice/rocket/actions?before={}", run.id.0 + 1), &fixture.reader).await;
    assert!(older.contains("Change") && !older.contains(r#"id="runs" hx-get"#), "an older page of finished runs stays still");

    let (status, ..) = fixture.send(Method::POST, &format!("{url}/rerun-failed"), &fixture.owner, "").await;
    assert!(status.is_redirection());
    let again = fixture.services.actions.as_ref().unwrap();
    let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
    assert_eq!(again.run_detail(&repo, run.number).await.unwrap().run.attempt, 2);

    let (status, ..) = fixture.send(Method::GET, "/alice/rocket/actions/runs/999", &fixture.reader, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, ..) = fixture.send(Method::GET, "/alice/rocket/actions/runs/nope", &fixture.reader, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
