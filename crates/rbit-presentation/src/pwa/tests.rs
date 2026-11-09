use super::*;
use crate::themes::{DNA_DARK_CANVAS, LIGHT_CANVAS};

fn app(app_icon_url: &str) -> InstallableApp<'_> {
    InstallableApp { name: "Forge", description: "Code", app_icon_url, color: LIGHT_CANVAS }
}

#[test]
fn launch_color_asks_for_the_chosen_theme_only_without_a_known_slug() {
    assert_eq!(launch_color("light", || unreachable!("the slug decides")), LIGHT_CANVAS);
    assert_eq!(launch_color("unknown", || Theme::DnaDark), DNA_DARK_CANVAS);
    assert_eq!(launch_color("", || Theme::System), DNA_DAY_CANVAS);
}

#[test]
fn manifest_uses_bundled_icons_unless_the_site_has_its_own() {
    let bundled: serde_json::Value = serde_json::from_str(&manifest(&app(""), |name| format!("/assets/{name}?v=1"))).unwrap();
    assert_eq!(bundled["name"], "Forge");
    assert_eq!(bundled["short_name"], "Forge");
    assert_eq!(bundled["start_url"], "/");
    assert_eq!(bundled["theme_color"], LIGHT_CANVAS);
    assert_eq!(bundled["icons"][2]["src"], "/assets/icon-maskable-512.png?v=1");
    assert_eq!(bundled["icons"][2]["purpose"], "maskable");
    let own: serde_json::Value = serde_json::from_str(&manifest(&app("https://cdn.example/icon.png"), |_| unreachable!())).unwrap();
    assert_eq!(own["icons"], json!([{"src": "https://cdn.example/icon.png", "sizes": "any", "purpose": "any"}]));
}

#[test]
fn worker_versions_follow_the_build_and_the_identity() {
    let version = worker_version("abc", "Forge");
    assert!(version.starts_with("abc-") && version.len() == "abc-".len() + IDENTITY_HEX_LEN, "{version}");
    assert_ne!(version, worker_version("abc", "Forge2"));
    assert_ne!(version, worker_version("abd", "Forge"));
    assert_eq!(worker_source("const VERSION = \"__VERSION__\";", &version), format!("const VERSION = \"{version}\";"));
}

#[test]
fn branding_sheets_are_immutable_only_for_the_current_colours() {
    let current = BrandingColors { primary: "#123456", text: "#ffffff" };
    let sheet = branding_sheet(Some(current), current);
    assert!(sheet.css.starts_with(".btn-primary{background-color:#123456;border-color:#123456;color:#ffffff}"), "{}", sheet.css);
    assert_eq!(sheet.cache_control, "public, max-age=31536000, immutable");
    let stale = branding_sheet(Some(current), BrandingColors { primary: "#000000", text: "#ffffff" });
    assert_eq!((stale.css, stale.cache_control), (sheet.css, "no-cache"));
    assert_eq!(branding_sheet(None, current), BrandingSheet { css: String::new(), cache_control: "no-cache" });
}
