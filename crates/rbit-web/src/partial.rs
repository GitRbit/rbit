//! Partial responses for navigation between pages of one repository.
//!
//! Between two pages of the same repository `assets/nav.js` replaces only `<main>` and the
//! repository tabs (`#repo-subnav`); the top bar, menus and footer stay mounted. Such a
//! boosted request says so in the [`PARTIAL_HEADER`] request header, carrying the
//! repository scope of the page it was sent from (`/owner/repo`, the tabs'
//! `data-repo-scope`). When the rendered page belongs to that same repository,
//! [`partial_responses`] answers with just the parts the client swaps:
//!
//! ```html
//! <title>…</title>
//! <main id="main" …>…</main>
//! <div id="repo-subnav" hx-swap-oob="true" data-repo-scope="/owner/repo" …>…</div>
//! ```
//!
//! htmx takes the title from the leading `<title>`, swaps the tabs out of band and `<main>`
//! into the target `nav.js` sets. Every other request — a page of another repository or
//! none, an error, a page without the markers — gets the full page, which `nav.js` still
//! swaps partially on its own.
//!
//! The page is cut at markers the layout emits (around `<title>` and `<main>`, in
//! `templates/base.html` and its parts in `templates/layout/`; `templates/repo/subnav.html`
//! around the tabs), the way [`crate::timing`] marks
//! the render time, so no handler or template does anything per page and the partial
//! response can only ever contain a subset of the full page (plus the `hx-swap-oob`
//! attribute). The title and tabs are searched from the start of the page and `<main>`'s end
//! from its end, so content inside `<main>` can never move a cut. Responses vary on the
//! header, and `middleware::html_caching` (outside this layer) computes the `ETag` over
//! what is actually sent, so the two shapes never share a cache entry or validator.
//!
//! The markers, the cut and the `Vary` value are decided by
//! [`rbit_presentation::navigation`]; this module only adapts requests and responses.

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rbit_presentation::navigation;
pub use rbit_presentation::navigation::{MAIN_END, MAIN_START, SUBNAV_END, SUBNAV_START, TITLE_END, TITLE_START, slice};

/// Sent by `assets/nav.js` (and its preloads) on boosted navigation inside a repository,
/// carrying the repository scope of the current page; echoed on partial responses.
pub const PARTIAL_HEADER: HeaderName = HeaderName::from_static(navigation::PARTIAL_HEADER_NAME);

/// Answers boosted `GET`s that carry [`PARTIAL_HEADER`] with only the page's title, main
/// content and repository tabs when the page belongs to the repository the header names.
pub async fn partial_responses(request: Request, next: Next) -> Response {
    let is_read = matches!(*request.method(), Method::GET | Method::HEAD);
    let scope = (is_read && request.headers().contains_key("hx-request")).then(|| request.headers().get(PARTIAL_HEADER).cloned()).flatten();
    let response = next.run(request).await;

    let is_html = response.headers().get(header::CONTENT_TYPE).is_some_and(|v| v.as_bytes().starts_with(b"text/html"));
    if !is_read || !is_html {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    // Fragments pinned by commit ids are never cut; they need not vary on the header.
    if crate::middleware::is_immutable(&parts.headers) {
        return Response::from_parts(parts, body);
    }
    append_vary(&mut parts.headers);
    let Some(scope) = scope.filter(|_| parts.status == StatusCode::OK) else {
        return Response::from_parts(parts, body);
    };

    // Pages are rendered into memory, so collecting the body costs nothing extra.
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "failed to read the response body").into_response();
    };
    let Some(partial) = slice(&bytes, scope.as_bytes()) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.insert(PARTIAL_HEADER, HeaderValue::from_static("true"));
    Response::from_parts(parts, Body::from(partial))
}

fn append_vary(headers: &mut HeaderMap) {
    let combined = navigation::vary_with_partial(headers.get(header::VARY).and_then(|v| v.to_str().ok()));
    if let Ok(value) = HeaderValue::from_str(&combined) {
        headers.insert(header::VARY, value);
    }
}

#[cfg(test)]
mod tests {
    use axum::{Router, middleware::from_fn, response::Html, routing::get};
    use tower::ServiceExt;

    use super::*;

    const SCOPE: &str = "/alice/demo";

    fn page(scope: &str) -> String {
        format!(
            "<!doctype html><html><head>{TITLE_START}<title>Issues · alice/demo</title>{TITLE_END}</head><body>\
             <header><nav>top bar</nav>{SUBNAV_START}\n<div id=\"repo-subnav\" data-repo-scope=\"{scope}\" class=\"x\"><a href=\"{scope}/issues\">Issues</a></div>\n{SUBNAV_END}</header>\
             {MAIN_START}<main id=\"main\"><p>list</p></main>{MAIN_END}<footer>footer</footer></body></html>"
        )
    }

