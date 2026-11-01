//! Native Actions documents retain bounded values and strict host request parsing.
use crate::actions_runner::Fixture;
use askama::Template;
use axum::{
    body::{Body, to_bytes},
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use rbit_core::{Visibility, actions::SecretScope};
use rbit_services::{
    CreateRepository,
    actions::secrets::{MAX_VALUE_BYTES, MAX_VARIABLE_BYTES},
};
use tower::ServiceExt as _;

const SETTINGS: &str = "/alice/rocket/-/plugins/rbit.actions/settings";
#[derive(Template)]
#[template(source = "{{ value }}", ext = "html")]
struct Escaped<'a> {
    value: &'a str,
}
async fn cookie(fixture: &Fixture) -> String {
    format!("rbit_session={}", fixture.services.accounts.start_session(&fixture.alice).await.unwrap().encode())
}
async fn request(fixture: &Fixture, cookie: &str, method: Method, path: &str, body: String) -> (StatusCode, HeaderMap, String) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "git.example.com")
        .header(header::ORIGIN, "https://git.example.com")
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    let response = fixture.router.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let bytes = to_bytes(body, 3 * 1024 * 1024).await.unwrap();
    (parts.status, parts.headers, String::from_utf8(bytes.to_vec()).unwrap())
}
async fn proof(fixture: &Fixture, cookie: &str, source: &str, target: &str) -> String {
    let (status, _, html) = request(fixture, cookie, Method::GET, source, String::new()).await;
    assert_eq!(status, StatusCode::OK);
    html.split(&format!("action=\"{target}\""))
        .nth(1)
        .unwrap()
        .split("</form>")
        .next()
        .unwrap()
        .split("name=\"_csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .into()
}
fn encoded(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}
#[tokio::test]
async fn native_actions_limits_preserve_full_maximum_secret_and_editable_variable_values() {
    let fixture = Fixture::new().await;
    let cookie = cookie(&fixture).await;
    let secrets = format!("{SETTINGS}/secrets");
    let variables = format!("{SETTINGS}/variables");
    let marker = "PRIVATE_KEY_DO_NOT_RENDER_";
    let secret = format!("{marker}{}", "&".repeat(MAX_VALUE_BYTES - marker.len()));
    let token = proof(&fixture, &cookie, SETTINGS, &secrets).await;
    let (status, headers, html) =
        request(&fixture, &cookie, Method::POST, &secrets, format!("_csrf={token}&name=MAX_SECRET&value={}", encoded(&secret))).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert!(!html.contains(marker));
    let repo = fixture.repo().await;
    let stored = fixture.database.actions_secrets().secrets_for_task(repo.repo.repo.id, fixture.alice.id()).await.unwrap();
    let stored = stored.iter().find(|secret| secret.info.name.as_str() == "MAX_SECRET").unwrap();
    assert_eq!(stored.sealed.ciphertext.len(), MAX_VALUE_BYTES + 16, "the complete value was sealed without truncation");
    let (status, _, html) = request(&fixture, &cookie, Method::GET, SETTINGS, String::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!html.contains(marker));
    let marker = "VISIBLE_FULL_VALUE_";
    let value = format!("{marker}{}", "&".repeat(MAX_VARIABLE_BYTES - marker.len()));
    let token = proof(&fixture, &cookie, SETTINGS, &variables).await;
    let (status, _, _) =
        request(&fixture, &cookie, Method::POST, &variables, format!("_csrf={token}&name=MAX_VARIABLE&value={}", encoded(&value))).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let stored = fixture.database.actions_secrets().variables(SecretScope::Repository(repo.repo.repo.id)).await.unwrap();
    assert_eq!(stored.iter().find(|v| v.name.as_str() == "MAX_VARIABLE").unwrap().value, value);
    let (status, headers, html) = request(&fixture, &cookie, Method::GET, &format!("{SETTINGS}?edit=MAX_VARIABLE"), String::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        html.matches("<!DOCTYPE html>").count() + html.matches("<!doctype html>").count(),
        1,
        "the compiled document is not nested in another host shell"
    );
    let escaped = Escaped { value: &value }.render().unwrap();
    assert!(html.contains(&escaped), "the selected editor retains the complete escaped48KiB value");
    assert!(!html.contains(&value));
    fixture.services.shutdown().await;
}
#[tokio::test]
async fn native_actions_limits_reject_query_aliases_and_scalar_duplicates_without_writes() {
    let fixture = Fixture::new().await;
    let cookie = cookie(&fixture).await;
    for query in ["done=saved&%64one=other", "unexpected=value", "done=%", "done=%GG", "done=%0a"] {
        let (status, headers, _) = request(&fixture, &cookie, Method::GET, &format!("{SETTINGS}?{query}"), String::new()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    }
    for path in [
        "/alice/rocket/-/plugins/rbit.actions/run?run=0",
        "/alice/rocket/-/plugins/rbit.actions/run?run=wrong",
        "/alice/rocket/-/plugins/rbit.actions/job?run=1&job=0",
    ] {
        let (status, _, _) = request(&fixture, &cookie, Method::GET, path, String::new()).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "resource identity {path}");
    }
    let variables = format!("{SETTINGS}/variables");
    let token = proof(&fixture, &cookie, SETTINGS, &variables).await;
    for fields in ["name=ONE&%6Eame=TWO&value=x", "name=ONE&value=left&%76alue=right"] {
        let (status, _, _) = request(&fixture, &cookie, Method::POST, &variables, format!("_csrf={token}&{fields}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, _, _) =
        request(&fixture, &cookie, Method::POST, &variables, format!("_csrf={token}&%5Fcsrf={token}&name=ONE&value=x")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let repo = fixture.repo().await;
    assert_eq!(fixture.database.actions_secrets().variables(SecretScope::Repository(repo.repo.repo.id)).await.unwrap().len(), 0);
    fixture.services.shutdown().await;
}
#[tokio::test]
async fn native_actions_limits_scope_form_proofs_to_the_exact_repository_without_writes() {
    let fixture = Fixture::new().await;
    let cookie = cookie(&fixture).await;
    fixture
        .services
        .repositories
        .create(&fixture.alice, CreateRepository { name: "other".into(), description: String::new(), visibility: Visibility::Private })
        .await
        .unwrap();
    let target = "/alice/other/-/plugins/rbit.actions/settings/variables";
    let token = proof(&fixture, &cookie, SETTINGS, &format!("{SETTINGS}/variables")).await;
    let (status, headers, _) =
        request(&fixture, &cookie, Method::POST, target, format!("_csrf={token}&name=WRONG_REPOSITORY&value=blocked")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    for name in ["rocket", "other"] {
        let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", name).await.unwrap();
        assert_eq!(fixture.database.actions_secrets().variables(SecretScope::Repository(repo.repo.repo.id)).await.unwrap().len(), 0);
    }
    fixture.services.shutdown().await;
}

async fn unread_notification(fixture: &Fixture) {
    let repo = fixture.repo().await.repo.repo.id;
    let now = rbit_core::Timestamp::now();
    let issue = fixture
        .database
        .issues()
        .create(&rbit_storage::sqlite::issues::NewIssue {
            repo,
            kind: rbit_core::IssueKind::Issue,
            title: rbit_core::IssueTitle::parse("Unread activity").unwrap(),
            body: rbit_core::IssueText::default(),
            author: fixture.alice.id(),
            at: now,
        })
        .await
        .unwrap();
    fixture
        .database
        .notifications()
        .notify(fixture.alice.id(), issue.id, repo, rbit_storage::sqlite::notifications::NotificationReason::Mention, None, now)
        .await
        .unwrap();
}

fn assert_document_appearance(html: &str, theme: &str, scheme: &str) {
    let document = html.split("<html").nth(1).expect("full native document").split('>').next().unwrap();
    assert!(document.contains(&format!("data-theme=\"{theme}\"")), "the document retains the host-selected theme");
    assert!(document.contains(&format!("data-scheme=\"{scheme}\"")), "the document retains the host-selected scheme");
    assert!(
        html.contains("<span class=\"notification-dot\" aria-hidden=\"true\"></span>"),
        "the resolved unread indicator survives native rendering"
    );
}

#[tokio::test]
async fn native_actions_documents_preserve_host_theme_and_unread_state_including_no_form_summaries() {
    use crate::actions_runner_tasks::{payload, poll, seed, spec, state, template};
    let fixture = Fixture::new().await;
    unread_notification(&fixture).await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let task = spec(&poll(&fixture, &credential, Some("appearance")).await.1).task_id.0;
    let body = serde_json::json!({"steps": [], "outputs": {}, "result": "success", "summaries": {"0": "## Theme-aware summary"}});
    assert_eq!(state(&fixture, &credential, task, &body).await.0, StatusCode::OK);
    let session = cookie(&fixture).await;
    for (theme, scheme) in [("dna-day", "light"), ("base24-berlin", "dark")] {
        let cookie = format!("{session}; rbit-theme={theme}");
        for path in [
            "/settings/account".to_owned(),
            SETTINGS.to_owned(),
            format!("/alice/rocket/-/plugins/rbit.actions/run?run={}", run.number),
            format!("/alice/rocket/-/plugins/rbit.actions/summary?run={}&job={}&step=0", run.number, job.id.0),
        ] {
            let (status, _, html) = request(&fixture, &cookie, Method::GET, &path, String::new()).await;
            assert_eq!(status, StatusCode::OK, "{path}");
            assert_document_appearance(&html, theme, scheme);
            if path.starts_with("/alice/rocket/") {
                let active: Vec<_> = html
                    .split("<a ")
                    .filter_map(|element| {
                        let tag = element.split('>').next()?;
                        if !tag.contains("aria-current=\"page\"") {
                            return None;
                        }
                        tag.split("data-repo-tab=\"").nth(1)?.split('"').next()
                    })
                    .collect();
                let expected = if path == SETTINGS { "settings" } else { "rbit.actions:actions-repository-tab" };
                assert_eq!(active, [expected], "the native document has exactly the correct active repository tab: {path}");
            }
        }
    }
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn native_actions_owned_success_and_validation_documents_preserve_host_theme_and_unread_state() {
    let fixture = Fixture::new().await;
    unread_notification(&fixture).await;
    let session = cookie(&fixture).await;
    let runners = "/-/plugins/rbit.actions/settings/runners";
    for (theme, scheme) in [("dna-day", "light"), ("base24-berlin", "dark")] {
        let cookie = format!("{session}; rbit-theme={theme}");
        for (fields, expected) in [
            ("action=create&lifetime=3600&labels=linux", StatusCode::OK),
            ("action=create&labels=not%20a%20label", StatusCode::UNPROCESSABLE_ENTITY),
        ] {
            let token = proof(&fixture, &cookie, runners, runners).await;
            let (status, _, html) = request(&fixture, &cookie, Method::POST, runners, format!("_csrf={token}&{fields}")).await;
            assert_eq!(status, expected);
            assert_document_appearance(&html, theme, scheme);
            if expected == StatusCode::OK {
                assert!(html.contains("id=\"issued-token\""));
            }
        }
    }
    fixture.services.shutdown().await;
}

#[path = "actions_summary_host_contract.rs"]
mod native_actions_summary_host_contract;
