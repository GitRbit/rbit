//! Overload remains a retryable generic error, never a credential challenge.

use super::*;
use axum::http::header;

#[tokio::test]
async fn unavailable_preserves_error_info_and_retry_privacy_headers() {
    let response = WebError::from(ServiceError::Unavailable { retry_after_seconds: 7 }).into_response();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers()[header::RETRY_AFTER], "7");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(!response.headers().contains_key(header::WWW_AUTHENTICATE));
    let info = response.extensions().get::<ErrorInfo>().unwrap();
    assert_eq!(info.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(info.title, "Temporarily unavailable");
    assert_eq!(info.message, UNAVAILABLE_MESSAGE);
    let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
    assert_eq!(body.as_ref(), UNAVAILABLE_MESSAGE.as_bytes());
}

#[test]
fn rate_limits_remain_distinct_from_temporary_unavailability() {
    let response = WebError::from(ServiceError::RateLimited { retry_after_seconds: 11 }).into_response();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()[header::RETRY_AFTER], "11");
    assert_eq!(response.extensions().get::<ErrorInfo>().unwrap().title, "Slow down");
}
