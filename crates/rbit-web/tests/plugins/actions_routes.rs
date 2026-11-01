//! Real Actions documents retain scoped authority, host proofs and recovery behavior.

use super::*;
use rbit_core::actions::RunnerScope;

const RUNNERS: &str = "/alice/rocket/-/plugins/rbit.actions/settings/runners";
const PERSONAL: &str = "/-/plugins/rbit.actions/settings/runners";
const ORG: &str = "/-/plugins/rbit.actions/org/runners";

async fn active() -> Fixture {
    let fixture = Fixture::new().await;
    fixture.services.set_actions_enabled(&fixture.admin, true).await.unwrap();
    let repo = fixture.services.repositories.open(Some(&fixture.admin), "alice", "rocket").await.unwrap();
    fixture.services.actions.snapshot().unwrap().set_repo_enabled(&fixture.admin, &repo, true).await.unwrap();
    fixture
}

fn csrf(html: &str, action: &str) -> String {
    let target = format!("action=\"{action}\"");
    let form =
        html.split("<form ").map(|form| form.split("</form>").next().unwrap()).find(|form| form.contains(&target)).expect("form action");
    let token = form.split("name=\"_csrf\" value=\"").nth(1).unwrap().split('"').next().unwrap();
    assert_ne!(token, "");
    token.to_owned()
}

async fn form(fixture: &Fixture, page: &str, action: &str, cookie: &str) -> String {
    let (status, html, headers) = fixture.request(Method::GET, page, Some(cookie), "http://localhost:3000", "").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    csrf(&html, action)
}

async fn count(fixture: &Fixture) -> usize {
    let repo = fixture.services.repositories.open(Some(&fixture.admin), "alice", "rocket").await.unwrap();
    fixture.database.actions_runners().registration_tokens(RunnerScope::Repository(repo.repo.repo.id)).await.unwrap().len()
}

async fn post(fixture: &Fixture, path: &str, cookie: &str, body: &str) -> StatusCode {
    fixture.request(Method::POST, path, Some(cookie), "http://localhost:3000", body).await.0
}

