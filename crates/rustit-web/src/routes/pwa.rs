//! Progressive-web-app plumbing: manifest, service worker, offline page.

use axum::{
    extract::State,
    http::header,
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::{
    AppState, assets,
    views::{layout::Page, pwa::OfflinePage},
};

pub async fn manifest(State(state): State<AppState>) -> Response {
    let site = state.services.instance.current().site_name;
    let manifest = json!({
        "name": site,
        "short_name": site,
        "description": format!("{site} — self-hosted Git"),
        "id": "/",
        "start_url": "/",
        "scope": "/",
        "display": "standalone",
        "background_color": "#09090c",
        "theme_color": "#7c3aed",
        "icons": [
            { "src": assets::url("icon.svg"), "sizes": "any", "type": "image/svg+xml", "purpose": "any" },
            { "src": assets::url("icon.svg"), "sizes": "any", "type": "image/svg+xml", "purpose": "maskable" }
        ],
        "shortcuts": [
            { "name": "New repository", "url": "/new" },
            { "name": "Explore", "url": "/explore" }
        ]
    });
    ([(header::CONTENT_TYPE, "application/manifest+json"), (header::CACHE_CONTROL, "no-cache")], manifest.to_string()).into_response()
}

/// Served from the root so its scope covers the whole site. The asset build hash is
/// injected so every deploy installs a fresh worker with a fresh cache.
pub async fn service_worker() -> Response {
    let source = assets::read("sw.js")
        .map(|bytes| String::from_utf8_lossy(&bytes).replace("__VERSION__", assets::build_version()))
        .unwrap_or_default();
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
            (header::HeaderName::from_static("service-worker-allowed"), "/"),
        ],
        source,
    )
        .into_response()
}

pub async fn offline(page: Page) -> OfflinePage {
    OfflinePage { page: page.titled("Offline") }
}
