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
//! The page is cut at markers the layout emits (`templates/base.html` around `<title>` and
//! `<main>`, `templates/repo/subnav.html` around the tabs), the way [`crate::timing`] marks
//! the render time, so no handler or template does anything per page and the partial
//! response can only ever contain a subset of the full page (plus the `hx-swap-oob`
//! attribute). The title and tabs are searched from the start of the page and `<main>`'s end
//! from its end, so content inside `<main>` can never move a cut. Responses vary on the
//! header, and `middleware::html_caching` (outside this layer) computes the `ETag` over
//! what is actually sent, so the two shapes never share a cache entry or validator.

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Sent by `assets/nav.js` (and its preloads) on boosted navigation inside a repository,
/// carrying the repository scope of the current page; echoed on partial responses.
pub const PARTIAL_HEADER: HeaderName = HeaderName::from_static("rbit-partial");

/// Spelling of [`PARTIAL_HEADER`] in `Vary`.
const VARY_NAME: &str = "Rbit-Partial";

/// Opens the document title in `base.html`.
pub const TITLE_START: &str = "<!--partial:title-->";
/// Closes the document title.
pub const TITLE_END: &str = "<!--/partial:title-->";
/// Opens the repository tabs in `repo/subnav.html`.
pub const SUBNAV_START: &str = "<!--partial:subnav-->";
/// Closes the repository tabs.
pub const SUBNAV_END: &str = "<!--/partial:subnav-->";
/// Opens `<main>` in `base.html`.
pub const MAIN_START: &str = "<!--partial:main-->";
/// Closes `<main>`.
pub const MAIN_END: &str = "<!--/partial:main-->";

/// How the tabs start in `repo/subnav.html`, up to the scope's value.
const SUBNAV_OPEN: &str = "<div id=\"repo-subnav\" data-repo-scope=\"";
/// Where the out-of-band attribute goes: right after the tabs' id.
const SUBNAV_ID: &str = "<div id=\"repo-subnav\"";

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

/// The partial form of `page` for a request from the repository `scope`: its title, main
/// content and out-of-band repository tabs; `None` (send the full page) when a marker is
/// missing or out of order, or the page belongs to another repository or none.
pub fn slice(page: &[u8], scope: &[u8]) -> Option<Vec<u8>> {
    let title = between(page, 0, TITLE_START, TITLE_END)?;
    let subnav = between(page, title.end, SUBNAV_START, SUBNAV_END)?;
    let main_start = find(page, subnav.end, MAIN_START)? + MAIN_START.len();
    let main_end = rfind(page, MAIN_END).filter(|&end| end >= main_start)?;

    let tabs = trim(&page[subnav.clone()]);
    let rest = tabs.strip_prefix(SUBNAV_OPEN.as_bytes())?;
    let same_repository = !scope.is_empty() && rest.strip_prefix(scope).is_some_and(|after| after.starts_with(b"\""));
    if !same_repository {
        return None;
    }

    let mut out = Vec::with_capacity(title.len() + (main_end - main_start) + tabs.len() + 32);
    out.extend_from_slice(trim(&page[title]));
    out.push(b'\n');
    out.extend_from_slice(trim(&page[main_start..main_end]));
    out.push(b'\n');
    out.extend_from_slice(SUBNAV_ID.as_bytes());
    out.extend_from_slice(b" hx-swap-oob=\"true\"");
    out.extend_from_slice(&tabs[SUBNAV_ID.len()..]);
    out.push(b'\n');
    Some(out)
}

/// The bytes between the first `start` marker at or after `from` and the next `end`.
fn between(page: &[u8], from: usize, start: &str, end: &str) -> Option<std::ops::Range<usize>> {
    let open = find(page, from, start)? + start.len();
    let close = find(page, open, end)?;
    Some(open..close)
}

fn find(haystack: &[u8], from: usize, needle: &str) -> Option<usize> {
    haystack.get(from..)?.windows(needle.len()).position(|w| w == needle.as_bytes()).map(|i| from + i)
}

fn rfind(haystack: &[u8], needle: &str) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle.as_bytes())
}

fn trim(bytes: &[u8]) -> &[u8] {
    bytes.trim_ascii()
}

