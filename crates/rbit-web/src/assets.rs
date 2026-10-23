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
#[derive(Debug)]
struct Served {
    bytes: Cow<'static, [u8]>,
    /// Content hash used in URLs and as the `ETag`.
    hash: String,
    /// The bytes Brotli-compressed at quality 11, made on first use; `None` when the file
    /// is not text or does not get smaller.
    brotli: OnceLock<Option<Vec<u8>>>,
    /// Normalized inline CSS and its CSP source, reused across rendered pages.
    style: OnceLock<(String, String)>,
}

impl Served {
    fn new(bytes: Cow<'static, [u8]>, hash: String) -> Self {
        Self { bytes, hash, brotli: OnceLock::new(), style: OnceLock::new() }
    }

    fn brotli(&self, path: &str) -> Option<&[u8]> {
        self.brotli.get_or_init(|| is_compressible(path).then(|| compress(&self.bytes)).flatten()).as_deref()
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
    if is_module
        && depth < MAX_IMPORT_DEPTH
        && let Ok(source) = std::str::from_utf8(&file.data)
    {
        let rewritten = rewrite_imports(source, path, depth);
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
pub fn inline_css(path: &str) -> String {
    let file = served(path).expect("initial stylesheet must be embedded; run npm run build");
    file.style().0.clone()
}

/// CSP source authorizing exactly the bytes emitted by `inline_css`.
pub fn style_source(path: &str) -> String {
    served(path).expect("initial stylesheet must be embedded; run npm run build").style().1.clone()
}

/// `GET /assets/{*path}`
///
/// Only a URL carrying the current hash is cached forever; anything else (no `?v=`, or
/// an outdated one) must be revalidated, so it can never pin old content.
pub async fn serve(Path(path): Path<String>, RawQuery(query): RawQuery, request_headers: HeaderMap) -> Response {
    let Some(file) = served(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // Mermaid needs generated SVG styles. Confine that exception to an opaque
    // sandboxed frame; repository HTML never receives these permissions.
    if path == "mermaid-frame.html" {
        return ([
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
            (header::X_FRAME_OPTIONS, "SAMEORIGIN"),
            (header::CONTENT_SECURITY_POLICY,
             "default-src 'none'; script-src 'self'; style-src 'unsafe-inline'; img-src data:; sandbox allow-scripts; frame-ancestors 'self'"),
        ], file.bytes.clone().into_owned()).into_response();
    }

    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let etag = format!("\"{}\"", file.hash);
    let version = format!("v={}", file.hash);
    let current = query.as_deref().is_some_and(|q| q.split('&').any(|pair| pair == version));
    let cache_control = if cfg!(debug_assertions) || !current { "no-cache" } else { "public, max-age=31536000, immutable" };
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_str(mime.as_ref()).unwrap_or(HeaderValue::from_static("application/octet-stream"))),
        (header::CACHE_CONTROL, HeaderValue::from_static(cache_control)),
    ];

    // Debug builds re-read assets on every request; compressing them at quality 11 each
    // time would only slow development down, so the response layer handles those.
    if !cfg!(debug_assertions) && is_compressible(&path) && accepts_brotli(&request_headers) {
        let compressed = tokio::task::spawn_blocking({
            let file = file.clone();
            move || file.brotli(&path).map(<[u8]>::to_vec)
        })
        .await
        .ok()
        .flatten();
        if let Some(compressed) = compressed {
            let etag = format!("\"{}-br\"", file.hash);
            return (
                headers,
                [
                    (header::CONTENT_ENCODING, HeaderValue::from_static("br")),
                    (header::VARY, HeaderValue::from_static("accept-encoding")),
                    (header::ETAG, HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"\""))),
                ],
                compressed,
            )
                .into_response();
        }
    }

    let etag = HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"\""));
    (headers, [(header::ETAG, etag)], file.bytes.clone().into_owned()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mermaid_style_permissions_are_confined_to_a_sandboxed_frame() {
        use tower::ServiceExt as _;

        let app = axum::Router::new().route("/assets/{*path}", axum::routing::get(serve)).layer(axum::middleware::from_fn_with_state(
            crate::middleware::SecurityHeaders::default(),
            crate::middleware::security_headers,
        ));
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
