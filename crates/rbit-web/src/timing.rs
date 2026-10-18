//! Per-request render timing for the footer's "Rendered in …" note.
//!
//! [`track`] records when a page request arrives; templates call [`elapsed`] while they
//! render, so the figure covers session resolution, the handler's work and the template
//! itself. The note sits between [`MARK_START`] and [`MARK_END`] so the `ETag` in
//! [`crate::middleware::html_caching`] can skip it: otherwise every render would differ.
//!
//! Partial swaps (`nav.js`) replace only part of the page and keep the old footer, so every
//! response also carries the total as `Server-Timing: render;dur=…`, which `nav.js` copies
//! into the footer after each htmx request.

use std::time::{Duration, Instant};

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};

tokio::task_local! {
    static REQUEST_START: Instant;
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
    let mut response = REQUEST_START.scope(start, next.run(request)).await;
    let millis = start.elapsed().as_secs_f64() * 1_000.0;
    if let Ok(value) = HeaderValue::from_str(&format!("render;dur={millis:.3}")) {
        response.headers_mut().append(SERVER_TIMING, value);
    }
    response
}

/// Time since the request arrived, formatted for people (`412 µs`, `3.1 ms`, `1.20 s`);
/// `None` outside a timed request, e.g. pages rendered in tests.
pub fn elapsed() -> Option<String> {
    REQUEST_START.try_with(Instant::elapsed).ok().map(format)
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

    #[test]
    fn formats_each_scale() {
        assert_eq!(format(Duration::from_micros(412)), "412 µs");
        assert_eq!(format(Duration::from_micros(3_140)), "3.1 ms");
        assert_eq!(format(Duration::from_millis(1_200)), "1.20 s");
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
        REQUEST_START.scope(Instant::now(), async { assert!(elapsed().is_some()) }).await;
    }
}