    #[test]
    fn the_layout_emits_every_marker() {
        // base.html and the fixed parts it renders (templates/layout/), in page order.
        let layout = [
            include_str!("../templates/layout/head.html"),
            include_str!("../templates/layout/top.html"),
            include_str!("../templates/layout/menus.html"),
            include_str!("../templates/base.html"),
            include_str!("../templates/layout/foot.html"),
        ]
        .concat();
        let subnav = include_str!("../templates/repo/subnav.html");
        for marker in [TITLE_START, TITLE_END, MAIN_START, MAIN_END] {
            assert_eq!(layout.matches(marker).count(), 1, "{marker}");
        }
        for marker in [SUBNAV_START, SUBNAV_END] {
            assert_eq!(subnav.matches(marker).count(), 1, "{marker}");
        }
        let tabs = &subnav[subnav.find(SUBNAV_START).unwrap() + SUBNAV_START.len()..];
        assert!(tabs.trim_start().starts_with(navigation::SUBNAV_OPEN), "the tabs must open with {}", navigation::SUBNAV_OPEN);
    }

    fn app() -> Router {
        Router::new()
            .route("/alice/demo/issues", get(|| async { Html(page(SCOPE)) }))
            .route("/missing", get(|| async { (StatusCode::NOT_FOUND, Html(page(SCOPE))) }))
            .route("/text", get(|| async { "plain" }))
            .route("/pinned", get(|| async { crate::middleware::cache_pinned(Html(page(SCOPE)).into_response(), true) }))
            .layer(from_fn(partial_responses))
            .layer(from_fn(crate::middleware::html_caching))
    }

    async fn send(method: Method, path: &str, headers: &[(&str, &str)]) -> (Response, String) {
        let mut request = Request::builder().method(method).uri(path);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = app().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let (parts, body) = response.into_parts();
        let body = String::from_utf8(axum::body::to_bytes(body, usize::MAX).await.unwrap().to_vec()).unwrap();
        (Response::from_parts(parts, Body::empty()), body)
    }

    /// A method, path and request headers.
    type Case<'a> = (Method, &'a str, &'a [(&'a str, &'a str)]);

    const PARTIAL: [(&str, &str); 2] = [("HX-Request", "true"), ("Rbit-Partial", SCOPE)];

    #[tokio::test]
    async fn partial_requests_get_partial_pages_with_their_own_etag() {
        let (full, full_body) = send(Method::GET, "/alice/demo/issues", &[("HX-Request", "true")]).await;
        let (partial, partial_body) = send(Method::GET, "/alice/demo/issues", &PARTIAL).await;
        assert!(full_body.contains("<footer>"));
        assert!(partial_body.starts_with("<title>") && !partial_body.contains("<footer>"));
        assert!(partial_body.contains("hx-swap-oob=\"true\""));
        assert_eq!(partial.headers()[PARTIAL_HEADER], "true");
        assert!(full.headers().get(PARTIAL_HEADER).is_none());
        for response in [&full, &partial] {
            let vary = response.headers()[header::VARY].to_str().unwrap();
            assert!(vary.contains("Rbit-Partial") && vary.contains("HX-Request"), "{vary}");
        }
        let (full_etag, partial_etag) = (&full.headers()[header::ETAG], &partial.headers()[header::ETAG]);
        assert_ne!(full_etag, partial_etag);

        // Each shape revalidates against its own validator.
        let partial_etag = partial_etag.to_str().unwrap();
        let (revalidated, _) = send(Method::GET, "/alice/demo/issues", &[PARTIAL[0], PARTIAL[1], ("If-None-Match", partial_etag)]).await;
        assert_eq!(revalidated.status(), StatusCode::NOT_MODIFIED);
        let (full_again, _) = send(Method::GET, "/alice/demo/issues", &[("HX-Request", "true"), ("If-None-Match", partial_etag)]).await;
        assert_eq!(full_again.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn only_successful_htmx_reads_from_the_same_repository_are_partial() {
        let cases: [Case<'_>; 4] = [
            // Without HX-Request (a plain page load that somehow carries the header).
            (Method::GET, "/alice/demo/issues", &[("Rbit-Partial", SCOPE)]),
            // From another repository.
            (Method::GET, "/alice/demo/issues", &[("HX-Request", "true"), ("Rbit-Partial", "/alice/other")]),
            // An error page.
            (Method::GET, "/missing", &PARTIAL),
            // Not a read.
            (Method::POST, "/alice/demo/issues", &PARTIAL),
        ];
        for (method, path, headers) in cases {
            let (response, body) = send(method.clone(), path, headers).await;
            assert!(response.headers().get(PARTIAL_HEADER).is_none(), "{method} {path} {headers:?}");
            if method == Method::GET {
                assert!(body.contains("<footer>"), "{path} {headers:?}");
            }
        }
        let (pinned, body) = send(Method::GET, "/pinned", &PARTIAL).await;
        assert!(body.contains("<footer>") && pinned.headers().get(PARTIAL_HEADER).is_none());
        assert_eq!(pinned.headers()[header::VARY], "HX-Request", "pinned fragments vary on neither the epoch nor the scope");
        let (text, body) = send(Method::GET, "/text", &PARTIAL).await;
        assert_eq!(body, "plain");
        assert!(text.headers().get(header::VARY).is_none());
    }
}
