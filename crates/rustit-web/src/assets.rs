//! Static assets embedded into the binary at compile time.
//!
//! URLs carry a per-file content hash (`/assets/app.css?v=3f9a…`) so responses can be
//! cached forever (`immutable`) and a deploy invalidates exactly the files that changed.
//! In debug builds rust-embed reads straight from disk, so CSS rebuilds show up on reload.

use std::sync::OnceLock;

use axum::{
    extract::Path,
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

fn hex_prefix(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().take(HASH_LEN / 2).fold(String::with_capacity(HASH_LEN), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Cache-busting URL for an embedded file, e.g. `url("app.css")`.
pub fn url(path: &str) -> String {
    if let Some(file) = EmbeddedAssets::get(path) {
        format!("/assets/{path}?v={}", hex_prefix(&file.metadata.sha256_hash()))
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
pub fn read(path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
    EmbeddedAssets::get(path).map(|file| file.data)
}

/// `GET /assets/{*path}`
pub async fn serve(Path(path): Path<String>) -> Response {
    let Some(file) = EmbeddedAssets::get(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let etag = format!("\"{}\"", hex_prefix(&file.metadata.sha256_hash()));
    let cache_control = if cfg!(debug_assertions) { "no-cache" } else { "public, max-age=31536000, immutable" };

    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_str(mime.as_ref()).unwrap_or(HeaderValue::from_static("application/octet-stream"))),
            (header::CACHE_CONTROL, HeaderValue::from_static(cache_control)),
            (header::ETAG, HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"\""))),
        ],
        file.data,
    )
        .into_response()
}
