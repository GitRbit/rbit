//! Borrow request headers only long enough to choose the existing Explore response shape.

use crate::localization::single_header;
use axum::{extract::FromRequestParts, http::request::Parts};
use std::{
    convert::Infallible,
    future::{Future, ready},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExploreRequestShape {
    results_only: bool,
}
impl ExploreRequestShape {
    pub fn results_only(self) -> bool {
        self.results_only
    }
}
impl<S: Send + Sync> FromRequestParts<S> for ExploreRequestShape {
    type Rejection = Infallible;
    fn from_request_parts(parts: &mut Parts, _state: &S) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let headers = &parts.headers;
        ready(Ok(Self {
            results_only: single_header(headers, "hx-request") == Some("true")
                && single_header(headers, "hx-target") == Some("explore-results")
                && !headers.contains_key("hx-history-restore-request")
                && !headers.contains_key("hx-boosted"),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{HeaderMap, HeaderValue, Request},
    };

    // Preserve the prior route's header expression as an independent behavior oracle.
    fn previous_shape(headers: &HeaderMap) -> bool {
        let one = |name: &str| {
            let values: Vec<_> = headers.get_all(name).iter().collect();
            if values.len() == 1 { values[0].to_str().ok() } else { None }
        };
        one("hx-request") == Some("true")
            && one("hx-target") == Some("explore-results")
            && !headers.contains_key("hx-history-restore-request")
            && !headers.contains_key("hx-boosted")
    }

    #[tokio::test]
    async fn request_shape_matches_old_expression_without_consuming_headers() {
        for request in [None, Some("true"), Some("false"), Some("TRUE")] {
            for target in [None, Some("explore-results"), Some("main"), Some("#explore-results")] {
                for bypass in [None, Some("hx-boosted"), Some("hx-history-restore-request")] {
                    let (mut parts, _) = Request::new(Body::empty()).into_parts();
                    if let Some(value) = request {
                        parts.headers.insert("hx-request", HeaderValue::from_static(value));
                    }
                    if let Some(value) = target {
                        parts.headers.insert("hx-target", HeaderValue::from_static(value));
                    }
                    // Presence, including a false value, preserves full/history semantics.
                    if let Some(name) = bypass {
                        parts.headers.insert(name, HeaderValue::from_static("false"));
                    }
                    let before = parts.headers.clone();
                    let selected = ExploreRequestShape::from_request_parts(&mut parts, &()).await.unwrap();
                    assert_eq!(selected.results_only(), previous_shape(&before));
                    assert_eq!(parts.headers, before);
                }
            }
        }
    }

    #[tokio::test]
    async fn repeated_and_non_utf8_shape_headers_cannot_enable_a_fragment() {
        for name in ["hx-request", "hx-target"] {
            for value in [HeaderValue::from_static("true"), HeaderValue::from_bytes(&[0xff]).unwrap()] {
                let (mut parts, _) = Request::builder()
                    .header("hx-request", "true")
                    .header("hx-target", "explore-results")
                    .body(Body::empty())
                    .unwrap()
                    .into_parts();
                parts.headers.append(name, value);
                assert!(!ExploreRequestShape::from_request_parts(&mut parts, &()).await.unwrap().results_only());
                assert!(!previous_shape(&parts.headers));
            }
        }
        let (mut parts, _) = Request::builder()
            .header("hx-request", HeaderValue::from_bytes(&[0xff]).unwrap())
            .header("hx-target", "explore-results")
            .body(Body::empty())
            .unwrap()
            .into_parts();
        assert!(!ExploreRequestShape::from_request_parts(&mut parts, &()).await.unwrap().results_only());
    }
}
