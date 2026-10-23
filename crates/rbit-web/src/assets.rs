//! Static assets embedded into the binary at compile time.
//!
//! URLs carry a per-file content hash (`/assets/app.css?v=3f9a…`) so responses can be
//! cached forever (`immutable`) and a deploy invalidates exactly the files that changed.
//! In debug builds rust-embed reads straight from disk, so CSS rebuilds show up on reload.
//!
//! JavaScript modules import each other by relative path (`import "./nav.js"`). Those
//! specifiers are rewritten to hashed URLs when a module is served, and a module's own
//! hash is taken over the rewritten source, so changing an imported module changes the
//! URL of every module that imports it, directly or not. (An import map would do the
//! same in the browser, but it needs an inline script, which the CSP forbids.)
//!
//! Text assets are Brotli-compressed once, at the highest quality, the first time a client
//! that accepts it asks for them (release builds only). The response layer's on-the-fly
//! compression trades size for speed, which never pays off for files that never change.

use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, PoisonError},
};

use axum::{
    body::Bytes,
    extract::{Path, RawQuery},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rust_embed::RustEmbed;
use sha2::{Digest, Sha256};

#[derive(RustEmbed)]
#[folder = "assets/"]
struct EmbeddedAssets;

/// Length of the hex hash used in asset URLs.
const HASH_LEN: usize = 12;

/// Nesting limit for module imports; deeper (or cyclic) graphs keep plain specifiers.
const MAX_IMPORT_DEPTH: usize = 8;

pub(crate) fn hex_prefix(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().take(HASH_LEN / 2).fold(String::with_capacity(HASH_LEN), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// An embedded file as it is served.
///
/// Both bodies are [`Bytes`], so a response shares them instead of copying: release
/// builds embed the files in the binary (`Bytes::from_static`), and the compressed form is
/// made once and reference-counted.
#[derive(Debug)]
struct Served {
    bytes: Bytes,
    /// Content hash used in URLs and as the `ETag`.
    hash: String,
    /// The bytes Brotli-compressed at quality 11, made on first use; `None` when the file
    /// is not text or does not get smaller.
    brotli: OnceLock<Option<Bytes>>,
    /// Normalized inline CSS and its CSP source, reused across rendered pages.
    style: OnceLock<(String, String)>,
}

impl Served {
    fn new(bytes: Cow<'static, [u8]>, hash: String) -> Self {
        let bytes = match bytes {
            Cow::Borrowed(embedded) => Bytes::from_static(embedded),
            Cow::Owned(read) => Bytes::from(read),
        };
        Self { bytes, hash, brotli: OnceLock::new(), style: OnceLock::new() }
    }

    /// The compressed body, compressing it first if no request has yet. Blocking: quality
    /// 11 takes a while on large files.
    fn brotli(&self, path: &str) -> Option<&Bytes> {
        self.brotli.get_or_init(|| is_compressible(path).then(|| compress(&self.bytes)).flatten().map(Bytes::from)).as_ref()
    }

    fn style(&self) -> &(String, String) {
        self.style.get_or_init(|| {
            // HTML parsers normalize CRLF/CR before checking style hashes.
            let css = std::str::from_utf8(&self.bytes).expect("embedded CSS is UTF-8").replace("\r\n", "\n").replace('\r', "\n");
            assert!(!css.to_ascii_lowercase().contains("</style"), "embedded CSS must be safe inside a style element");
            let source = format!("'sha256-{}'", STANDARD.encode(Sha256::digest(css.as_bytes())));
            (css, source)
        })
    }
}

/// The compressed body of `file` without blocking: from its cache, or made on the
/// blocking pool by the first request that needs it.
async fn brotli_of(file: &Arc<Served>, path: &str) -> Option<Bytes> {
    if let Some(cached) = file.brotli.get() {
        return cached.clone();
    }
    let file = Arc::clone(file);
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || file.brotli(&path).cloned()).await.ok().flatten()
}

/// Text formats worth compressing; images and fonts are compressed already.
fn is_compressible(path: &str) -> bool {
    let extension = path.rsplit_once('.').map_or("", |(_, extension)| extension);
    ["css", "js", "json", "svg", "txt", "webmanifest", "xml", "html"].iter().any(|text| extension.eq_ignore_ascii_case(text))
}

fn compress(bytes: &[u8]) -> Option<Vec<u8>> {
    let params = brotli::enc::BrotliEncoderParams { quality: 11, lgwin: 22, ..Default::default() };
    let mut out = Vec::with_capacity(bytes.len() / 3);
    brotli::BrotliCompress(&mut &bytes[..], &mut out, &params).ok()?;
    (out.len() < bytes.len()).then_some(out)
}

/// Whether an `Accept-Encoding` header admits Brotli (`br`, without `q=0`).
fn accepts_brotli(headers: &HeaderMap) -> bool {
    headers.get_all(header::ACCEPT_ENCODING).iter().filter_map(|value| value.to_str().ok()).flat_map(|value| value.split(',')).any(
        |coding| {
            let mut parts = coding.split(';').map(str::trim);
            parts.next().is_some_and(|name| name.eq_ignore_ascii_case("br"))
                && !parts.any(|param| param.strip_prefix("q=").is_some_and(|q| q.parse::<f32>().is_ok_and(|q| q <= 0.0)))
        },
    )
}

fn is_javascript(path: &str) -> bool {
    std::path::Path::new(path).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("js"))
}

/// The served form of `path`, memoised in release builds (assets are fixed at compile
/// time there; debug builds re-read them from disk on every request).
fn served(path: &str) -> Option<Arc<Served>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Served>>>> = OnceLock::new();
    if cfg!(debug_assertions) {
        return prepare(path, 0).map(Arc::new);
    }
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some(found) = cache.lock().unwrap_or_else(PoisonError::into_inner).get(path) {
        return Some(found.clone());
    }
    let prepared = Arc::new(prepare(path, 0)?);
    cache.lock().unwrap_or_else(PoisonError::into_inner).insert(path.to_owned(), prepared.clone());
    Some(prepared)
}

