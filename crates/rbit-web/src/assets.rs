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

use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, PoisonError},
};

use axum::{
    extract::{Path, RawQuery},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;
use sha2::{Digest, Sha256};

#[derive(RustEmbed)]
#[folder = "assets/"]
struct EmbeddedAssets;

/// Length of the hex hash used in asset URLs.
const HASH_LEN: usize = 12;

/// Nesting limit for module imports; deeper (or cyclic) graphs keep plain specifiers.
const MAX_IMPORT_DEPTH: usize = 8;

fn hex_prefix(bytes: &[u8]) -> String {
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
            return Some(Served { bytes: Cow::Owned(rewritten.into_bytes()), hash });
        }
    }
    Some(Served { hash: hex_prefix(&file.metadata.sha256_hash()), bytes: file.data })
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

/// `GET /assets/{*path}`
///
/// Only a URL carrying the current hash is cached forever; anything else (no `?v=`, or
/// an outdated one) must be revalidated, so it can never pin old content.
pub async fn serve(Path(path): Path<String>, RawQuery(query): RawQuery) -> Response {
    let Some(file) = served(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let etag = format!("\"{}\"", file.hash);
    let version = format!("v={}", file.hash);
    let current = query.as_deref().is_some_and(|q| q.split('&').any(|pair| pair == version));
    let cache_control = if cfg!(debug_assertions) || !current { "no-cache" } else { "public, max-age=31536000, immutable" };

    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_str(mime.as_ref()).unwrap_or(HeaderValue::from_static("application/octet-stream"))),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache_control)),
            (header::ETAG, HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"\""))),
        ],
        file.bytes.clone().into_owned(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for module in ["nav.js", "theme-picker.js", "filter.js", "diff.js", "passkey.js"] {
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
        let current = serve(Path("app.js".to_owned()), RawQuery(Some(format!("v={hash}")))).await;
        let stale = serve(Path("app.js".to_owned()), RawQuery(Some("v=000000000000".to_owned()))).await;
        let bare = serve(Path("app.js".to_owned()), RawQuery(None)).await;
        let expected = if cfg!(debug_assertions) { "no-cache" } else { "public, max-age=31536000, immutable" };
        assert_eq!(current.headers()[header::CACHE_CONTROL], expected);
        assert_eq!(stale.headers()[header::CACHE_CONTROL], "no-cache");
        assert_eq!(bare.headers()[header::CACHE_CONTROL], "no-cache");
    }
}
