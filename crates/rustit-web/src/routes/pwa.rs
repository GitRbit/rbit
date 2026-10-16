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
        // The splash screen before first paint. No theme_color: the page's own theme-color
        // meta tags (light, dark or the chosen theme) apply from there on.
        "background_color": crate::themes::LIGHT_CANVAS,
        // PNGs (rendered from icon.svg by `npm run icons`) for platforms that ignore SVG
        // icons; the maskable one keeps the glyph inside the safe zone.
        "icons": [
            { "src": assets::url("icon.svg"), "sizes": "any", "type": "image/svg+xml", "purpose": "any" },
            { "src": assets::url("icon-192.png"), "sizes": "192x192", "type": "image/png", "purpose": "any" },
            { "src": assets::url("icon-512.png"), "sizes": "512x512", "type": "image/png", "purpose": "any" },
            { "src": assets::url("icon-maskable-512.png"), "sizes": "512x512", "type": "image/png", "purpose": "maskable" }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// PNG width and height from the IHDR chunk.
    fn png_size(bytes: &[u8]) -> (u32, u32) {
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let read = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
        (read(16), read(20))
    }

    #[tokio::test]
    async fn service_worker_is_versioned_and_scoped_to_the_whole_site() {
        let response = service_worker().await;
        let headers = response.headers();
        assert_eq!(headers[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
        assert_eq!(headers[header::CACHE_CONTROL], "no-cache", "browsers must revalidate the worker on every update check");
        assert_eq!(headers["service-worker-allowed"], "/");

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let source = String::from_utf8(body.to_vec()).unwrap();
        assert!(!source.contains("__VERSION__"), "placeholder left in the served worker");
        assert!(source.contains(&format!("\"{}\"", assets::build_version())));
    }

    #[test]
    fn generated_icons_are_embedded_at_their_declared_sizes() {
        for (file, size) in [("icon-192.png", 192), ("icon-512.png", 512), ("icon-maskable-512.png", 512), ("apple-touch-icon.png", 180)] {
            let bytes = assets::read(file).unwrap_or_else(|| panic!("{file} is missing; run `npm run icons`"));
            assert_eq!(png_size(&bytes), (size, size), "{file}");
        }
    }
}
