//! Theme gallery behavior and document attributes through real Askama rendering.

use super::*;

fn page(theme: Theme) -> Page {
    crate::views::test_support::page(SettingsSection::Appearance.url()).with_theme(theme)
}

fn all_cards(gallery: &Gallery) -> impl Iterator<Item = &ThemeCard> {
    gallery.defaults.iter().chain(&gallery.classic).chain(gallery.families.iter().flat_map(|family| &family.cards))
}

/// These root attributes have single-token quoted values. Inspect only the root
/// opening tag so card previews cannot accidentally satisfy a document assertion.
fn root_attribute<'a>(html: &'a str, name: &str) -> Option<&'a str> {
    let root = html.split_once("<html ").expect("document root").1.split_once('>').expect("root opening tag").0;
    root.split_ascii_whitespace()
        .filter_map(|attribute| attribute.split_once('='))
        .find(|(attribute, _)| *attribute == name)
        .map(|(_, value)| value.strip_prefix('"').and_then(|value| value.strip_suffix('"')).expect("quoted root attribute"))
}

#[test]
fn families_cover_every_theme_once() {
    let listed: usize = families().iter().map(|(_, members)| members.len()).sum();
    assert_eq!(listed, themes::tinted().len());
    let (last, _) = families().last().unwrap();
    assert_eq!(*last, STANDALONE_FAMILY);
    for (name, members) in &families()[..families().len() - 1] {
        assert!(members.len() > 1, "{name} is a real family");
        assert!(members.iter().all(|theme| theme.family == *name));
    }
}

#[test]
fn defaults_come_first_and_the_current_theme_is_marked() {
    let gallery = Gallery::new(&page(Theme::Dark), SettingsSection::Appearance.url(), VariantFilter::All, None);
    let slugs: Vec<_> = gallery.defaults.iter().map(|card| card.slug).collect();
    assert_eq!(slugs, ["system", "dna-day", "dna-dark", "colorblind-light", "colorblind-dark"]);
    assert_eq!(
        gallery.defaults.iter().map(|card| card.name).collect::<Vec<_>>(),
        ["System", "rbit Light", "rbit Dark", "rbit Light Colorblind", "rbit Dark Colorblind"]
    );
    let classic: Vec<_> = gallery.classic.iter().map(|card| (card.slug, card.counterpart)).collect();
    assert_eq!(classic, [("light", "dark"), ("dark", "light")]);
    let current: Vec<_> = all_cards(&gallery).filter(|card| card.is_current).map(|card| card.slug).collect();
    assert_eq!(current, ["dark"]);
    assert_eq!(gallery.shown, themes::tinted().len() + 6);
    assert!(all_cards(&gallery).all(|card| !card.hidden));
}

#[test]
fn system_page_uses_dna_browser_colors_and_default_toggle() {
    let html = AppearancePage::new(page(Theme::System), VariantFilter::All, None).render().unwrap();
    assert_eq!(root_attribute(&html, "lang"), Some("en"));
    assert_eq!(root_attribute(&html, "dir"), Some("ltr"));
    assert_eq!(root_attribute(&html, "data-theme"), None, "unselected preferences follow the device");
    assert_eq!(root_attribute(&html, "data-scheme"), None, "the device selects its own scheme");
    assert!(html.contains(&format!("content=\"{}\" media=\"(prefers-color-scheme: light)\"", themes::DNA_DAY_CANVAS)));
    assert!(html.contains(&format!("content=\"{}\" media=\"(prefers-color-scheme: dark)\"", themes::DNA_DARK_CANVAS)));
    assert!(html.contains("value=\"dna-dark\" data-theme-toggle"));
}

#[test]
fn variant_filter_and_search_hide_cards() {
    let dark = Gallery::new(&page(Theme::System), SettingsSection::Appearance.url(), VariantFilter::Dark, None);
    assert!(all_cards(&dark).filter(|card| !card.hidden).all(|card| card.variant != "light"));
    assert!(dark.filters.iter().any(|link| link.is_active && link.key == "dark"));

    let search = Gallery::new(&page(Theme::System), SettingsSection::Appearance.url(), VariantFilter::All, Some("  gruvbox   DARK "));
    assert_eq!(search.query, "gruvbox DARK");
    let visible: Vec<_> = all_cards(&search).filter(|card| !card.hidden).collect();
    assert!(!visible.is_empty());
    assert!(visible.iter().all(|card| card.name.to_lowercase().contains("gruvbox") || card.slug == "system"));
    assert!(search.families.iter().any(|family| family.hidden), "families without matches are hidden");
    assert_eq!(search.next, "/settings/appearance?q=gruvbox%20DARK");
    assert_eq!(search.filters[1].url, "/settings/appearance?variant=dark&q=gruvbox%20DARK");
}