fn prepare(path: &str, depth: usize) -> Option<Served> {
    let file = EmbeddedAssets::get(path)?;
    let is_module = is_javascript(path) && !path.starts_with("vendor/");
    let is_document = std::path::Path::new(path).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("html"));
    if (is_module || is_document)
        && depth < MAX_IMPORT_DEPTH
        && let Ok(source) = std::str::from_utf8(&file.data)
    {
        let rewritten = if is_module { rewrite_imports(source, path, depth) } else { rewrite_sources(source, depth) };
        if rewritten != source {
            let hash = hex_prefix(&Sha256::digest(rewritten.as_bytes()));
            return Some(Served::new(Cow::Owned(rewritten.into_bytes()), hash));
        }
    }
    Some(Served::new(file.data, hex_prefix(&file.metadata.sha256_hash())))
}

/// Appends `?v=<hash>` to the relative specifier of every static `import`/`export … from`
/// statement that names an embedded module. Statements are recognised by line (the
/// line starts with `import`/`export`, or with `}` and contains ` from `), which is how
/// these hand-written modules are formatted; strings and comments elsewhere are never
/// touched.
fn rewrite_imports(source: &str, path: &str, depth: usize) -> String {
    let directory = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut out = String::with_capacity(source.len() + 64);
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let is_statement =
            trimmed.starts_with("import") || trimmed.starts_with("export") || (trimmed.starts_with('}') && trimmed.contains(" from "));
        match is_statement.then(|| rewrite_specifier(line, directory, depth)).flatten() {
            Some(rewritten) => out.push_str(&rewritten),
            None => out.push_str(line),
        }
    }
    out
}

/// Appends `?v=<hash>` to every `src="/assets/…"` attribute of an embedded HTML document
/// that names an embedded file without a query, so the document's subresources are
/// `immutable` like the pages' and its own hash changes with theirs.
fn rewrite_sources(source: &str, depth: usize) -> String {
    const OPEN: &str = "src=\"/assets/";
    let mut out = String::with_capacity(source.len() + 32);
    let mut rest = source;
    while let Some(at) = rest.find(OPEN) {
        let start = at + OPEN.len();
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find('"') else { break };
        let path = &rest[..end];
        out.push_str(path);
        if !path.contains('?')
            && let Some(dependency) = prepare(path, depth + 1)
        {
            out.push_str("?v=");
            out.push_str(&dependency.hash);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// `line` with its last quoted specifier versioned, if it names an embedded module.
fn rewrite_specifier(line: &str, directory: &str, depth: usize) -> Option<String> {
    let end = line.rfind(['"', '\''])?;
    let quote = line[end..].chars().next()?;
    let start = line[..end].rfind(quote)? + 1;
    let specifier = &line[start..end];
    if !is_javascript(specifier) {
        return None;
    }
    let dependency = prepare(&resolve(directory, specifier)?, depth + 1)?;
    Some(format!("{}?v={}{}", &line[..end], dependency.hash, &line[end..]))
}

/// `./x.js` / `../x.js` relative to `directory`, as an embedded path; `None` for bare or
/// absolute specifiers and paths that leave the asset folder.
fn resolve(directory: &str, specifier: &str) -> Option<String> {
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return None;
    }
    let mut parts: Vec<&str> = directory.split('/').filter(|p| !p.is_empty()).collect();
    for part in specifier.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name),
        }
    }
    Some(parts.join("/"))
}

