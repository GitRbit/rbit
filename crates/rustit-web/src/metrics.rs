//! Request metrics, exported in Prometheus format at `/metrics`.
//!
//! Instrumentation goes through the [`metrics`] facade, so any crate (the background jobs
//! in `rustit-services`, for instance) records into the same registry; [`install`] sets
//! up the Prometheus recorder that `/metrics` renders.
//!
//! Recorded here, for every request:
//!
//! * `rustit_http_requests_total{method, route, status}` — `route` is the *matched route
//!   template* (`/{owner}/{repo}/blob/{*rest}`), never the raw path, and `status` is the
//!   status class (`2xx`, `4xx`, …), so the number of series stays bounded however many
//!   repositories and files exist. Requests no route matched share `route="unmatched"`.
//! * `rustit_http_request_duration_seconds{method, route}` — time until the response
//!   headers are ready. Streamed bodies (Git packfiles) continue after that point.
//! * `rustit_git_operations_total{service, stage, status}` — smart-HTTP Git traffic:
//!   `service` is `upload-pack` (clone/fetch) or `receive-pack` (push), `stage` is
//!   `advertise` (`info/refs`) or `exchange` (the pack negotiation itself).

use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use axum::{
    extract::{MatchedPath, Request},
    http::{Method, StatusCode},
    middleware::Next,
    response::Response,
};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

/// Histogram buckets (seconds) for every `*_seconds` metric: from fast page renders to
/// long `git gc` runs.
const DURATION_BUCKETS: [f64; 14] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 120.0, 600.0];

/// How often histogram samples are folded into their buckets when nobody scrapes.
const UPKEEP_INTERVAL: Duration = Duration::from_secs(5);

/// Installs the global Prometheus recorder (once) and returns the handle that renders
/// it. Call early at start-up so metrics recorded before the first request are kept;
/// later calls return the same handle.
pub fn install() -> &'static PrometheusHandle {
    static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
    HANDLE.get_or_init(|| {
        let builder = PrometheusBuilder::new();
        let builder = match builder.set_buckets_for_metric(Matcher::Suffix("_seconds".to_owned()), &DURATION_BUCKETS) {
            Ok(builder) => builder,
            Err(error) => {
                tracing::warn!(%error, "invalid histogram buckets; using summaries");
                PrometheusBuilder::new()
            }
        };
        let recorder = builder.build_recorder();
        let handle = recorder.handle();
        if metrics::set_global_recorder(recorder).is_err() {
            tracing::warn!("a metrics recorder was already installed; /metrics will be empty");
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let upkeep = handle.clone();
            runtime.spawn(async move {
                let mut interval = tokio::time::interval(UPKEEP_INTERVAL);
                loop {
                    interval.tick().await;
                    upkeep.run_upkeep();
                }
            });
        }
        handle
    })
}

/// The current metrics in the Prometheus text exposition format.
pub fn render() -> String {
    let handle = install();
    handle.run_upkeep();
    handle.render()
}

/// Records one HTTP request. Applied to every route.
pub async fn track(request: Request, next: Next) -> Response {
    install();
    let method = method_label(request.method());
    let route = request.extensions().get::<MatchedPath>().map_or_else(|| "unmatched".to_owned(), |path| path.as_str().to_owned());
    let git = git_operation(&route, request.uri().query());
    let started = Instant::now();

    let response = next.run(request).await;

    let status = status_class(response.status());
    let elapsed = started.elapsed().as_secs_f64();
    metrics::histogram!("rustit_http_request_duration_seconds", "method" => method, "route" => route.clone()).record(elapsed);
    metrics::counter!("rustit_http_requests_total", "method" => method, "route" => route, "status" => status).increment(1);
    if let Some((service, stage)) = git {
        metrics::counter!("rustit_git_operations_total", "service" => service, "stage" => stage, "status" => status).increment(1);
    }
    response
}

/// Standard methods by name; anything else is `other`, so clients cannot mint series.
fn method_label(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::HEAD => "HEAD",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        Method::OPTIONS => "OPTIONS",
        _ => "other",
    }
}

fn status_class(status: StatusCode) -> &'static str {
    match status.as_u16() {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        _ => "5xx",
    }
}

/// `(service, stage)` for the smart-HTTP Git routes, from the matched route template and
/// (for `info/refs`) the `service` query parameter.
fn git_operation(route: &str, query: Option<&str>) -> Option<(&'static str, &'static str)> {
    if route.ends_with("/git-upload-pack") {
        return Some(("upload-pack", "exchange"));
    }
    if route.ends_with("/git-receive-pack") {
        return Some(("receive-pack", "exchange"));
    }
    if route.ends_with("/info/refs") {
        let service = query?.split('&').find_map(|pair| pair.strip_prefix("service="))?;
        return match service {
            "git-upload-pack" => Some(("upload-pack", "advertise")),
            "git-receive-pack" => Some(("receive-pack", "advertise")),
            _ => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use axum::{Router, body::Body, middleware::from_fn, routing::get};
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn labels_are_bounded() {
        assert_eq!(method_label(&Method::GET), "GET");
        assert_eq!(method_label(&Method::from_bytes(b"BREW").unwrap()), "other");
        assert_eq!(status_class(StatusCode::NOT_FOUND), "4xx");
        assert_eq!(status_class(StatusCode::from_u16(599).unwrap()), "5xx");
    }

    #[test]
    fn git_routes_are_classified() {
        assert_eq!(git_operation("/{owner}/{repo}/git-upload-pack", None), Some(("upload-pack", "exchange")));
        assert_eq!(git_operation("/{owner}/{repo}/git-receive-pack", None), Some(("receive-pack", "exchange")));
        assert_eq!(git_operation("/{owner}/{repo}/info/refs", Some("service=git-receive-pack")), Some(("receive-pack", "advertise")));
        assert_eq!(git_operation("/{owner}/{repo}/info/refs", Some("service=evil")), None);
        assert_eq!(git_operation("/{owner}/{repo}/info/refs", None), None);
        assert_eq!(git_operation("/{owner}/{repo}", None), None);
    }

    #[tokio::test]
    async fn requests_are_counted_by_route_template() {
        let app = Router::new().route("/items/{id}", get(|| async { "ok" })).layer(from_fn(track));
        for id in ["first-unique-item", "second-unique-item"] {
            let request = Request::builder().uri(format!("/items/{id}")).body(Body::empty()).unwrap();
            assert_eq!(app.clone().oneshot(request).await.unwrap().status(), StatusCode::OK);
        }

        let text = render();
        assert!(text.contains(r#"rustit_http_requests_total{method="GET",route="/items/{id}",status="2xx"}"#), "{text}");
        assert!(text.contains("rustit_http_request_duration_seconds_bucket"), "{text}");
        assert!(!text.contains("unique-item"), "raw paths must never become labels");
    }
}