#[tokio::test]
async fn native_actions_routes_follow_enable_disable_and_reject_old_generation_proofs() {
    let fixture = Fixture::new().await;
    assert_eq!(fixture.get(RUNNERS).await.0, StatusCode::NOT_FOUND);
    fixture.services.set_actions_enabled(&fixture.admin, true).await.unwrap();
    let token = form(&fixture, RUNNERS, RUNNERS, &fixture.admin_cookie).await;
    fixture.services.set_actions_enabled(&fixture.admin, false).await.unwrap();
    assert_eq!(fixture.get(RUNNERS).await.0, StatusCode::NOT_FOUND);
    assert_eq!(post(&fixture, RUNNERS, &fixture.admin_cookie, &format!("_csrf={token}&action=create")).await, StatusCode::NOT_FOUND);
    assert_eq!(count(&fixture).await, 0);
    fixture.services.set_actions_enabled(&fixture.admin, true).await.unwrap();
    assert_eq!(post(&fixture, RUNNERS, &fixture.admin_cookie, &format!("_csrf={token}&action=create")).await, StatusCode::FORBIDDEN);
    assert_eq!(count(&fixture).await, 0);
    let fresh = form(&fixture, RUNNERS, RUNNERS, &fixture.admin_cookie).await;
    assert_eq!(post(&fixture, RUNNERS, &fixture.admin_cookie, &format!("_csrf={fresh}&action=create")).await, StatusCode::OK);
    assert_eq!(count(&fixture).await, 1);
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn native_actions_routes_refuse_missing_cross_session_cross_endpoint_and_cross_origin_proofs_before_writes() {
    let fixture = active().await;
    let token = form(&fixture, RUNNERS, RUNNERS, &fixture.admin_cookie).await;
    let second = format!("rbit_session={}", fixture.services.accounts.start_session(&fixture.admin).await.unwrap().encode());
    for (path, cookie, body) in [
        (RUNNERS, fixture.admin_cookie.as_str(), "action=create".to_owned()),
        (RUNNERS, second.as_str(), format!("_csrf={token}&action=create")),
        (PERSONAL, fixture.admin_cookie.as_str(), format!("_csrf={token}&action=create")),
    ] {
        assert_eq!(post(&fixture, path, cookie, &body).await, StatusCode::FORBIDDEN);
    }
    assert_eq!(
        fixture
            .request(Method::POST, RUNNERS, Some(&fixture.admin_cookie), "https://evil.example", &format!("_csrf={token}&action=create"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(count(&fixture).await, 0);
    assert_eq!(fixture.database.actions_runners().registration_tokens(RunnerScope::Owner(fixture.admin.id())).await.unwrap().len(), 0);
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn native_actions_routes_legacy_gets_redirect_but_legacy_posts_cannot_mutate() {
    let fixture = active().await;
    let legacy = "/alice/rocket/settings/actions/runners";
    let (status, _, headers) = fixture.get(legacy).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], RUNNERS);
    let logs = "/alice/rocket/actions/runs/3/jobs/7/logs";
    for (source, target) in [
        (logs.to_owned(), "/alice/rocket/-/plugins/rbit.actions/job?job=7&run=3"),
        (format!("{logs}?step=0"), "/alice/rocket/-/plugins/rbit.actions/logs?job=7&run=3&step=0"),
        (format!("{logs}?%73tep=0"), "/alice/rocket/-/plugins/rbit.actions/logs?job=7&run=3&step=0"),
    ] {
        let (status, _, headers) = fixture.get(&source).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(headers[header::LOCATION], target);
    }
    let token = form(&fixture, RUNNERS, RUNNERS, &fixture.admin_cookie).await;
    let status = post(&fixture, legacy, &fixture.admin_cookie, &format!("_csrf={token}&action=create")).await;
    assert!(matches!(status, StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED));
    assert_eq!(count(&fixture).await, 0);
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn native_actions_routes_organization_hidden_owner_is_reauthorized_by_domain_services() {
    let fixture = active().await;
    let bob_account = fixture.database.accounts().find_account_by_name("bob").await.unwrap().unwrap();
    let bob = fixture.database.accounts().find_active_user(bob_account.id).await.unwrap().unwrap();
    let forbidden = fixture
        .services
        .orgs
        .create(
            &fixture.admin,
            rbit_services::CreateOrganization { name: "acme".into(), display_name: String::new(), description: String::new() },
        )
        .await
        .unwrap();
    let own = fixture
        .services
        .orgs
        .create(
            &bob,
            rbit_services::CreateOrganization { name: "bob-team".into(), display_name: String::new(), description: String::new() },
        )
        .await
        .unwrap();
    let token = form(&fixture, &format!("{ORG}?owner=bob-team"), ORG, &fixture.user_cookie).await;
    assert_eq!(post(&fixture, ORG, &fixture.user_cookie, &format!("_csrf={token}&owner=acme&action=create")).await, StatusCode::NOT_FOUND);
    assert_eq!(fixture.database.actions_runners().registration_tokens(RunnerScope::Owner(forbidden.account.id)).await.unwrap().len(), 0);
    assert_eq!(fixture.database.actions_runners().registration_tokens(RunnerScope::Owner(own.account.id)).await.unwrap().len(), 0);
    assert_eq!(post(&fixture, ORG, &fixture.user_cookie, &format!("_csrf={token}&owner=bob-team&action=create")).await, StatusCode::OK);
    assert_eq!(fixture.database.actions_runners().registration_tokens(RunnerScope::Owner(own.account.id)).await.unwrap().len(), 1);
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn native_actions_routes_terminal_close_keeps_host_recovery_and_persisted_intent() {
    let fixture = active().await;
    let token = form(&fixture, RUNNERS, RUNNERS, &fixture.admin_cookie).await;
    fixture.services.prepare_shutdown();
    assert_eq!(fixture.get(RUNNERS).await.0, StatusCode::NOT_FOUND);
    assert_eq!(post(&fixture, RUNNERS, &fixture.admin_cookie, &format!("_csrf={token}&action=create")).await, StatusCode::NOT_FOUND);
    assert_eq!(fixture.get("/admin/plugins").await.0, StatusCode::OK);
    assert!(fixture.services.set_actions_enabled(&fixture.admin, true).await.is_err());
    assert_eq!(fixture.database.plugin_settings().desired_enabled("rbit.actions").await.unwrap(), Some(true));
    assert_eq!(count(&fixture).await, 0);
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn native_actions_routes_designated_read_only_reviewer_can_approve_a_deployment_without_write_access() {
    use crate::actions_runner_tasks::{payload, poll, seed, template};
    use rbit_services::actions::environments::EnvironmentInput;
    let fixture = crate::actions_runner::Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let reviewer = fixture
        .services
        .accounts
        .create_user(
            SignUp { username: "reviewer".into(), email: "reviewer@example.com".into(), password: "correct horse battery".into() },
            Some(false),
        )
        .await
        .unwrap();
    let repo = fixture.repo().await;
    fixture.services.collaborators.add(&fixture.alice, &repo, "reviewer", rbit_core::membership::Permission::Read).await.unwrap();
    fixture
        .actions()
        .put_environment(
            &fixture.alice,
            &repo,
            "production",
            &EnvironmentInput { reviewers: vec!["reviewer".into()], ..EnvironmentInput::default() },
        )
        .await
        .unwrap();
    let credential = fixture.runner(&["linux"]).await;
    let mut deploy = template("deploy", &["linux"], &[]);
    deploy.environment = Some("production".into());
    let run = seed(&fixture, &[payload(&deploy)]).await;
    assert_eq!(poll(&fixture, &credential, Some("waiting-review")).await.0, StatusCode::NO_CONTENT);
    let reviewer_repo = fixture.services.repositories.open(Some(&reviewer), "alice", "rocket").await.unwrap();
    assert!(reviewer_repo.access.can_read());
    assert!(!reviewer_repo.access.can_write());
    let cookie = format!("rbit_session={}", fixture.services.accounts.start_session(&reviewer).await.unwrap().encode());
    let request = |method, path: String, body: String| {
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::COOKIE, &cookie)
            .header(header::HOST, "git.example.com")
            .header(header::ORIGIN, "https://git.example.com")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .unwrap()
    };
    let response = fixture
        .router
        .clone()
        .oneshot(request(Method::GET, format!("/alice/rocket/-/plugins/rbit.actions/run?run={}", run.number), String::new()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = String::from_utf8(to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap().to_vec()).unwrap();
    let action = "/alice/rocket/-/plugins/rbit.actions/review";
    let proof = csrf(&html, action);
    let body = format!("_csrf={proof}&run={}&environment=production&decision=approve&comment=reviewed", run.number);
    let response = fixture.router.clone().oneshot(request(Method::POST, action.to_owned(), body)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let _ = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
    assert_eq!(fixture.actions().pending_deployments(Some(&reviewer), &reviewer_repo, &run).await.unwrap().len(), 0);
    fixture.services.shutdown().await;
}

#[cfg(feature = "i18n")]
#[tokio::test]
async fn native_actions_routes_runner_token_success_uses_captured_rtl_catalog_for_titles_and_metadata() {
    let fixture = active().await;
    let translated = "استكشاف\ntitle-runners = العداؤون\ntitle-runners-administration = إدارة العدائين";
    fixture.services.install_language_pack(&fixture.admin, &language_pack("ar", "rtl", translated)).await.unwrap();
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    fixture.services.set_account_locale(&fixture.admin, Some("ar")).await.unwrap();
    for (path, title) in [(PERSONAL, "العداؤون"), ("/-/plugins/rbit.actions/admin/runners", "إدارة العدائين")] {
        let proof = form(&fixture, path, path, &fixture.admin_cookie).await;
        let (status, html, headers) = fixture
            .request(Method::POST, path, Some(&fixture.admin_cookie), "http://localhost:3000", &format!("_csrf={proof}&action=create"))
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(headers[header::CONTENT_LANGUAGE], "ar");
        assert!(!headers.contains_key(header::LOCATION));
        assert!(html.contains("<html lang=\"ar\" dir=\"rtl\""));
        assert!(html.contains(&format!("<title>{title}")), "owned callback must not fall back to the ambient English title");
        assert!(html.contains("id=\"issued-token\""));
    }
    fixture.services.shutdown().await;
}
