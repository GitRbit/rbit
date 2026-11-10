//! The installable app: the web manifest, the service worker's version and the branding
//! stylesheet.
//!
//! `rbit-web` serves these with their content types and cache headers, supplies the
//! instance's settings and hashed asset URLs, and renders the offline page without any
//! account (the service worker keeps it across sign-outs).

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::themes::{DNA_DAY_CANVAS, Theme};

/// Hex digits of the site identity digest in a service worker version.
const IDENTITY_HEX_LEN: usize = 12;

/// The service worker's placeholder for its version.
const VERSION_PLACEHOLDER: &str = "__VERSION__";

/// What the manifest says about the site.
#[derive(Debug, Clone, Copy)]
pub struct InstallableApp<'a> {
    /// The site name, used as both name and short name.
    pub name: &'a str,
    pub description: &'a str,
    /// An operator-supplied icon URL; empty for the built-in icons.
    pub app_icon_url: &'a str,
    /// The launch background and theme colour ([`launch_color`]).
    pub color: &'a str,
    /// The instance offers the Explore page, so the app gets a shortcut to it.
    pub explore: bool,
}

/// The colour the installed app launches with: the theme named in the manifest URL, else
/// the visitor's chosen theme (asked only then), else the default DNA day canvas.
pub fn launch_color(slug: &str, chosen: impl FnOnce() -> Theme) -> &'static str {
    Theme::parse(slug).unwrap_or_else(chosen).theme_color().unwrap_or(DNA_DAY_CANVAS)
}

/// The web app manifest. `asset_url` turns a bundled icon's name into its hashed URL.
pub fn manifest(app: &InstallableApp<'_>, asset_url: impl Fn(&str) -> String) -> String {
    let icons = if app.app_icon_url.is_empty() {
        json!([
            {"src": asset_url("icon-192.png"), "sizes": "192x192", "type": "image/png", "purpose": "any"},
            {"src": asset_url("icon-512.png"), "sizes": "512x512", "type": "image/png", "purpose": "any"},
            {"src": asset_url("icon-maskable-512.png"), "sizes": "512x512", "type": "image/png", "purpose": "maskable"}
        ])
    } else {
        json!([{"src": app.app_icon_url, "sizes": "any", "purpose": "any"}])
    };
    let mut shortcuts = vec![json!({ "name": "New repository", "url": "/new" })];
    if app.explore {
        shortcuts.push(json!({ "name": "Explore", "url": "/explore" }));
    }
    let manifest = json!({
        "name": app.name,
        "short_name": app.name,
        "description": app.description,
        "id": "/",
        "start_url": "/",
        "scope": "/",
        "display": "standalone",
        // Match the selected theme before the first page paints. The id stays stable.
        "background_color": app.color,
        "theme_color": app.color,
        // PNGs generated from scripts/icons/logo.png by `npm run icons`; the maskable icon keeps
        // the entire mascot inside the safe zone.
        "icons": icons,
        "shortcuts": shortcuts
    });
    manifest.to_string()
}

/// The service worker's version: the asset build plus a digest of the site's identity, so
/// every deploy and every identity change installs a fresh worker with a fresh cache (and
/// refreshes the anonymous offline fallback).
pub fn worker_version(build: &str, identity: &str) -> String {
    use std::fmt::Write;
    let digest = Sha256::digest(identity.as_bytes());
    let hex = digest.iter().take(IDENTITY_HEX_LEN / 2).fold(String::with_capacity(IDENTITY_HEX_LEN), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    });
    format!("{build}-{hex}")
}

/// The worker's source with its version filled in.
pub fn worker_source(template: &str, version: &str) -> String {
    template.replace(VERSION_PLACEHOLDER, version)
}

/// The branding colours, as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrandingColors<'a> {
    pub primary: &'a str,
    pub text: &'a str,
}

/// The branding stylesheet and how long it may be cached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrandingSheet {
    pub css: String,
    pub cache_control: &'static str,
}

/// The branding sheet for the instance's valid colours (`None` when unset or invalid),
/// requested by a URL naming `requested`. A URL naming the current colours is
/// content-addressed and cached for good; any other URL is revalidated.
pub fn branding_sheet(current: Option<BrandingColors<'_>>, requested: BrandingColors<'_>) -> BrandingSheet {
    let css = current.map_or_else(String::new, |colors| {
        format!(
            ".btn-primary{{background-color:{};border-color:{};color:{}}}.btn-primary:hover{{background-color:{};border-color:{};filter:brightness(.9)}}",
            colors.primary, colors.primary, colors.text, colors.primary, colors.primary
        )
    });
    let cache_control = if current == Some(requested) { "public, max-age=31536000, immutable" } else { "no-cache" };
    BrandingSheet { css, cache_control }
}

#[cfg(test)]
mod tests;
