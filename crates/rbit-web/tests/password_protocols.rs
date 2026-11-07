//! Password capacity is retryable across HTML, Git and LFS, without credential leaks.

mod authenticated;
mod fixture;
mod tokens;

use axum::http::{HeaderMap, Method, StatusCode, header};

use fixture::{Fixture, LOGIN_FORM, PASSWORD};

fn unavailable(status: StatusCode, headers: &HeaderMap, body: &str) {
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(headers[header::RETRY_AFTER], "1");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert!(!headers.contains_key(header::WWW_AUTHENTICATE));
    assert!(!headers.contains_key("lfs-authenticate"));
    assert!(!body.contains(PASSWORD));
    assert!(!body.contains("blocking work") && !body.contains("capacity is exhausted"));
    assert!(body.contains("The service is temporarily unavailable. Try again shortly."));
}

async fn password_protocols(fixture: &Fixture) {
    for form in [LOGIN_FORM, "login=unknown&password=protocol-password-123"] {
        let (status, headers, body) = fixture.form("/login", form).await;
        unavailable(status, &headers, &body);
        assert!(headers[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/html"));
        assert!(body.contains("hx-history=\"false\""), "generic error DOMs must not enter HTMX history");
        assert!(!headers.contains_key(header::SET_COOKIE));
    }
    let (status, headers, body) =
        fixture.form("/signup", "username=newuser&email=newuser%40example.com&password=protocol-password-123").await;
    unavailable(status, &headers, &body);
    let authorization = Fixture::basic_header();
    let (status, headers, body) = fixture
        .request(Method::GET, "/keeper/project.git/info/refs?service=git-upload-pack", &[("authorization", &authorization)], "")
        .await;
    unavailable(status, &headers, &body);
    let (status, headers, body) = fixture
        .request(
            Method::POST,
            "/keeper/project.git/info/lfs/objects/batch",
            &[("authorization", &authorization), ("content-type", "application/vnd.git-lfs+json")],
            r#"{"operation":"download","objects":[]}"#,
        )
        .await;
    unavailable(status, &headers, &body);
    assert_eq!(headers[header::CONTENT_TYPE], "application/vnd.git-lfs+json");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({
            "message": "The service is temporarily unavailable. Try again shortly."
        })
    );
}

#[tokio::test]
async fn busy_password_protocols_retry_without_becoming_bad_credentials_and_other_auth_still_works() {
    let fixture = Fixture::new().await;
    let (status, _, body) = fixture.form("/login", "login=keeper&password=wrong").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("Incorrect username or password."));
    let held = fixture.passwords.begin().unwrap();
    password_protocols(&fixture).await;
    let token = format!("Bearer {}", fixture.token);
    let (status, _, _) = fixture.request(Method::GET, "/api/v1/user", &[("authorization", &token)], "").await;
    assert_eq!(status, StatusCode::OK, "personal access tokens do not use password capacity");
    let (status, _, _) = fixture.request(Method::GET, "/healthz", &[], "").await;
    assert_eq!(status, StatusCode::OK);
    drop(held);
    let (status, _, _) = fixture.form("/login", LOGIN_FORM).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "capacity is reusable when the owner releases it");
}

#[tokio::test]
async fn closed_password_protocols_use_the_same_public_retry_contract() {
    let fixture = Fixture::new().await;
    fixture.passwords.close();
    password_protocols(&fixture).await;
}

#[tokio::test]
async fn reset_overload_preserves_secret_url_headers_and_leaves_the_link_usable() {
    let fixture = Fixture::new().await;
    let secret = fixture.reset_token().await;
    let path = format!("/password/reset/{secret}");
    let held = fixture.passwords.begin().unwrap();
    let form = "new_password=replacement-password-456&confirm_password=replacement-password-456";
    let (status, headers, body) = fixture.htmx_form(&path, form).await;
    unavailable(status, &headers, &body);
    assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
    assert!(body.contains("hx-history=\"false\""), "a still-usable reset token must stay out of HTMX history");
    assert_eq!(fixture.services.recovery.reset_target(&secret).await.unwrap(), "keeper");
    drop(held);
    fixture.services.accounts.authenticate("keeper", PASSWORD.into()).await.unwrap();
    let (status, headers, _) = fixture.form(&path, form).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
    assert!(matches!(fixture.services.recovery.reset_target(&secret).await, Err(rbit_services::ServiceError::NotFound)));
}