#[test]
fn picker_lists_every_theme_grouped_with_the_current_one_marked() {
    let tinted = &themes::tinted()[0];
    let picker = ThemePicker::new(&page(Theme::Tinted(tinted)));
    assert_eq!(picker.groups[0].name, "Built in");
    assert_eq!(picker.groups[1].name, "rbit Classic");
    assert_eq!(picker.groups[1].rows.iter().map(|row| row.card.name).collect::<Vec<_>>(), ["rbit Classic Light", "rbit Classic Dark"]);
    let rows: Vec<_> = picker.groups.iter().flat_map(|group| &group.rows).collect();
    assert_eq!(rows.len(), picker.total);
    let current: Vec<_> = rows.iter().filter(|row| row.card.is_current).map(|row| row.card.slug).collect();
    assert_eq!(current, [tinted.slug]);
    let standalone = picker.groups.last().unwrap();
    assert_eq!(standalone.name, STANDALONE_FAMILY);
    assert!(standalone.rows[0].letter.is_some(), "the first standalone theme opens a letter");
    assert!(standalone.rows.iter().filter_map(|row| row.letter).count() > 10);
    let gruvbox = rows.iter().find(|row| row.card.slug == "base16-gruvbox-dark-hard").unwrap();
    assert!(gruvbox.search.contains("gruvbox") && gruvbox.search.contains("dark"));

    let html = picker.render().unwrap();
    assert!(html.contains("<dialog class=\"tpk\""));
    let filters: Vec<_> = html.split("data-tpk-variant=\"").skip(1).map(|part| part.split('"').next().unwrap()).collect();
    assert_eq!(filters, ["", "dark", "light", "colorblind"]);
    assert!(html.contains("/assets/themes/all.css?v="), "chips render in their own theme");
    assert!(html.contains(&format!("value=\"{}\" role=\"option\" aria-selected=\"false\" aria-pressed=\"true\"", tinted.slug)));
    assert!(html.contains("<form class=\"tpk-list\" id=\"tpk-list\" method=\"post\" action=\"/themes\""), "works as a plain form");
    assert!(!html.contains("href=\"/themes\""), "the retired gallery has no links");
    assert!(!html.contains("style="), "no inline styles under the CSP");
}

#[test]
fn renders_live_preview_cards() {
    let tinted = &themes::tinted()[0];
    let html = AppearancePage::new(page(Theme::Tinted(tinted)), VariantFilter::All, None).render().unwrap();
    assert_eq!(root_attribute(&html, "lang"), Some("en"));
    assert_eq!(root_attribute(&html, "dir"), Some("ltr"));
    assert_eq!(root_attribute(&html, "data-theme"), Some(tinted.slug));
    assert_eq!(root_attribute(&html, "data-scheme"), Some(tinted.variant.as_str()));
    assert!(html.contains(&format!("data-theme=\"{}\" value=\"{}\"", tinted.slug, tinted.slug)), "cards carry their own theme");
    assert!(html.contains("/assets/themes/all.css?v="), "the gallery links every theme");
    assert!(html.contains(&format!("<style data-theme-stylesheet data-theme=\"{}\">", tinted.slug)), "the active theme's own sheet");
    assert!(html.contains("<form method=\"post\" action=\"/themes\""), "works without scripts");
    assert!(!html.contains("href=\"/themes\""), "the retired gallery has no links");
    assert!(!html.contains("style="), "no inline styles under the CSP");
}

#[test]
fn colorblind_themes_are_searchable_and_render_their_own_toggle_pair() {
    for theme in [Theme::ColorblindLight, Theme::ColorblindDark] {
        let gallery = Gallery::new(&page(theme), SettingsSection::Appearance.url(), VariantFilter::All, Some("colorblind"));
        let visible: Vec<_> = gallery.defaults.iter().filter(|card| !card.hidden).collect();
        assert_eq!(visible.len(), 2);
        let current = visible.iter().find(|card| card.is_current).unwrap();
        assert_eq!(current.slug, theme.slug());
        assert_eq!(current.counterpart, theme.toggled().slug());
        let html = AppearancePage::new(page(theme), VariantFilter::All, None).render().unwrap();
        assert_eq!(root_attribute(&html, "data-theme"), Some(theme.slug()));
        assert_eq!(root_attribute(&html, "data-scheme"), theme.scheme());
        assert!(html.contains(&format!("value=\"{}\" data-theme-toggle", theme.toggled().slug())));
    }
}