/// Cache-busting URL for an embedded file, e.g. `url("app.css")`.
pub fn url(path: &str) -> String {
    if let Some(file) = served(path) {
        format!("/assets/{path}?v={}", file.hash)
    } else {
        tracing::warn!(path, "unknown asset referenced; did you run `npm run build`?");
        format!("/assets/{path}")
    }
}

/// A hash over every embedded file; changes whenever any asset changes.
/// Used to version the service worker's cache.
pub fn build_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let mut names: Vec<_> = EmbeddedAssets::iter().collect();
        names.sort();
        let mut hasher = Sha256::new();
        for name in names {
            if let Some(file) = EmbeddedAssets::get(&name) {
                hasher.update(name.as_bytes());
                hasher.update(file.metadata.sha256_hash());
            }
        }
        hex_prefix(&hasher.finalize())
    })
}

/// Raw bytes of an embedded file, for routes that post-process assets (e.g. `sw.js`).
pub fn read(path: &str) -> Option<Cow<'static, [u8]>> {
    EmbeddedAssets::get(path).map(|file| file.data)
}

/// Trusted embedded CSS, emitted verbatim inside a style element. Never accepts user CSS.
///
/// Renders straight from the shared, memoised file: a page writes the stylesheet into its
/// output without first copying it into a string of its own.
pub fn inline_css(path: &str) -> InlineCss {
    InlineCss(served(path).expect("initial stylesheet must be embedded; run npm run build"))
}

/// Embedded CSS as rendered by [`inline_css`].
#[derive(Debug)]
pub struct InlineCss(Arc<Served>);

impl InlineCss {
    pub fn as_str(&self) -> &str {
        &self.0.style().0
    }
}

impl std::fmt::Display for InlineCss {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<InlineCss> for &str {
    fn eq(&self, other: &InlineCss) -> bool {
        *self == other.as_str()
    }
}

tokio::task_local! {
    /// Whether pages rendered by the current request carry their stylesheets inline.
    static HEAD_STYLES: bool;
}

/// Whether the page being rendered should inline its stylesheets in `<head>`: `false`
/// inside [`with_head_styles`]`(false, …)`, otherwise `true`.
pub fn head_styles() -> bool {
    HEAD_STYLES.try_with(|include| *include).unwrap_or(true)
}

/// Runs `future` (a request's handling) with [`head_styles`] answering `include`.
pub async fn with_head_styles<F: Future>(include: bool, future: F) -> F::Output {
    HEAD_STYLES.scope(include, future).await
}

/// CSP source authorizing exactly the bytes emitted by `inline_css`.
pub fn style_source(path: &str) -> String {
    served(path).expect("initial stylesheet must be embedded; run npm run build").style().1.clone()
}

/// `GET /assets/{*path}`
///
/// Only a URL carrying the current hash (or the current [`build_version`]) is cached
/// forever; anything else (no `?v=`, or an outdated one) must be revalidated, so it can
/// never pin old content. Revalidations naming the current content are answered with
/// `304 Not Modified`.
pub async fn serve(Path(path): Path<String>, RawQuery(query): RawQuery, request_headers: HeaderMap) -> Response {
    let mut response = serve_file(&path, query.as_deref(), &request_headers).await;
    // Mermaid needs generated SVG styles. Confine that exception to an opaque
    // sandboxed frame; repository HTML never receives these permissions.
    if path == "mermaid-frame.html" {
        let headers = response.headers_mut();
        if headers.contains_key(header::CONTENT_TYPE) {
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
        }
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("SAMEORIGIN"));
        headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(
            "default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; img-src data:; sandbox allow-scripts; frame-ancestors 'self'",
        ));
    }
    response
}