fn append_vary(headers: &mut HeaderMap) {
    let combined = match headers.get(header::VARY).and_then(|v| v.to_str().ok()) {
        Some(existing) if !existing.is_empty() => format!("{existing}, {VARY_NAME}"),
        _ => VARY_NAME.to_owned(),
    };
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
    fn same_repository_pages_are_cut_to_title_main_and_tabs() {
        let partial = String::from_utf8(slice(page(SCOPE).as_bytes(), SCOPE.as_bytes()).unwrap()).unwrap();
        assert_eq!(
            partial,
            "<title>Issues · alice/demo</title>\n<main id=\"main\"><p>list</p></main>\n\
             <div id=\"repo-subnav\" hx-swap-oob=\"true\" data-repo-scope=\"/alice/demo\" class=\"x\"><a href=\"/alice/demo/issues\">Issues</a></div>\n"
        );
        assert!(!partial.contains("top bar") && !partial.contains("footer"));
    }

    #[test]
    fn other_repositories_get_the_full_page() {
        let full = page(SCOPE);
        for scope in ["/alice/demo2", "/alice/dem", "/alice", "", "/bob/demo"] {
            assert_eq!(slice(full.as_bytes(), scope.as_bytes()), None, "{scope:?}");
        }
    }

    #[test]
    fn pages_without_every_marker_get_the_full_page() {
        let full = page(SCOPE);
        for marker in [TITLE_START, TITLE_END, SUBNAV_START, SUBNAV_END, MAIN_START, MAIN_END] {
            let without = full.replacen(marker, "", 1);
            assert_eq!(slice(without.as_bytes(), SCOPE.as_bytes()), None, "{marker}");
        }
        // Out of order: the tabs after <main>.
        let swapped = format!(
            "{TITLE_START}<title>t</title>{TITLE_END}{MAIN_START}<main id=\"main\"></main>{MAIN_END}{SUBNAV_START}<div id=\"repo-subnav\" data-repo-scope=\"{SCOPE}\"></div>{SUBNAV_END}"
        );
        assert_eq!(slice(swapped.as_bytes(), SCOPE.as_bytes()), None);
        // Markup the cut does not recognise.
        let other = full.replace("<div id=\"repo-subnav\" data-repo-scope", "<div data-repo-scope");
        assert_eq!(slice(other.as_bytes(), SCOPE.as_bytes()), None);
    }

    #[test]
    fn markers_inside_main_cannot_move_the_cut() {
        let full = page(SCOPE).replace("<p>list</p>", &format!("<p>{MAIN_END}{SUBNAV_START}{MAIN_START}</p>"));
        let partial = String::from_utf8(slice(full.as_bytes(), SCOPE.as_bytes()).unwrap()).unwrap();
        assert!(partial.contains(&format!("<main id=\"main\"><p>{MAIN_END}{SUBNAV_START}{MAIN_START}</p></main>")), "{partial}");
        assert!(!partial.contains("footer"));
    }

    #[test]
    fn the_layout_emits_every_marker() {
        let base = include_str!("../templates/base.html");
        let subnav = include_str!("../templates/repo/subnav.html");
        for marker in [TITLE_START, TITLE_END, MAIN_START, MAIN_END] {
            assert_eq!(base.matches(marker).count(), 1, "{marker}");
        }
        for marker in [SUBNAV_START, SUBNAV_END] {
            assert_eq!(subnav.matches(marker).count(), 1, "{marker}");
        }
        let tabs = &subnav[subnav.find(SUBNAV_START).unwrap() + SUBNAV_START.len()..];
        assert!(tabs.trim_start().starts_with(SUBNAV_OPEN), "the tabs must open with {SUBNAV_OPEN}");
    }

    fn app() -> Router {
        Router::new()
            .route("/alice/demo/issues", get(|| async { Html(page(SCOPE)) }))
            .route("/missing", get(|| async { (StatusCode::NOT_FOUND, Html(page(SCOPE))) }))
            .route("/text", get(|| async { "plain" }))
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
        let (text, body) = send(Method::GET, "/text", &PARTIAL).await;
        assert_eq!(body, "plain");
        assert!(text.headers().get(header::VARY).is_none());
    }
}
