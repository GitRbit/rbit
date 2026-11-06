//! Exercise the real head-style, CSP, partial and cache middleware around probe views.

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    middleware::{from_fn, from_fn_with_state},
    response::{Html, IntoResponse},
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rbit_localization::Locale;
use sha2::{Digest as _, Sha256};
use tower::ServiceExt as _;

use super::{MODES, Mode, explore_into, fixtures, login_into};
use crate::{
    middleware, partial,
    themes::{self, Theme},
    timing,
};
use fixtures::Actor;

#[derive(Debug, Clone, Copy)]
enum Feature {
    Explore,
    Login,
}

fn router(mode: Mode, feature: Feature, theme: Theme) -> Router {
    response_router(mode, feature, theme, ResponseKind::Cacheable)
}

#[derive(Debug, Clone, Copy)]
enum ResponseKind {
    Cacheable,
    NoStore,
    SetsCookie,
    InvalidForm,
}

fn response_router(mode: Mode, feature: Feature, theme: Theme, kind: ResponseKind) -> Router {
    let render = move || async move {
        let mut output = String::new();
        // Render completely before constructing a successful response.
        match feature {
            Feature::Explore => explore_into(mode, &fixtures::explore(Actor::Admin, theme, Locale::PseudoRtl, false), &mut output),
            Feature::Login => login_into(mode, &fixtures::login(Actor::Anonymous, theme, true), &mut output),
        }
        .unwrap();
        let mut response = Html(output).into_response();
        match kind {
            ResponseKind::Cacheable => {}
            ResponseKind::NoStore => {
                response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            }
            ResponseKind::SetsCookie => {
                response.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_static("probe=fixed; Path=/; HttpOnly; SameSite=Lax"));
            }
            ResponseKind::InvalidForm => *response.status_mut() = StatusCode::UNPROCESSABLE_ENTITY,
        }
        response
    };
    Router::new()
        .route("/probe", get(render).post(render))
        .layer(from_fn(partial::partial_responses))
        .layer(from_fn(middleware::html_caching))
        .layer(from_fn_with_state(middleware::SecurityHeaders::default(), middleware::security_headers))
        .layer(from_fn(timing::track))
}

async fn request(mode: Mode, feature: Feature, theme: Theme, htmx: bool) -> (HeaderMap, String) {
    let (status, headers, body) = send(router(mode, feature, theme), theme, htmx, Method::GET, None).await;
    assert_eq!(status, StatusCode::OK);
    (headers, body)
}

async fn send(app: Router, theme: Theme, htmx: bool, method: Method, validator: Option<&HeaderValue>) -> (StatusCode, HeaderMap, String) {
    let mut request =
        Request::builder().method(method).uri("/probe").header(header::COOKIE, format!("{}={}", themes::COOKIE, theme.slug()));
    if htmx {
        request = request.header("HX-Request", "true").header("Rbit-Partial", "/keeper/project");
    }
    if let Some(validator) = validator {
        request = request.header(header::IF_NONE_MATCH, validator.clone());
    }
    let response = app.oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    (status, headers, body)
}

fn assert_style_hashes(headers: &HeaderMap, body: &str, include: bool, theme: Theme) {
    let policy = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(!policy.contains("unsafe-inline") && !policy.contains("unsafe-eval"));
    let styles: Vec<_> =
        body.split("<style ").skip(1).map(|tail| tail.split_once('>').unwrap().1.split_once("</style>").unwrap().0).collect();
    if include {
        assert_eq!(styles.len(), if theme.stylesheet_path().is_some() { 2 } else { 1 });
        for stylesheet in styles {
            let hash = STANDARD.encode(Sha256::digest(stylesheet.as_bytes()));
            assert!(policy.contains(&format!("'sha256-{hash}'")), "actual rendered stylesheet must be authorized");
        }
    } else {
        assert_eq!(styles, [] as [&str; 0]);
        assert!(!policy.contains("sha256-"));
    }
}

