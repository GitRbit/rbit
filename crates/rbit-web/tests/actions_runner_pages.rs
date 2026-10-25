//! Actions workstream U: the runners pages of a repository, an organisation, a user and the
//! instance — a token created on the page registers a real runner over the protocol, which
//! the page then lists, disables and removes.

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_services::SignUp;
use tower::ServiceExt;

use crate::actions_runner::{Fixture, register_body};

async fn send(fixture: &Fixture, user: &rbit_core::User, method: Method, path: &str, form: &str) -> (StatusCode, String, Option<String>) {
    let token = fixture.services.accounts.start_session(user).await.unwrap();
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::COOKIE, format!("rbit_session={}", token.encode()))
        .header(header::HOST, "localhost:3000")
        .header(header::ORIGIN, "http://localhost:3000")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form.to_owned()))
        .unwrap();
    let response = fixture.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let location = response.headers().get(header::LOCATION).map(|value| value.to_str().unwrap().to_owned());
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).into_owned(), location)
}

/// The token value shown on a page after creating one.
fn issued(page: &str) -> String {
    let start = page.find(r#"id="issued-token" class="input rounded-r-none font-mono text-xs" value=""#).expect("a token is shown");
    let rest = &page[start..];
    let value = &rest[rest.find("value=\"").unwrap() + 7..];
    value[..value.find('"').unwrap()].to_owned()
}

#[tokio::test]
async fn a_token_from_the_page_registers_a_runner_the_page_then_manages() {
    let fixture = Fixture::new().await;
    let alice = fixture.alice.clone();
    let page_url = "/alice/rocket/settings/actions/runners";

    let (status, page, _) = send(&fixture, &alice, Method::GET, page_url, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("No runners yet"));
    let (status, settings, _) = send(&fixture, &alice, Method::GET, "/alice/rocket/settings/actions", "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(settings.contains(page_url), "the Actions settings link to the runners");

    let (status, page, _) = send(&fixture, &alice, Method::POST, page_url, "action=create&lifetime=86400&labels=linux").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Registration token created") && page.contains("rbit-runner register --url"));
    let token = issued(&page);
    assert!(token.starts_with("rbrg_"), "{token}");
    let (_, again, _) = send(&fixture, &alice, Method::GET, page_url, "").await;
    assert!(!again.contains(&token), "the token is shown once");
    assert!(again.contains("1 use left") && again.contains("linux"));

    let answer = fixture.call("register", None, register_body(&token, "build-1", &["linux"]).to_string()).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.json);
    let runner = answer.json["runner_id"].as_i64().unwrap();
    let (_, page, _) = send(&fixture, &alice, Method::GET, page_url, "").await;
    assert!(page.contains("build-1") && page.contains("container, host") && page.contains("linux/x86_64"));
    assert!(page.contains("used up"), "a single-use token is spent");

    let runner_url = format!("{page_url}/{runner}");
    let (status, _, location) = send(&fixture, &alice, Method::POST, &runner_url, "action=disable").await;
    assert!(status.is_redirection());
    assert_eq!(location.as_deref(), Some(page_url));
    let (_, page, _) = send(&fixture, &alice, Method::GET, page_url, "").await;
    assert!(page.contains("disabled") && page.contains(r#"value="enable""#));
    send(&fixture, &alice, Method::POST, &runner_url, "action=remove").await;
    let (_, page, _) = send(&fixture, &alice, Method::GET, page_url, "").await;
    assert!(page.contains("No runners yet"));

    let (_, page, _) = send(&fixture, &alice, Method::POST, page_url, "action=create&labels=not%20a%20label").await;
    assert!(page.contains("role=\"alert\""), "a malformed label is refused on the form");
}

#[tokio::test]
async fn tokens_can_be_revoked_and_strangers_see_nothing() {
    let fixture = Fixture::new().await;
    let alice = fixture.alice.clone();
    let page_url = "/alice/rocket/settings/actions/runners";
    send(&fixture, &alice, Method::POST, page_url, "action=create&reusable=on").await;
    let token = fixture
        .database
        .actions_runners()
        .registration_tokens(rbit_core::actions::RunnerScope::Repository(fixture.repo().await.repo.repo.id))
        .await
        .unwrap()
        .remove(0);
    let (_, page, _) = send(&fixture, &alice, Method::POST, page_url, &format!("action=revoke&token={}", token.id.0)).await;
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
    for path in [page_url, "/admin/runners"] {
        let (status, ..) = send(&fixture, &bob, Method::GET, path, "").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    let (status, ..) = send(&fixture, &bob, Method::POST, page_url, "action=create").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn owners_and_administrators_have_their_own_runners_pages() {
    let fixture = Fixture::new().await;
    let alice = fixture.alice.clone();
    let (status, page, _) = send(&fixture, &alice, Method::GET, "/settings/actions/runners", "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("your repositories"));
    let (_, page, _) = send(&fixture, &alice, Method::POST, "/settings/actions/runners", "action=create").await;
    assert!(issued(&page).starts_with("rbrg_"));

    let (status, page, _) = send(&fixture, &alice, Method::GET, "/admin/runners", "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains(r#"href="/admin/runners""#), "the admin navigation lists runners with Actions on");
    let (_, page, _) = send(&fixture, &alice, Method::POST, "/admin/runners", "action=create&lifetime=3600").await;
    assert!(issued(&page).starts_with("rbrg_"));

    fixture
        .services
        .orgs
        .create(&alice, rbit_services::CreateOrganization { name: "acme".into(), display_name: String::new(), description: String::new() })
        .await
        .unwrap();
    let (status, page, _) = send(&fixture, &alice, Method::GET, "/acme/settings/actions/runners", "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("acme"));
    let (_, settings, _) = send(&fixture, &alice, Method::GET, "/acme/settings/actions", "").await;
    assert!(settings.contains("/acme/settings/actions/runners"));
}
