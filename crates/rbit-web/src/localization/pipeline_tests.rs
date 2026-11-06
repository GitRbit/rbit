//! Exercise the real HTML pipeline/cache layers with an intentionally equal body.
use super::page_locale;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    middleware::from_fn,
    response::{Html, IntoResponse},
    routing::get,
};
use tower::ServiceExt;

#[tokio::test]
async fn equal_legacy_fragment_bytes_have_catalog_specific_validators_and_304_metadata() {
    let router = Router::new()
        .route("/fragment", get(|| async { Html("<p lang=\"en\">Legacy body</p>") }))
        .layer(from_fn(page_locale))
        .layer(from_fn(crate::middleware::html_caching));
    let mut tags = Vec::new();
    for (language, key) in [("en", "1.en"), ("es", "1.es")] {
        let request = Request::builder().uri("/fragment").header(header::ACCEPT_LANGUAGE, language).body(Body::empty()).unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.headers()[header::CONTENT_LANGUAGE], language);
        assert_eq!(response.headers()["rbit-catalog-key"], key);
        let tag = response.headers()[header::ETAG].to_str().unwrap().to_owned();
        let bytes = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(&bytes[..], b"<p lang=\"en\">Legacy body</p>");
        let request = Request::builder()
            .uri("/fragment")
            .header(header::ACCEPT_LANGUAGE, language)
            .header(header::IF_NONE_MATCH, &tag)
            .body(Body::empty())
            .unwrap();
        let validated = router.clone().oneshot(request).await.unwrap();
        assert_eq!(validated.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(validated.headers()["rbit-catalog-key"], key);
        assert_eq!(validated.headers()[header::CONTENT_LANGUAGE], language);
        assert!(axum::body::to_bytes(validated.into_body(), 4096).await.unwrap().is_empty());
        tags.push(tag);
    }
    assert_ne!(tags[0], tags[1]);
}

#[tokio::test]
async fn language_neutral_immutable_fragment_preserves_its_original_contract() {
    let router = Router::new()
        .route("/pinned", get(|| async { crate::middleware::cache_pinned(Html("<p>fixed code bytes</p>").into_response(), true) }))
        .layer(from_fn(page_locale))
        .layer(from_fn(crate::middleware::html_caching));
    let request = Request::builder().uri("/pinned").header(header::ACCEPT_LANGUAGE, "es").body(Body::empty()).unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.headers()[header::CACHE_CONTROL], crate::middleware::PINNED_CACHE_CONTROL);
    assert!(!response.headers().contains_key("rbit-catalog-key"));
    assert!(!response.headers().contains_key(header::CONTENT_LANGUAGE));
    assert_eq!(response.headers()[header::VARY], "HX-Request");
}