#[tokio::test]
async fn real_middleware_keeps_csp_cache_partial_and_timing_contracts_for_every_candidate() {
    for feature in [Feature::Explore, Feature::Login] {
        for theme in [Theme::System, Theme::Tinted(&themes::tinted()[0])] {
            for htmx in [false, true] {
                let (expected_headers, expected_body) = request(Mode::Inherited, feature, theme, htmx).await;
                assert_style_hashes(&expected_headers, &expected_body, !htmx, theme);
                assert!(expected_body.contains(timing::MARK_START), "real request timing must remain in the footer");
                assert!(expected_body.contains("<html"), "non-repository views must not become repository partials");
                assert!(expected_headers.contains_key("server-timing"));
                assert_eq!(expected_headers[header::CACHE_CONTROL], "private, no-cache");
                assert!(expected_headers[header::ETAG].to_str().unwrap().starts_with("W/\""), "cacheable HTML must supply a validator");
                let vary = expected_headers.get_all(header::VARY).iter().map(|value| value.to_str().unwrap()).collect::<Vec<_>>().join(",");
                assert!(vary.to_ascii_lowercase().contains("hx-request"));
                assert!(vary.to_ascii_lowercase().contains("rbit-partial"));
                for mode in MODES {
                    let (headers, body) = request(mode, feature, theme, htmx).await;
                    // Only the existing dynamic timing span is excluded, using the same
                    // contract as production validators. Other bytes remain exact.
                    assert_eq!(
                        timing::without_render_time(body.as_bytes()),
                        timing::without_render_time(expected_body.as_bytes()),
                        "{feature:?} {mode:?} htmx={htmx}"
                    );
                    for name in
                        [header::CONTENT_SECURITY_POLICY, header::CACHE_CONTROL, header::ETAG, header::VARY, header::REFERRER_POLICY]
                    {
                        assert_eq!(
                            headers.get_all(&name).iter().collect::<Vec<_>>(),
                            expected_headers.get_all(&name).iter().collect::<Vec<_>>(),
                            "{name}"
                        );
                    }
                    assert_style_hashes(&headers, &body, !htmx, theme);
                }
            }
        }
    }
}

#[tokio::test]
async fn every_candidate_revalidates_real_etags_with_empty_304_and_retained_csp() {
    for feature in [Feature::Explore, Feature::Login] {
        for theme in [Theme::System, Theme::Tinted(&themes::tinted()[0])] {
            for htmx in [false, true] {
                for mode in MODES {
                    let (original, body) = request(mode, feature, theme, htmx).await;
                    let validator = original.get(header::ETAG).expect("cacheable GET must produce an ETag");
                    assert!(validator.to_str().unwrap().starts_with("W/\""));
                    assert_style_hashes(&original, &body, !htmx, theme);
                    let (status, headers, body) = send(router(mode, feature, theme), theme, htmx, Method::GET, Some(validator)).await;
                    assert_eq!(status, StatusCode::NOT_MODIFIED, "{feature:?} {mode:?} {theme:?} htmx={htmx}");
                    assert!(body.is_empty(), "304 must not carry rendered HTML or styles");
                    assert!(!headers.contains_key(header::CONTENT_TYPE));
                    assert!(headers.contains_key("server-timing"));
                    assert_eq!(headers[header::CACHE_CONTROL], "private, no-cache");
                    for name in [header::ETAG, header::CONTENT_SECURITY_POLICY, header::VARY, header::REFERRER_POLICY] {
                        assert_eq!(
                            headers.get_all(&name).iter().collect::<Vec<_>>(),
                            original.get_all(&name).iter().collect::<Vec<_>>(),
                            "304 changed {name}"
                        );
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn handler_policies_cookie_responses_errors_and_posts_do_not_gain_validators() {
    // These are selected-middleware fixtures, not real auth/session routes. They
    // exercise html_caching's existing explicit-policy/status/method safeguards.
    for feature in [Feature::Explore, Feature::Login] {
        for mode in MODES {
            for theme in [Theme::System, Theme::Tinted(&themes::tinted()[0])] {
                for htmx in [false, true] {
                    for (kind, method) in [
                        (ResponseKind::NoStore, Method::GET),
                        (ResponseKind::SetsCookie, Method::GET),
                        (ResponseKind::InvalidForm, Method::GET),
                        (ResponseKind::Cacheable, Method::POST),
                    ] {
                        let expected_status =
                            if matches!(kind, ResponseKind::InvalidForm) { StatusCode::UNPROCESSABLE_ENTITY } else { StatusCode::OK };
                        let (status, headers, body) =
                            send(response_router(mode, feature, theme, kind), theme, htmx, method, Some(&HeaderValue::from_static("*")))
                                .await;
                        assert_eq!(status, expected_status, "non-validator response must not turn into 304");
                        assert!(!headers.contains_key(header::ETAG), "{feature:?} {mode:?} {kind:?} must not acquire a validator");
                        assert_eq!(
                            headers[header::CACHE_CONTROL],
                            if matches!(kind, ResponseKind::NoStore) { "no-store" } else { "private, no-cache" }
                        );
                        assert!(body.contains("<html"));
                        assert!(headers.contains_key("server-timing"));
                        assert_eq!(headers.contains_key(header::SET_COOKIE), matches!(kind, ResponseKind::SetsCookie));
                        assert_style_hashes(&headers, &body, !htmx, theme);
                    }
                }
            }
        }
    }
}