async fn serve_file(path: &str, query: Option<&str>, request_headers: &HeaderMap) -> Response {
    let Some(file) = served(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let current = query.is_some_and(|query| is_current(query, &file.hash));
    let cache_control =
        HeaderValue::from_static(if cfg!(debug_assertions) || !current { "no-cache" } else { "public, max-age=31536000, immutable" });

    // Both encodings decode to the same bytes, named by the same content hash, so either
    // validator still matching means the client's copy is current.
    if let Some(etag) = request_headers.get(header::IF_NONE_MATCH).and_then(|tags| matching_etag(tags, &file.hash)) {
        let mut response = (StatusCode::NOT_MODIFIED, [(header::CACHE_CONTROL, cache_control), (header::ETAG, etag)]).into_response();
        if is_compressible(path) {
            response.headers_mut().insert(header::VARY, HeaderValue::from_static("accept-encoding"));
        }
        return response;
    }

    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_str(mime.as_ref()).unwrap_or(HeaderValue::from_static("application/octet-stream"))),
        (header::CACHE_CONTROL, cache_control),
    ];
    // Debug builds re-read assets on every request; compressing them at quality 11 each
    // time would only slow development down, so the response layer handles those.
    let compressed = if !cfg!(debug_assertions) && is_compressible(path) && accepts_brotli(request_headers) {
        brotli_of(&file, path).await
    } else {
        None
    };
    if let Some(compressed) = compressed {
        return (
            headers,
            [
                (header::CONTENT_ENCODING, HeaderValue::from_static("br")),
                (header::VARY, HeaderValue::from_static("accept-encoding")),
                (header::ETAG, etag_value(&file.hash, "-br")),
            ],
            compressed,
        )
            .into_response();
    }

    (headers, [(header::ETAG, etag_value(&file.hash, ""))], file.bytes.clone()).into_response()
}

/// Whether `query` versions a file with `hash`: by its own hash, or by the current
/// [`build_version`] (scripts that cannot know a file's hash, like the theme picker linking
/// one of ~500 theme sheets, use that; content is just as fixed per build).
fn is_current(query: &str, hash: &str) -> bool {
    query.split('&').filter_map(|pair| pair.strip_prefix("v=")).any(|version| version == hash || version == build_version())
}

/// The strong `ETag` of a representation: the content hash, plus `-br` when compressed.
fn etag_value(hash: &str, suffix: &str) -> HeaderValue {
    HeaderValue::from_str(&format!("\"{hash}{suffix}\"")).unwrap_or(HeaderValue::from_static("\"\""))
}

