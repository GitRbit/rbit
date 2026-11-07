use std::collections::HashSet;

use axum::http::{HeaderValue, header};

use super::*;

fn is_hex(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn luminance(hex: &str) -> f64 {
    let channel = |i: usize| {
        let c = f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap()) / 255.0;
        if c <= 0.040_45 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
}

fn contrast(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[test]
fn catalog_is_sorted_unique_and_well_formed() {
    let themes = tinted();
    assert!(themes.len() >= 300, "the catalog imports hundreds of schemes, found {}", themes.len());
    assert!(themes.windows(2).all(|pair| pair[0].slug < pair[1].slug), "sorted by slug, without duplicates");
    let mut names = HashSet::new();
    for theme in themes {
        let prefix = format!("{}-", theme.system.as_str());
        assert!(theme.slug.starts_with(&prefix), "{} carries its system prefix", theme.slug);
        assert!(theme.slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'), "{}", theme.slug);
        assert!(rbit_services::preferences::is_theme_slug(theme.slug), "{} can be stored", theme.slug);
        assert!(!theme.name.is_empty() && !theme.family.is_empty() && !theme.author.is_empty(), "{}", theme.slug);
        assert!(names.insert(theme.name.to_lowercase()), "{} is listed once", theme.name);
        assert!(is_hex(theme.canvas) && is_hex(theme.ink), "{}", theme.slug);
        assert!(theme.swatches.iter().all(|c| is_hex(c)), "{}", theme.slug);
        assert_eq!(find(theme.slug), Some(theme));
    }
}

#[test]
fn counterparts_are_the_same_family_in_the_other_variant() {
    let mut paired = 0;
    for theme in tinted() {
        let Some(slug) = theme.counterpart else { continue };
        paired += 1;
        let other = find(slug).unwrap_or_else(|| panic!("{} pairs with unknown {slug}", theme.slug));
        assert_eq!(other.family, theme.family, "{} ↔ {slug}", theme.slug);
        assert_ne!(other.variant, theme.variant, "{} ↔ {slug}", theme.slug);
    }
    assert!(paired >= 100, "most families pair their light and dark schemes, found {paired}");
}

#[test]
fn the_toggle_keeps_the_scheme_and_flips_the_variant() {
    let toggled = |slug: &str| Theme::parse(slug).unwrap().toggled().slug();
    assert_eq!(toggled("system"), "dna-dark");
    assert_eq!(toggled("light"), "dark");
    assert_eq!(toggled("dark"), "light");
    assert_eq!(toggled("dna-dark"), "dna-day");
    assert_eq!(toggled("dna-day"), "dna-dark");
    for (from, to) in [
        ("base16-gruvbox-dark-hard", "base16-gruvbox-light-hard"),
        ("base16-gruvbox-light-soft", "base16-gruvbox-dark-soft"),
        ("base16-solarized-light", "base16-solarized-dark"),
        ("base24-catppuccin-latte", "base24-catppuccin-mocha"),
        ("base24-catppuccin-frappe", "base24-catppuccin-latte"),
        ("base16-rose-pine-dawn", "base16-rose-pine"),
        ("base24-tokyo-night-dark", "base24-tokyo-night-light"),
    ] {
        assert_eq!(toggled(from), to);
    }
    // Schemes without a sibling fall back to the built-in theme of the other variant.
    let lonely_dark = tinted().iter().find(|t| t.counterpart.is_none() && t.variant == Variant::Dark).unwrap();
    assert_eq!(Theme::Tinted(lonely_dark).toggled(), Theme::DnaDay);
    let lonely_light = tinted().iter().find(|t| t.counterpart.is_none() && t.variant == Variant::Light).unwrap();
    assert_eq!(Theme::Tinted(lonely_light).toggled(), Theme::DnaDark);
}

#[test]
fn colorblind_themes_restore_and_toggle_within_their_pair() {
    for (slug, theme, scheme, canvas) in [
        ("colorblind-light", Theme::ColorblindLight, "light", DNA_DAY_CANVAS),
        ("colorblind-dark", Theme::ColorblindDark, "dark", DNA_DARK_CANVAS),
    ] {
        assert_eq!(Theme::parse(slug), Some(theme));
        assert_eq!(theme.slug(), slug);
        assert_eq!(theme.scheme(), Some(scheme));
        assert_eq!(theme.theme_color(), Some(canvas));
        assert_eq!(theme.stylesheet_path(), None);
        assert!(Theme::BUILT_IN.contains(&theme));
        assert_ne!(theme.toggled(), theme);
        assert_eq!(theme.toggled().toggled(), theme);
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(&format!("rbit-theme={slug}")).unwrap());
        assert_eq!(Theme::from_headers(&headers), theme);
        assert!(rbit_services::preferences::is_theme_slug(slug));
    }
}

#[test]
fn catalog_text_is_readable_on_its_page() {
    for theme in tinted() {
        let ratio = contrast(theme.ink, theme.canvas);
        assert!(ratio >= 4.5, "{}: ink on canvas is {ratio:.2}:1", theme.slug);
        let dark = luminance(theme.canvas) < luminance(theme.ink);
        assert_eq!(dark, theme.variant == Variant::Dark, "{}: variant matches its colours", theme.slug);
    }
}

#[test]
fn both_variants_and_known_families_are_present() {
    let themes = tinted();
    assert!(themes.iter().any(|t| t.variant == Variant::Light));
    assert!(themes.iter().any(|t| t.variant == Variant::Dark));
    for family in ["Catppuccin", "Gruvbox", "Tokyo Night", "Rosé Pine", "Solarized"] {
        assert!(themes.iter().filter(|t| t.family == family).count() >= 2, "{family} groups its variants");
    }
}

#[test]
fn every_tinted_theme_has_its_stylesheet() {
    for theme in tinted() {
        let css = assets::read(&format!("themes/{}.css", theme.slug)).unwrap_or_else(|| panic!("{} has no stylesheet", theme.slug));
        let css = String::from_utf8_lossy(&css);
        assert!(css.contains(&format!("[data-theme=\"{}\"]{{color-scheme:{};", theme.slug, theme.variant.as_str())), "{}", theme.slug);
        assert!(css.contains(&format!("--canvas:{};", theme.canvas)), "{}", theme.slug);
    }
    let all = assets::read("themes/all.css").expect("the gallery stylesheet exists");
    assert_eq!(String::from_utf8_lossy(&all).matches("[data-theme=").count(), tinted().len());
}

#[test]
fn built_in_canvas_colours_match_the_stylesheet() {
    let css = include_str!("../../styles/app.css");
    assert!(css.contains(&format!("--canvas: light-dark({LIGHT_CANVAS}, {DARK_CANVAS});")), "update LIGHT_CANVAS / DARK_CANVAS");
    let system = css.split(":root:not([data-theme]),").nth(1).expect("system has its default palette");
    let system = system.split('{').nth(1).unwrap().split('}').next().unwrap();
    assert!(system.contains(&format!("--canvas: light-dark({DNA_DAY_CANVAS}, {DNA_DARK_CANVAS});")));
    assert!(system.contains("--inverted: light-dark(#00785f, #77d2ba);"), "system uses the DNA accent in both schemes");
    for (slug, canvas) in [("dna-dark", DNA_DARK_CANVAS), ("dna-day", DNA_DAY_CANVAS)] {
        let rule = css.split(&format!("[data-theme=\"{slug}\"] {{")).nth(1).unwrap_or_else(|| panic!("{slug} has its rule"));
        assert!(rule.contains(&format!("--canvas: {canvas};")), "update the {slug} canvas");
    }
}

#[test]
fn parses_built_in_and_tinted_slugs() {
    assert_eq!(Theme::parse("system"), Some(Theme::System));
    assert_eq!(Theme::parse("light"), Some(Theme::Light));
    assert_eq!(Theme::parse("dark"), Some(Theme::Dark));
    assert_eq!(Theme::parse("dna-dark"), Some(Theme::DnaDark));
    assert_eq!(Theme::parse("dna-day"), Some(Theme::DnaDay));
    let first = &tinted()[0];
    assert_eq!(Theme::parse(first.slug), Some(Theme::Tinted(first)));
    for unknown in ["", "Dark", "base16-does-not-exist", "../dark", "dark "] {
        assert_eq!(Theme::parse(unknown), None, "{unknown:?}");
    }
}

#[test]
fn reads_the_cookie_and_falls_back_to_system() {
    let with = |cookie: &str| {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(cookie).unwrap());
        Theme::from_headers(&headers)
    };
    assert_eq!(Theme::from_headers(&HeaderMap::new()), Theme::System);
    assert_eq!(Theme::System.toggled(), Theme::DnaDark);
    assert_eq!(Theme::parse("light").unwrap().name(), "rbit Classic Light");
    assert_eq!(Theme::parse("dark").unwrap().name(), "rbit Classic Dark");
    assert_eq!(Theme::DnaDay.name(), "rbit Light");
    assert_eq!(Theme::DnaDark.name(), "rbit Dark");
    assert_eq!(with("rbit-theme=dark"), Theme::Dark);
    assert_eq!(with("other=1; rbit-theme=light; more=2"), Theme::Light);
    assert_eq!(with("rbit-theme=base24-nope"), Theme::System);
    assert_eq!(with("rustit-theme=dark"), Theme::Dark, "the cookie from before the rename");
    assert_eq!(with("rustit-theme=dark; rbit-theme=light"), Theme::Light);
    assert_eq!(with("rbit-theme=%3Cscript%3E"), Theme::System);
    let slug = tinted()[0].slug;
    assert_eq!(with(&format!("rbit-theme={slug}")).slug(), slug);
}

#[test]
fn cookie_is_long_lived_lax_and_script_readable() {
    let cookie = Theme::Dark.cookie(true).to_string();
    for part in ["rbit-theme=dark", "Path=/", "SameSite=Lax", "Secure", "Max-Age=31536000"] {
        assert!(cookie.contains(part), "{cookie} has {part}");
    }
    assert!(!cookie.contains("HttpOnly"));
    assert!(!Theme::Light.cookie(false).to_string().contains("Secure"));
}

#[test]
fn page_attributes_follow_the_theme() {
    assert_eq!(Theme::System.attribute(), None);
    assert_eq!(Theme::System.scheme(), None);
    assert_eq!(Theme::System.stylesheet(), None);
    assert_eq!(Theme::Dark.attribute(), Some("dark"));
    assert_eq!(Theme::Dark.theme_color(), Some(DARK_CANVAS));
    let tinted = Theme::Tinted(&tinted()[0]);
    assert_eq!(tinted.attribute(), Some(tinted.slug()));
    assert!(tinted.stylesheet().is_some_and(|url| url.starts_with(&format!("/assets/themes/{}.css?v=", tinted.slug()))));
}
