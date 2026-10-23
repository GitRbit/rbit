//! Versionless cached update metadata for persistent footers during htmx navigation.

use axum::{
    extract::{Request, State},
    http::{HeaderName, HeaderValue, header},
    middleware::Next,
    response::Response,
};
use rbit_services::updates::UpdateStatus;

use crate::AppState;

const NOTICE_HEADER: HeaderName = HeaderName::from_static("rbit-update-available");

/// Copies an existing service snapshot; page requests never check upstream releases.
pub async fn report(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    add_notice(&mut response, &state.services.updates.status());
    response
}

fn add_notice(response: &mut Response, status: &UpdateStatus) {
    let is_html = response.headers().get(header::CONTENT_TYPE).is_some_and(|value| value.as_bytes().starts_with(b"text/html"));
    if is_html
        && !crate::middleware::is_immutable(response.headers())
        && let Ok(value) = HeaderValue::from_str(&format!("{};{}", status.revision, status.notice()))
    {
        response.headers_mut().insert(NOTICE_HEADER, value);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{Router, body::Body, http::StatusCode, middleware::from_fn, response::Html, routing::get};
    use rbit_services::updates::{AvailableRelease, UpdateState};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn newer_notice_survives_unchanged_partial_body_and_not_modified_response() {
        let status = Arc::new(Mutex::new(UpdateStatus { revision: 1, state: UpdateState::Pending, release: None }));
        let observed = status.clone();
        let page = format!(
            "{}<title>Repository</title>{}{}<div id=\"repo-subnav\" data-repo-scope=\"/a/b\"></div>{}{}<main id=\"main\">body</main>{}",
            crate::partial::TITLE_START,
            crate::partial::TITLE_END,
            crate::partial::SUBNAV_START,
            crate::partial::SUBNAV_END,
            crate::partial::MAIN_START,
            crate::partial::MAIN_END,
        );
        let app = Router::new()
            .route(
                "/a/b",
                get(move || {
                    let page = page.clone();
                    async { Html(page) }
                }),
            )
            .layer(from_fn(crate::partial::partial_responses))
            .layer(from_fn(move |request, next: Next| {
                let observed = observed.clone();
                async move {
                    let mut response = next.run(request).await;
                    add_notice(&mut response, &observed.lock().unwrap());
                    response
                }
            }))
            .layer(from_fn(crate::middleware::html_caching));
        let request = |etag: Option<&str>| {
            let mut request = axum::http::Request::builder().uri("/a/b").header("HX-Request", "true").header("Rbit-Partial", "/a/b");
            if let Some(etag) = etag {
                request = request.header(header::IF_NONE_MATCH, etag);
            }
            request.body(Body::empty()).unwrap()
        };
        let response = app.clone().oneshot(request(None)).await.unwrap();
        assert_eq!(response.headers()[NOTICE_HEADER], "1;pending");
        let etag = response.headers()[header::ETAG].to_str().unwrap().to_owned();
        *status.lock().unwrap() = UpdateStatus {
            revision: 2,
            state: UpdateState::Available,
            release: Some(AvailableRelease {
                version: "99.0.0".into(),
                url: "https://github.com/GitRbit/rbit/releases/tag/v99.0.0".into(),
            }),
        };
        let response = app.oneshot(request(Some(&etag))).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(response.headers()[NOTICE_HEADER], "2;https://github.com/GitRbit/rbit/releases/tag/v99.0.0");
    }
}