/// The `ETag` to confirm when `If-None-Match` names the content `hash` in either encoding
/// (or is `*`). `If-None-Match` uses weak comparison, so `W/` prefixes are ignored.
fn matching_etag(tags: &HeaderValue, hash: &str) -> Option<HeaderValue> {
    tags.to_str().ok()?.split(',').map(str::trim).find_map(|tag| {
        if tag == "*" {
            return Some(etag_value(hash, ""));
        }
        let opaque = tag.trim_start_matches("W/").strip_prefix('"')?.strip_suffix('"')?;
        let suffix = opaque.strip_prefix(hash)?;
        matches!(suffix, "" | "-br").then(|| etag_value(hash, suffix))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mermaid_style_permissions_are_confined_to_a_sandboxed_frame() {
        use tower::ServiceExt as _;

        let app = axum::Router::new()
            .route("/assets/{*path}", axum::routing::get(serve))
            .layer(axum::middleware::from_fn(crate::middleware::security_headers));
        let response = app
            .oneshot(axum::http::Request::builder().uri("/assets/mermaid-frame.html").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let policy = response.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(policy.contains("sandbox allow-scripts;"));
        assert!(!policy.contains("allow-same-origin"));
        assert!(policy.contains("script-src 'self';"));
        assert!(policy.contains("style-src 'unsafe-inline';"));
        assert_eq!(response.headers()[header::X_FRAME_OPTIONS], "SAMEORIGIN");
    }

    #[tokio::test]
    async fn the_mermaid_frame_loads_its_renderer_from_an_immutable_url_and_revalidates() {
        let renderer = served("vendor/mermaid-frame.js").unwrap().hash.clone();
        let frame = served("mermaid-frame.html").unwrap();
        let html = std::str::from_utf8(&frame.bytes).unwrap();
        assert!(html.contains(&format!("src=\"/assets/vendor/mermaid-frame.js?v={renderer}\"")), "{html}");
        assert_ne!(frame.hash, hex_prefix(&EmbeddedAssets::get("mermaid-frame.html").unwrap().metadata.sha256_hash()));

        let response = serve(Path("mermaid-frame.html".to_owned()), RawQuery(None), HeaderMap::new()).await;
        let etag = response.headers()[header::ETAG].clone();
        assert_eq!(etag, format!("\"{}\"", frame.hash).as_str());
        let revalidated =
            serve(Path("mermaid-frame.html".to_owned()), RawQuery(None), HeaderMap::from_iter([(header::IF_NONE_MATCH, etag)])).await;
        assert_eq!(revalidated.status(), StatusCode::NOT_MODIFIED);
        assert!(revalidated.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("sandbox allow-scripts;"));
    }

    #[test]
    fn document_sources_get_hashed_urls_once() {
        let htmx = served("vendor/htmx.min.js").unwrap().hash.clone();
        let source = "<script src=\"/assets/vendor/htmx.min.js\"></script><img src=\"/assets/missing.png\"><script src=\"/assets/vendor/htmx.min.js?v=1\"></script>";
        assert_eq!(
            rewrite_sources(source, 0),
            format!(
                "<script src=\"/assets/vendor/htmx.min.js?v={htmx}\"></script><img src=\"/assets/missing.png\"><script src=\"/assets/vendor/htmx.min.js?v=1\"></script>"
            )
        );
    }

    #[test]
    fn urls_versioned_by_file_or_build_are_current() {
        let hash = served("themes/base24-berlin.css").unwrap().hash.clone();
        assert!(is_current(&format!("v={hash}"), &hash));
        assert!(is_current(&format!("x=1&v={}", build_version()), &hash), "theme-picker.js links theme sheets by build");
        assert!(!is_current("v=000000000000", &hash));
        assert!(!is_current(&format!("w={hash}"), &hash));
        assert!(!is_current("", &hash));
    }

    #[test]
    fn inline_style_hashes_follow_html_newline_normalization() {
        let file = Served::new(Cow::Borrowed(b"body {\r\ncolor: red;\r}\n"), String::new());
        let (css, source) = file.style();
        assert_eq!(css, "body {\ncolor: red;\n}\n");
        assert_eq!(source, &format!("'sha256-{}'", STANDARD.encode(Sha256::digest(css.as_bytes()))));
    }

    #[test]
    fn relative_imports_get_hashed_urls() {
        let nav = served("nav.js").expect("nav.js is embedded");
        let source = rewrite_imports(
            "// it's \"./nav.js\"\nimport \"./nav.js\";\nimport { x } from './nav.js';\nimport {\n  y,\n} from \"./nav.js\";\nconst s = \"./nav.js\";",
            "app.js",
            0,
        );
        assert_eq!(
            source,
            format!(
                "// it's \"./nav.js\"\nimport \"./nav.js?v={0}\";\nimport {{ x }} from './nav.js?v={0}';\nimport {{\n  y,\n}} from \"./nav.js?v={0}\";\nconst s = \"./nav.js\";",
                nav.hash
            )
        );
        // Unknown files, bare specifiers and escapes from the asset folder are left alone.
        let untouched = "import \"./missing.js\"; import \"htmx\"; import \"../../etc.js\";";
        assert_eq!(rewrite_imports(untouched, "app.js", 0), untouched);
    }

    #[test]
    fn a_module_url_changes_with_its_imports() {
        let app = served("app.js").expect("app.js is embedded");
        let source = std::str::from_utf8(&app.bytes).unwrap();
        for module in ["nav.js", "preload.js", "theme-picker.js", "filter.js", "diff.js", "blocks.js", "passkey.js"] {
            let expected = format!("\"./{module}?v={}\"", served(module).unwrap().hash);
            assert!(source.contains(&expected), "app.js should import {expected}");
        }
        assert_ne!(app.hash, hex_prefix(&EmbeddedAssets::get("app.js").unwrap().metadata.sha256_hash()));
        assert_eq!(url("app.js"), format!("/assets/app.js?v={}", app.hash));
    }

    #[test]
    fn resolves_relative_specifiers_inside_the_asset_folder() {
        assert_eq!(resolve("", "./nav.js").as_deref(), Some("nav.js"));
        assert_eq!(resolve("themes", "../nav.js").as_deref(), Some("nav.js"));
        assert_eq!(resolve("", "../nav.js"), None);
        assert_eq!(resolve("", "nav.js"), None);
    }

    #[tokio::test]
    async fn only_current_urls_are_immutable() {
        let hash = served("app.js").unwrap().hash.clone();
        let current = serve(Path("app.js".to_owned()), RawQuery(Some(format!("v={hash}"))), HeaderMap::new()).await;
        let stale = serve(Path("app.js".to_owned()), RawQuery(Some("v=000000000000".to_owned())), HeaderMap::new()).await;
        let bare = serve(Path("app.js".to_owned()), RawQuery(None), HeaderMap::new()).await;
        let expected = if cfg!(debug_assertions) { "no-cache" } else { "public, max-age=31536000, immutable" };
        assert_eq!(current.headers()[header::CACHE_CONTROL], expected);
        assert_eq!(stale.headers()[header::CACHE_CONTROL], "no-cache");
        assert_eq!(bare.headers()[header::CACHE_CONTROL], "no-cache");
    }

    #[test]
    fn reads_brotli_from_accept_encoding() {
        let accept = |value: &'static str| HeaderMap::from_iter([(header::ACCEPT_ENCODING, HeaderValue::from_static(value))]);
        assert!(accepts_brotli(&accept("gzip, deflate, br, zstd")));
        assert!(accepts_brotli(&accept("BR;q=0.5")));
        assert!(!accepts_brotli(&accept("gzip, br;q=0")));
        assert!(!accepts_brotli(&accept("gzip, brotli")));
        assert!(!accepts_brotli(&HeaderMap::new()));
    }

    #[test]
    fn text_assets_compress_once_and_round_trip() {
        let css = served("app.css").expect("app.css is embedded");
        let compressed = css.brotli("app.css").expect("app.css compresses");
        assert!(compressed.len() < css.bytes.len());
        assert!(std::ptr::eq(compressed, css.brotli("app.css").unwrap()), "compressed once");
        let mut decoded = Vec::new();
        brotli::BrotliDecompress(&mut &compressed[..], &mut decoded).unwrap();
        assert_eq!(decoded, css.bytes.as_ref());
        assert!(served("icon-96.png").unwrap().brotli("icon-96.png").is_none(), "images are left alone");
    }

    #[tokio::test]
    async fn responses_share_embedded_and_compressed_bytes_instead_of_copying() {
        static EMBEDDED: &[u8] = b"body { color: red; }\n";
        let file = Arc::new(Served::new(Cow::Borrowed(EMBEDDED), String::new()));
        assert_eq!(file.bytes.as_ptr(), EMBEDDED.as_ptr(), "embedded bytes are borrowed, not copied");

        let css = Arc::new(Served::new(Cow::Owned(assets_bytes("app.css")), String::new()));
        let first = brotli_of(&css, "app.css").await.expect("app.css compresses");
        let second = brotli_of(&css, "app.css").await.unwrap();
        assert_eq!(first.as_ptr(), second.as_ptr(), "compressed once, then shared");
        assert_eq!(first.as_ptr(), css.brotli.get().unwrap().as_ref().unwrap().as_ptr());
    }

    fn assets_bytes(path: &str) -> Vec<u8> {
        read(path).unwrap().into_owned()
    }

    #[tokio::test]
    async fn current_validators_are_answered_with_not_modified() {
        let hash = served("app.js").unwrap().hash.clone();
        let revalidate = |tags: &str| {
            let headers = HeaderMap::from_iter([(header::IF_NONE_MATCH, HeaderValue::from_str(tags).unwrap())]);
            serve(Path("app.js".to_owned()), RawQuery(Some(format!("v={hash}"))), headers)
        };
        for (tags, confirmed) in [
            (format!("\"{hash}\""), format!("\"{hash}\"")),
            (format!("\"other\", W/\"{hash}-br\""), format!("\"{hash}-br\"")),
            ("*".to_owned(), format!("\"{hash}\"")),
        ] {
            let response = revalidate(&tags).await;
            assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "{tags}");
            assert_eq!(response.headers()[header::ETAG], confirmed.as_str());
            assert!(response.headers().contains_key(header::CACHE_CONTROL));
            assert!(axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().is_empty(), "no body is sent");
        }
        for stale in ["\"000000000000\"", &format!("\"{hash}-gzip\""), &format!("\"{hash}x\"")] {
            let response = revalidate(stale).await;
            assert_eq!(response.status(), StatusCode::OK, "{stale}");
            assert!(!axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn release_assets_are_served_precompressed() {
        let accept = HeaderMap::from_iter([(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip, br"))]);
        let response = serve(Path("app.css".to_owned()), RawQuery(None), accept).await;
        if cfg!(debug_assertions) {
            assert!(response.headers().get(header::CONTENT_ENCODING).is_none());
        } else {
            assert_eq!(response.headers()[header::CONTENT_ENCODING], "br");
            assert_eq!(response.headers()[header::VARY], "accept-encoding");
            assert!(response.headers()[header::ETAG].to_str().unwrap().ends_with("-br\""));
        }
    }
}
