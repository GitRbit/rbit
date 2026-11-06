//! Per-request render timing for the footer's "Rendered in …" note.
//!
//! [`track`] records when a page request arrives; templates call [`elapsed`] while they
//! render, so the figure covers session resolution, the handler's work and the template
//! itself. The note sits between [`MARK_START`] and [`MARK_END`] so the `ETag` in
//! [`crate::middleware::html_caching`] can skip it: otherwise every render would differ.
//!
//! Partial swaps (`nav.js`) replace only part of the page and keep the old footer, so every
//! response with a rendered footer also carries that interval as `Server-Timing: view;dur=…`.
//! Navigation uses it for fresh cached/partial footers. The separate `render` metric
//! retains the total middleware interval for diagnostics.

use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use axum::{extract::Request, http::HeaderName, middleware::Next, response::Response};

mod metric;

tokio::task_local! {
    static REQUEST_TIMING: RequestTiming;
}

/// `Cell` stays within the request task's scope; the scoped future remains `Send`.
struct RequestTiming {
    start: Instant,
    view: Cell<Option<Duration>>,
}

/// Opens the timed span in the page; [`crate::middleware::html_caching`] hashes around it.
pub const MARK_START: &str = "<!--render-time-->";
/// Closes the timed span.
pub const MARK_END: &str = "<!--/render-time-->";

const SERVER_TIMING: HeaderName = HeaderName::from_static("server-timing");

/// Starts the clock for everything the request does from here on, and reports the total
/// in a `Server-Timing` header.
pub async fn track(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let (mut response, view) = REQUEST_TIMING
        .scope(RequestTiming { start, view: Cell::new(None) }, async move {
            let response = next.run(request).await;
            let view = REQUEST_TIMING.with(|timing| timing.view.get());
            (response, view)
        })
        .await;
    if let Some(value) = metric::total(start.elapsed()) {
        response.headers_mut().append(SERVER_TIMING, value);
    }
    if let Some(view) = view
        && let Some(value) = metric::view(view)
    {
        response.headers_mut().append(SERVER_TIMING, value);
    }
    response
}

/// Time since the request arrived, formatted for people (`412 µs`, `3.1 ms`, `1.20 s`);
/// `None` outside a timed request, e.g. pages rendered in tests.
pub fn elapsed() -> Option<String> {
    REQUEST_TIMING
        .try_with(|timing| {
            let elapsed = timing.start.elapsed();
            timing.view.set(Some(elapsed));
            format(elapsed)
        })
        .ok()
}

#[cfg(test)]
fn view_metric(view: Duration) -> String {
    // The footer truncates sub-millisecond durations to whole microseconds. Preserve
    // those digits; larger durations need their fractional precision so later
    // millisecond/second formatting cannot cross a rounding boundary.
    if view < Duration::from_millis(1) {
        let micros = view.as_micros();
        format!("view;dur={}.{:03}", micros / 1_000, micros % 1_000)
    } else {
        let nanos = view.as_nanos();
        format!("view;dur={}.{:06}", nanos / 1_000_000, nanos % 1_000_000)
    }
}

fn format(elapsed: Duration) -> String {
    let micros = elapsed.as_micros();
    if micros < 1_000 {
        format!("{micros} µs")
    } else if micros < 1_000_000 {
        format!("{:.1} ms", elapsed.as_secs_f64() * 1_000.0)
    } else {
        format!("{:.2} s", elapsed.as_secs_f64())
    }
}

