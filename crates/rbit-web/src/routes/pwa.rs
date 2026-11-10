//! Progressive-web-app routes: manifest, service worker, offline page and branding sheet.
//!
//! What they contain is decided by [`rbit_presentation::pwa`]; these handlers read the
//! instance settings and request, and set content types and cache headers.

use axum::{
    extract::{Query, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use rbit_presentation::pwa::{self, BrandingColors, InstallableApp};

use crate::{
    AppState, assets,
    themes::{Theme, ThemeHttp},
    views::{layout::Page, pwa::OfflinePage},
};

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct ManifestQuery {
    theme: String,
}

fn launch_color(slug: &str, headers: &HeaderMap) -> &'static str {
    pwa::launch_color(slug, || Theme::from_headers(headers))
}

pub async fn manifest(State(state): State<AppState>, Query(query): Query<ManifestQuery>, headers: HeaderMap) -> Response {
    let color = launch_color(&query.theme, &headers);
    let settings = state.services.instance.current();
    let app = InstallableApp {
        name: &settings.site_name,
        description: &settings.branding.description,
        app_icon_url: &settings.branding.app_icon_url,
        color,
        explore: settings.explore_enabled,
    };
    let manifest = pwa::manifest(&app, assets::url);
    (
        [(header::CONTENT_TYPE, "application/manifest+json"), (header::CACHE_CONTROL, "private, no-cache"), (header::VARY, "Cookie")],
        manifest,
    )
        .into_response()
}

/// Served from the root so its scope covers the whole site. The asset build hash is
/// injected so every deploy installs a fresh worker with a fresh cache.
pub async fn service_worker(State(state): State<AppState>) -> Response {
    let settings = state.services.instance.current();
    let identity = format!("{}{:?}", settings.site_name, settings.branding);
    // Refresh the anonymous offline fallback after identity changes, even without a deploy.
    worker_response(&pwa::worker_version(assets::build_version(), &identity))
}

fn worker_response(version: &str) -> Response {
    let source = assets::read("sw.js").map(|bytes| pwa::worker_source(&String::from_utf8_lossy(&bytes), version)).unwrap_or_default();
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

pub async fn offline(State(state): State<AppState>) -> OfflinePage {
    // The service worker persists this page across sign-outs and account switches.
    // It must never contain the requesting account's navigation or preferences.
    let page = Page::new(&state, None, "/offline");
    let title = page.text(rbit_localization::shell::pages::PageMessage::Offline).to_string();
    OfflinePage { page: page.titled(title) }
}

/// The colours `base.html` links the branding sheet with (`?color=…&text=…`).
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct BrandingQuery {
    color: String,
    text: String,
}

/// `GET /assets/branding.css`: a separate same-origin sheet keeps arbitrary inline styles
/// blocked by the CSP.
///
/// The sheet is a function of the two colours, and pages link it with both in the query,
/// so a URL naming the current colours is content-addressed like a hashed asset and is
/// cached for good; changing the branding changes the URL. Any other URL (stale colours,
/// none) is revalidated. It depends on no session, so it is served with the static assets.
pub async fn branding_styles(State(state): State<AppState>, Query(query): Query<BrandingQuery>) -> Response {
    let branding = state.services.instance.current().branding;
    let valid = !branding.primary_color.is_empty() && branding.validate().is_ok();
    let current = valid.then_some(BrandingColors { primary: &branding.primary_color, text: &branding.primary_text_color });
    let sheet = pwa::branding_sheet(current, BrandingColors { primary: &query.color, text: &query.text });
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, sheet.cache_control)], sheet.css).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_colors_follow_rbit_defaults_and_saved_choices() {
        let mut headers = HeaderMap::new();
        assert_eq!(launch_color("", &headers), crate::themes::DNA_DAY_CANVAS);
        assert_eq!(launch_color("dna-dark", &headers), crate::themes::DNA_DARK_CANVAS);
        headers.insert(header::COOKIE, "rbit-theme=dna-dark".parse().unwrap());
        assert_eq!(launch_color("unknown", &headers), crate::themes::DNA_DARK_CANVAS);
        assert_eq!(launch_color("light", &headers), crate::themes::LIGHT_CANVAS);
        assert_eq!(launch_color("base24-berlin", &headers), crate::themes::Theme::parse("base24-berlin").unwrap().theme_color().unwrap());
    }

    /// PNG width and height from the IHDR chunk.
    fn png_size(bytes: &[u8]) -> (u32, u32) {
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let read = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
        (read(16), read(20))
    }

    #[tokio::test]
    async fn service_worker_is_versioned_and_scoped_to_the_whole_site() {
        let response = worker_response(assets::build_version());
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
        for (file, size) in [
            ("icon-32.png", 32),
            ("icon-192.png", 192),
            ("icon-512.png", 512),
            ("icon-maskable-512.png", 512),
            ("apple-touch-icon.png", 180),
        ] {
            let bytes = assets::read(file).unwrap_or_else(|| panic!("{file} is missing; run `npm run icons`"));
            assert_eq!(png_size(&bytes), (size, size), "{file}");
        }
    }
}
