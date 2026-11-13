use super::*;

#[test]
fn git_and_lfs_tokens_are_confined_to_configured_origins() {
    for (kind, host, allowed, refused) in [
        (
            SourceKind::GitHub,
            "",
            vec!["https://github.com/owner/repo.git", "https://API.GITHUB.COM:443/repos/owner/repo"],
            vec!["http://github.com/owner/repo.git", "https://github.com:8443/owner/repo.git", "https://github.com.evil.test/repo.git"],
        ),
        (
            SourceKind::Gitea,
            "https://forge.example:8443/gitea",
            vec!["https://forge.example:8443/gitea/owner/repo.git"],
            vec!["http://forge.example:8443/repo.git", "https://forge.example/repo.git"],
        ),
        (
            SourceKind::Gitea,
            "http://[::1]:3000",
            vec!["http://[::1]:3000/owner/repo.git"],
            vec!["http://[::1]:3001/owner/repo.git", "http://[::2]:3000/repo.git"],
        ),
    ] {
        let source = ImportSource::parse(kind, host, "owner/repo").unwrap();
        let client = SourceClient::new(
            source,
            Some("source-secret".to_owned()),
            true,
            Duration::from_secs(5),
            Duration::ZERO,
            CancellationToken::new(),
            false,
        )
        .unwrap();
        for url in allowed {
            assert_eq!(client.git_token(url), Some("source-secret"), "{url}");
        }
        for url in refused {
            assert_eq!(client.git_token(url), None, "{url}");
        }
    }
}

#[tokio::test]
async fn import_api_and_download_clients_omit_running_version_when_disabled() {
    use axum::{Router, extract::Request, routing::get};
    let app = Router::new().route(
        "/probe",
        get(|request: Request| async move {
            assert_eq!(request.headers()[reqwest::header::USER_AGENT], "rbit-Importer");
            StatusCode::NO_CONTENT
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let source = ImportSource::parse(rbit_core::import::SourceKind::Gitea, &format!("http://{address}"), "owner/repository").unwrap();
    let client = SourceClient::new(source, None, true, Duration::from_secs(5), Duration::ZERO, CancellationToken::new(), false).unwrap();
    for http in [&client.api, &client.downloads] {
        assert_eq!(http.get(format!("http://{address}/probe")).send().await.unwrap().status(), StatusCode::NO_CONTENT);
    }
    server.abort();
}

fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(*name, HeaderValue::from_str(value).unwrap());
    }
    map
}

#[test]
fn next_links_are_found() {
    let link = r#"<https://api.github.com/repositories/1/issues?page=2>; rel="next", <https://api.github.com/repositories/1/issues?page=5>; rel="last""#;
    assert_eq!(next_link(&headers(&[("link", link)])).as_deref(), Some("https://api.github.com/repositories/1/issues?page=2"));
    let last_page = r#"<https://x/issues?page=1>; rel="first", <https://x/issues?page=4>; rel="prev""#;
    assert_eq!(next_link(&headers(&[("link", last_page)])), None);
    assert_eq!(next_link(&HeaderMap::new()), None);
    assert_eq!(next_link(&headers(&[("link", "<https://x/2>;rel=next")])).as_deref(), Some("https://x/2"), "Gitea spacing");
}

#[test]
fn rate_limits_are_waited_out() {
    let now = Timestamp(1_000);
    let used_up = headers(&[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "1030")]);
    assert_eq!(rate_limit_wait(StatusCode::FORBIDDEN, &used_up, now), Some(Duration::from_secs(31)));
    assert_eq!(rate_limit_wait(StatusCode::TOO_MANY_REQUESTS, &headers(&[("retry-after", "7")]), now), Some(Duration::from_secs(7)));
    assert_eq!(rate_limit_wait(StatusCode::TOO_MANY_REQUESTS, &HeaderMap::new(), now), Some(Duration::from_secs(60)));
    assert_eq!(rate_limit_wait(StatusCode::FORBIDDEN, &HeaderMap::new(), now), None, "a plain 403 is a permission problem");
    assert_eq!(rate_limit_wait(StatusCode::OK, &used_up, now), None);
    assert_eq!(exhausted_until_reset(&used_up, now), Some(Duration::from_secs(31)));
    assert_eq!(exhausted_until_reset(&headers(&[("x-ratelimit-remaining", "4")]), now), None);
}

#[test]
fn error_bodies_are_summarised() {
    assert_eq!(error_message(r#"{"message": "Bad credentials", "documentation_url": "x"}"#), "Bad credentials");
    assert_eq!(error_message("plain text"), "plain text");
    assert_eq!(error_message(&"x".repeat(1000)).len(), MAX_ERROR_BODY);
}