/// Borrowed parts of `body` around its first complete render-time span. Hashing them
/// in order keeps validators stable without copying the rest of a large HTML page.
/// With no complete span, the first part is the entire body and the second is empty.
pub fn without_render_time(body: &[u8]) -> [&[u8]; 2] {
    let find = |haystack: &[u8], needle: &str| haystack.windows(needle.len()).position(|w| w == needle.as_bytes());
    let Some(start) = find(body, MARK_START) else { return [body, &[]] };
    let Some(end) = find(&body[start..], MARK_END).map(|i| start + i + MARK_END.len()) else { return [body, &[]] };
    [&body[..start], &body[end..]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn formats_each_scale() {
        assert_eq!(format(Duration::from_micros(412)), "412 µs");
        assert_eq!(format(Duration::from_micros(3_140)), "3.1 ms");
        assert_eq!(format(Duration::from_millis(1_200)), "1.20 s");
    }

    #[test]
    fn view_metric_preserves_footer_digits_without_crossing_millisecond_rounding() {
        assert_eq!(view_metric(Duration::from_nanos(512_999)), "view;dur=0.512");
        let boundary = Duration::from_nanos(3_150_100);
        assert_eq!(format(boundary), "3.2 ms");
        assert_eq!(view_metric(boundary), "view;dur=3.150100");
    }

    #[test]
    fn strips_only_the_marked_span() {
        let a = format!("<p>a{MARK_START}1 ms{MARK_END}b</p>");
        let b = format!("<p>a{MARK_START}9 ms{MARK_END}b</p>");
        assert_eq!(without_render_time(a.as_bytes()), without_render_time(b.as_bytes()));
        assert_eq!(without_render_time(a.as_bytes()).concat(), b"<p>ab</p>");
        assert_eq!(without_render_time(b"<p>plain</p>").concat(), b"<p>plain</p>");
    }

    #[test]
    fn absent_malformed_and_multiple_markers_keep_the_existing_cut() {
        let cases = [
            (String::new(), String::new()),
            (format!("a{MARK_START}missing end"), format!("a{MARK_START}missing end")),
            (format!("a{MARK_END}b"), format!("a{MARK_END}b")),
            (format!("{MARK_END}a{MARK_START}b"), format!("{MARK_END}a{MARK_START}b")),
            (format!("a{MARK_START}one{MARK_END}b{MARK_START}two{MARK_END}c"), format!("ab{MARK_START}two{MARK_END}c")),
            (format!("a{MARK_START}{MARK_START}nested{MARK_END}{MARK_END}b"), format!("a{MARK_END}b")),
            (format!("{MARK_START}all{MARK_END}"), String::new()),
        ];
        for (body, expected) in cases {
            assert_eq!(without_render_time(body.as_bytes()).concat(), expected.as_bytes(), "{body}");
        }
    }

    #[test]
    fn large_pages_borrow_the_original_body_on_both_sides() {
        let mut body = vec![b'x'; 2 * 1024 * 1024];
        let start = body.len();
        body.extend_from_slice(format!("{MARK_START}1 ms{MARK_END}").as_bytes());
        let end = body.len();
        body.extend_from_slice(b"footer");
        let [before, after] = without_render_time(&body);
        assert_eq!(before.len(), start);
        assert_eq!(before.as_ptr(), body.as_ptr());
        assert_eq!(after, b"footer");
        assert_eq!(after.as_ptr(), body[end..].as_ptr());
    }

    #[tokio::test]
    async fn elapsed_needs_a_timed_request() {
        assert_eq!(elapsed(), None);
        REQUEST_TIMING.scope(RequestTiming { start: Instant::now(), view: Cell::new(None) }, async { assert!(elapsed().is_some()) }).await;
    }

    fn metric(response: &Response, name: &str) -> Option<String> {
        response
            .headers()
            .get_all(SERVER_TIMING)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with(&format!("{name};dur=")))
            .map(str::to_owned)
    }

    async fn footer() -> Response {
        use axum::response::IntoResponse;
        // Record two actual footer calls: the last is the one emitted into HTML.
        let _ = elapsed();
        tokio::task::yield_now().await;
        let text = elapsed().unwrap();
        let recorded = REQUEST_TIMING.with(|timing| timing.view.get().unwrap());
        let mut response = axum::response::Html(format!("<main>same</main>{MARK_START}{text}{MARK_END}")).into_response();
        response.headers_mut().insert("x-test-footer-nanos", HeaderValue::from_str(&recorded.as_nanos().to_string()).unwrap());
        response.headers_mut().insert("x-test-footer-text", HeaderValue::from_str(&text.replace('µ', "u")).unwrap());
        response
    }

    #[tokio::test]
    async fn view_records_the_emitted_footer_interval_across_html_and_304() {
        use axum::{
            Router,
            body::Body,
            http::{StatusCode, header},
            middleware::from_fn,
            routing::get,
        };
        use tower::ServiceExt;

        let app = Router::new().route("/", get(footer)).layer(from_fn(crate::middleware::html_caching)).layer(from_fn(track));
        let first = app.clone().oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()).await.unwrap();
        let etag = first.headers()[header::ETAG].clone();
        let later =
            app.oneshot(Request::builder().uri("/").header(header::IF_NONE_MATCH, etag).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(later.status(), StatusCode::NOT_MODIFIED);
        for response in [first, later] {
            let nanos: u64 = response.headers()["x-test-footer-nanos"].to_str().unwrap().parse().unwrap();
            let recorded = Duration::from_nanos(nanos);
            assert_eq!(metric(&response, "view").unwrap(), view_metric(recorded));
            assert_eq!(response.headers()["x-test-footer-text"].to_str().unwrap(), format(recorded).replace('µ', "u"));
            let total: f64 = metric(&response, "render").unwrap().strip_prefix("render;dur=").unwrap().parse().unwrap();
            assert!(total + 0.001 >= recorded.as_secs_f64() * 1_000.0);
        }
    }

    #[tokio::test]
    async fn view_is_absent_without_a_footer_and_isolated_between_interleaved_requests() {
        use axum::{Router, body::Body, middleware::from_fn, routing::get};
        use std::sync::Arc;
        use tower::ServiceExt;

        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let html_barrier = Arc::clone(&barrier);
        let app = Router::new()
            .route(
                "/html",
                get(move || {
                    let barrier = Arc::clone(&html_barrier);
                    async move {
                        barrier.wait().await;
                        footer().await
                    }
                }),
            )
            .route(
                "/json",
                get(move || {
                    let barrier = Arc::clone(&barrier);
                    async move {
                        barrier.wait().await;
                        axum::Json(serde_json::json!({"ok":true}))
                    }
                }),
            )
            .layer(from_fn(track));
        let (html, json) = tokio::join!(
            app.clone().oneshot(Request::builder().uri("/html").body(Body::empty()).unwrap()),
            app.oneshot(Request::builder().uri("/json").body(Body::empty()).unwrap())
        );
        assert!(metric(&html.unwrap(), "view").is_some());
        let json = json.unwrap();
        assert!(metric(&json, "view").is_none());
        assert!(metric(&json, "render").is_some());
        assert_eq!(elapsed(), None);
    }
}
