//! Native runner forms retain the real runner protocol and scope authorization.

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_services::SignUp;
use tower::ServiceExt;

use crate::actions_runner::{Fixture, register_body};

const REPO_RUNNERS: &str = "/alice/rocket/-/plugins/rbit.actions/settings/runners";
const REPO_RUNNER: &str = "/alice/rocket/-/plugins/rbit.actions/settings/runner";

struct Browser<'a> {
    fixture: &'a Fixture,
    cookie: String,
}
impl<'a> Browser<'a> {
    async fn new(fixture: &'a Fixture, user: &rbit_core::User) -> Self {
        let token = fixture.services.accounts.start_session(user).await.unwrap();
        Self { fixture, cookie: format!("rbit_session={}", token.encode()) }
    }
    async fn send(&self, method: Method, path: &str, form: &str) -> (StatusCode, String, Option<String>) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::COOKIE, &self.cookie)
            .header(header::HOST, "git.example.com")
            .header(header::ORIGIN, "https://git.example.com")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(form.to_owned()))
            .unwrap();
        let response = self.fixture.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let location = response.headers().get(header::LOCATION).map(|value| value.to_str().unwrap().to_owned());
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
        (status, String::from_utf8_lossy(&body).into_owned(), location)
    }
    async fn post(&self, page: &str, action: &str, form: &str) -> (StatusCode, String, Option<String>) {
        let (status, html, _) = self.send(Method::GET, page, "").await;
        assert_eq!(status, StatusCode::OK);
        let proof = proof(&html, action);
        self.send(Method::POST, action, &format!("_csrf={proof}&{form}")).await
    }
}

fn proof(html: &str, action: &str) -> String {
    let target = format!("action=\"{action}\"");
    let form = html.split("<form ").find(|form| form.split("</form>").next().unwrap().contains(&target)).expect("authorized action");
    let proof = form.split("name=\"_csrf\" value=\"").nth(1).unwrap().split('"').next().unwrap();
    assert_ne!(proof, "");
    proof.to_owned()
}

/// The token value is only present in the successful POST response.
fn issued(page: &str) -> String {
    let start = page.find(r#"id="issued-token" class="input rounded-r-none font-mono text-xs" value=""#).expect("a token is shown");
    let rest = &page[start..];
    let value = &rest[rest.find("value=\"").unwrap() + 7..];
    value[..value.find('"').unwrap()].to_owned()
}

#[tokio::test]
async fn a_token_from_the_page_registers_a_runner_the_page_then_manages() {
    let fixture = Fixture::new().await;
    let browser = Browser::new(&fixture, &fixture.alice).await;
    let (status, page, _) = browser.send(Method::GET, REPO_RUNNERS, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("No runners yet"));
    let (status, settings, _) = browser.send(Method::GET, "/alice/rocket/-/plugins/rbit.actions/settings", "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(settings.contains(REPO_RUNNERS));
    let (status, page, _) = browser.post(REPO_RUNNERS, REPO_RUNNERS, "action=create&lifetime=86400&labels=linux").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Registration token created") && page.contains("rbit-runner register --url"));
    let token = issued(&page);
    assert!(token.starts_with("rbrg_"));
    let (_, again, _) = browser.send(Method::GET, REPO_RUNNERS, "").await;
    assert!(!again.contains(&token));
    assert!(again.contains("1 use left") && again.contains("linux"));
    let answer = fixture.call("register", None, register_body(&token, "build-1", &["linux"]).to_string()).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.json);
    let runner = answer.json["runner_id"].as_i64().unwrap();
    let (_, page, _) = browser.send(Method::GET, REPO_RUNNERS, "").await;
    assert!(page.contains("build-1") && page.contains("container, host") && page.contains("linux/x86_64"));
    assert!(page.contains("used up"));
    let (status, _, location) = browser.post(REPO_RUNNERS, REPO_RUNNER, &format!("action=disable&runner={runner}")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location.as_deref(), Some(REPO_RUNNERS));
    let (_, page, _) = browser.send(Method::GET, REPO_RUNNERS, "").await;
    assert!(page.contains("disabled") && page.contains(r#"value="enable""#));
    let (status, ..) = browser.post(REPO_RUNNERS, REPO_RUNNER, &format!("action=remove&runner={runner}")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (_, page, _) = browser.send(Method::GET, REPO_RUNNERS, "").await;
    assert!(page.contains("No runners yet"));
    let (status, page, _) = browser.post(REPO_RUNNERS, REPO_RUNNERS, "action=create&labels=not%20a%20label").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(page.contains("role=\"alert\""));
}

#[tokio::test]
async fn tokens_can_be_revoked_and_strangers_see_nothing() {
    let fixture = Fixture::new().await;
    let browser = Browser::new(&fixture, &fixture.alice).await;
    let (status, ..) = browser.post(REPO_RUNNERS, REPO_RUNNERS, "action=create&reusable=true").await;
    assert_eq!(status, StatusCode::OK);
    let token = fixture
        .database
        .actions_runners()
        .registration_tokens(rbit_core::actions::RunnerScope::Repository(fixture.repo().await.repo.repo.id))
        .await
        .unwrap()
        .remove(0);
    let (status, page, _) = browser.post(REPO_RUNNERS, REPO_RUNNERS, &format!("action=revoke&token={}", token.id.0)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("revoked") || page.contains("no longer usable"));
    let bob = fixture
        .services
        .accounts
        .create_user(
            SignUp { username: "bob".into(), email: "bob@example.com".into(), password: "correct horse battery".into() },
            Some(false),
        )
        .await
        .unwrap();
    let stranger = Browser::new(&fixture, &bob).await;
    for path in [REPO_RUNNERS, "/-/plugins/rbit.actions/admin/runners"] {
        assert_eq!(stranger.send(Method::GET, path, "").await.0, StatusCode::NOT_FOUND);
    }
    assert_eq!(stranger.send(Method::POST, REPO_RUNNERS, "action=create").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn owners_and_administrators_have_their_own_runners_pages() {
    let fixture = Fixture::new().await;
    let browser = Browser::new(&fixture, &fixture.alice).await;
    let personal = "/-/plugins/rbit.actions/settings/runners";
    let (status, page, _) = browser.send(Method::GET, personal, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("your repositories"));
    let (status, page, _) = browser.post(personal, personal, "action=create").await;
    assert_eq!(status, StatusCode::OK);
    assert!(issued(&page).starts_with("rbrg_"));
    let admin = "/-/plugins/rbit.actions/admin/runners";
    let (status, page, _) = browser.send(Method::GET, admin, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains(&format!("href=\"{admin}\"")));
    let (status, page, _) = browser.post(admin, admin, "action=create&lifetime=3600").await;
    assert_eq!(status, StatusCode::OK);
    assert!(issued(&page).starts_with("rbrg_"));
    fixture
        .services
        .orgs
        .create(
            &fixture.alice,
            rbit_services::CreateOrganization { name: "acme".into(), display_name: String::new(), description: String::new() },
        )
        .await
        .unwrap();
    let organization = "/-/plugins/rbit.actions/org/runners?owner=acme";
    let (status, page, _) = browser.send(Method::GET, organization, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("acme") && page.contains("name=\"owner\" value=\"acme\""));
    let (status, page, _) = browser.post(organization, "/-/plugins/rbit.actions/org/runners", "action=create&owner=acme").await;
    assert_eq!(status, StatusCode::OK);
    assert!(issued(&page).starts_with("rbrg_"));
    let (_, settings, _) = browser.send(Method::GET, "/-/plugins/rbit.actions/org/settings?owner=acme", "").await;
    assert!(settings.contains(organization));
}
