//! Security headers on HTML responses and the CSRF origin check.

use rustit_e2e::{Response, StatusCode, TestServer};

#[track_caller]
fn assert_security_headers(response: &Response) {
    let csp = response.header("content-security-policy").expect("a Content-Security-Policy header");
    for directive in ["default-src 'self'", "script-src 'self'", "object-src 'none'", "frame-ancestors 'none'"] {
        assert!(csp.contains(directive), "CSP {csp:?} lacks {directive:?}");
    }
    assert!(!csp.contains("unsafe-inline") && !csp.contains("unsafe-eval"), "CSP {csp:?} allows inline code");
    assert_eq!(response.header("x-frame-options"), Some("DENY"));
    assert_eq!(response.header("x-content-type-options"), Some("nosniff"));
    assert!(response.header("referrer-policy").is_some(), "a Referrer-Policy header");
}

#[tokio::test]
async fn html_responses_carry_security_headers() {
    let server = TestServer::start();
    let anonymous = server.client();
    for path in ["/", "/login", "/signup", "/explore", "/no-such-user", "/no/such/page/at/all"] {
        let page = anonymous.get(path).await;
        assert!(page.header("content-type").is_some_and(|t| t.starts_with("text/html")), "{path} is HTML");
        assert_security_headers(&page);
    }

    let alice = server.sign_up("alice").await;
    alice.create_repo("alice", "demo", "public").await;
    for path in ["/alice", "/alice/demo", "/settings/profile", "/admin"] {
        let page = alice.get(path).await;
        page.expect_status(StatusCode::OK);
        assert_security_headers(&page);
    }
    // Error pages and form re-renders too.
    let rejected = alice.post("/new", &[("owner", "alice"), ("name", "../x"), ("visibility", "public")]).await;
    rejected.expect_status(StatusCode::UNPROCESSABLE_ENTITY);
    assert_security_headers(&rejected);
}

#[tokio::test]
async fn session_cookie_is_http_only_and_same_site() {
    let server = TestServer::start();
    let signed_up = server.client().sign_up("alice").await;
    let cookie = signed_up.header("set-cookie").expect("a session cookie");
    assert!(cookie.starts_with("rustit_session="), "{cookie}");
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax") && cookie.contains("Path=/"), "{cookie}");
}

#[tokio::test]
async fn cross_site_posts_are_refused() {
    let server = TestServer::start();
    let alice = server.sign_up("alice").await;
    let form = [("owner", "alice"), ("name", "forged"), ("visibility", "public")];

    let foreign_origin = alice.post_with("/new", &form, &[("Origin", "https://evil.example")]).await;
    foreign_origin.expect_status(StatusCode::FORBIDDEN);
    let other_port = alice.post_with("/new", &form, &[("Origin", "http://127.0.0.1:1")]).await;
    other_port.expect_status(StatusCode::FORBIDDEN);
    let fetch_metadata = alice.post_with("/new", &form, &[("Sec-Fetch-Site", "cross-site")]).await;
    fetch_metadata.expect_status(StatusCode::FORBIDDEN);
    let same_site = alice.post_with("/new", &form, &[("Sec-Fetch-Site", "same-site")]).await;
    same_site.expect_status(StatusCode::FORBIDDEN);
    alice.get("/alice/forged").await.expect_status(StatusCode::NOT_FOUND);

    // A cross-site sign-out does not end the session either.
    alice.post_with("/logout", &[], &[("Origin", "https://evil.example")]).await.expect_status(StatusCode::FORBIDDEN);
    alice.get("/settings/profile").await.expect_status(StatusCode::OK);

    // The same request from the site itself goes through.
    alice.post_with("/new", &form, &[("Sec-Fetch-Site", "same-origin")]).await.expect_redirect("/alice/forged");
}
