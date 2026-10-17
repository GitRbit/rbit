//! The theme gallery: `/themes` for everyone, and the Appearance settings page that shows
//! the same gallery inside the settings layout.
//!
//! Frontend workstream T owns this file. Every card is a live preview: it carries
//! `data-theme`, so the tokens inside it are that theme's, and `assets/themes/all.css` (linked
//! only by the gallery) defines all of them. Filtering and search work server-side for
//! visitors without JavaScript; `theme-picker.js` repeats them instantly in the browser.

use std::sync::OnceLock;

use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

use crate::{
    assets,
    themes::{self, Theme, TintedTheme, Variant},
    views::{
        layout::Page,
        settings::{SettingsLink, SettingsSection},
    },
};

/// Where the public gallery lives.
pub const GALLERY_URL: &str = "/themes";
/// Heading of the group collecting schemes that have no family of their own.
const STANDALONE_FAMILY: &str = "More themes";
/// Longest accepted search text.
const MAX_QUERY_LEN: usize = 100;

/// The variant filter above the gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VariantFilter {
    #[default]
    All,
    Dark,
    Light,
}

impl VariantFilter {
    const ALL: [Self; 3] = [Self::All, Self::Dark, Self::Light];

    /// `?variant=` value; anything unknown shows every theme.
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("dark") => Self::Dark,
            Some("light") => Self::Light,
            _ => Self::All,
        }
    }

    const fn key(self) -> &'static str {
        match self {
            Self::All => "",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    fn admits(self, variant: Option<Variant>) -> bool {
        match (self, variant) {
            (Self::All, _) | (_, None) => true,
            (Self::Dark, Some(variant)) => variant == Variant::Dark,
            (Self::Light, Some(variant)) => variant == Variant::Light,
        }
    }
}

/// A filter link (`All`, `Dark`, `Light`).
#[derive(Debug, Clone)]
pub struct FilterLink {
    pub key: &'static str,
    pub label: &'static str,
    pub url: String,
    pub is_active: bool,
}

/// One preview card.
#[derive(Debug, Clone)]
pub struct ThemeCard {
    pub slug: &'static str,
    pub name: &'static str,
    /// Shown as the card's tooltip, for attribution.
    pub credit: String,
    /// `light`, `dark`, or empty for System.
    pub variant: &'static str,
    /// What the light/dark toggle switches to once this theme is chosen.
    pub counterpart: &'static str,
    pub is_current: bool,
    /// Filtered out by the current search (rendered `hidden`, so scripts can show it again).
    pub hidden: bool,
}

/// A family heading with its themes.
#[derive(Debug, Clone)]
pub struct FamilyGroup {
    pub name: &'static str,
    pub cards: Vec<ThemeCard>,
    pub hidden: bool,
}

/// Everything the gallery partial (`themes/gallery.html`) renders.
#[derive(Debug, Clone)]
pub struct Gallery {
    pub current: Theme,
    pub variant: &'static str,
    pub query: String,
    pub filters: Vec<FilterLink>,
    /// Rbit's own themes, pinned first.
    pub defaults: Vec<ThemeCard>,
    /// Whether the search hides all of Rbit's own themes.
    pub defaults_hidden: bool,
    pub families: Vec<FamilyGroup>,
    pub shown: usize,
    pub total: usize,
    /// Whether the search hides every card, built-in ones included.
    pub nothing_found: bool,
    /// The tinted schemes' licence and author list.
    pub licence_url: String,
    /// The page the gallery is on, which filter links and the search form point at.
    pub base_url: &'static str,
    /// Where a theme form returns to without JavaScript (this page, same filters).
    pub next: String,
    /// The combined stylesheet defining every tinted theme.
    pub stylesheet: String,
    /// Lets scripts build cache-busting URLs for single-theme stylesheets.
    pub asset_version: &'static str,
    pub signed_in: bool,
}

impl Gallery {
    pub fn new(page: &Page, base_url: &'static str, variant: VariantFilter, query: Option<&str>) -> Self {
        let query = normalise_query(query.unwrap_or_default());
        let words: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_owned).collect();
        let current = page.theme;
        let mut shown = 0;
        let mut any_visible = false;
        let mut card = |theme: Theme, family: &str, credit: String| {
            let haystack = format!("{} {family}", theme.name()).to_lowercase();
            let hidden = !variant.admits(theme.variant()) || !words.iter().all(|word| haystack.contains(word.as_str()));
            any_visible |= !hidden;
            if !hidden && theme.variant().is_some() {
                shown += 1;
            }
            ThemeCard {
                slug: theme.slug(),
                name: theme.name(),
                credit,
                variant: theme.variant().map_or("", Variant::as_str),
                counterpart: theme.toggled().slug(),
                is_current: theme == current,
                hidden,
            }
        };

        let defaults: Vec<ThemeCard> =
            Theme::BUILT_IN.iter().map(|&theme| card(theme, "rbit", format!("{} · built in", theme.name()))).collect();
        let families = families()
            .iter()
            .map(|(name, members)| {
                let cards: Vec<ThemeCard> =
                    members.iter().map(|&theme| card(Theme::Tinted(theme), name, format!("{} by {}", theme.name, theme.author))).collect();
                FamilyGroup { name, hidden: cards.iter().all(|c| c.hidden), cards }
            })
            .collect();

        let query_suffix = |key: &str| {
            let mut params = Vec::new();
            if !key.is_empty() {
                params.push(format!("variant={key}"));
            }
            if !query.is_empty() {
                params.push(format!("q={}", utf8_percent_encode(&query, NON_ALPHANUMERIC)));
            }
            if params.is_empty() { String::new() } else { format!("?{}", params.join("&")) }
        };
        let filters = VariantFilter::ALL
            .iter()
            .map(|&filter| FilterLink {
                key: filter.key(),
                label: filter.label(),
                url: format!("{base_url}{}", query_suffix(filter.key())),
                is_active: filter == variant,
            })
            .collect();

        Self {
            current,
            variant: variant.key(),
            next: format!("{base_url}{}", query_suffix(variant.key())),
            query,
            filters,
            defaults_hidden: defaults.iter().all(|card| card.hidden),
            defaults,
            families,
            shown,
            total: themes::tinted().len() + 2,
            nothing_found: !any_visible,
            licence_url: assets::url("themes/LICENSE.txt"),
            base_url,
            stylesheet: assets::url("themes/all.css"),
            asset_version: assets::build_version(),
            signed_in: page.is_signed_in(),
        }
    }

    pub fn shown_label(&self) -> String {
        format!("{} {}", self.shown, if self.shown == 1 { "theme" } else { "themes" })
    }
}

/// Trimmed, single-spaced and bounded.
fn normalise_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_QUERY_LEN).collect()
}

/// Families with more than one member, alphabetically, then every standalone scheme under
/// one heading. Members are sorted by name. Built once.
pub fn families() -> &'static [(&'static str, Vec<&'static TintedTheme>)] {
    static FAMILIES: OnceLock<Vec<(&'static str, Vec<&'static TintedTheme>)>> = OnceLock::new();
    FAMILIES.get_or_init(|| {
        let mut grouped: Vec<(&'static str, Vec<&'static TintedTheme>)> = Vec::new();
        let mut sorted: Vec<&'static TintedTheme> = themes::tinted().iter().collect();
        sorted.sort_by_cached_key(|theme| (theme.family.to_lowercase(), theme.name.to_lowercase()));
        for theme in sorted {
            match grouped.last_mut() {
                Some((family, members)) if *family == theme.family => members.push(theme),
                _ => grouped.push((theme.family, vec![theme])),
            }
        }
        let (mut families, singles): (Vec<_>, Vec<_>) = grouped.into_iter().partition(|(_, members)| members.len() > 1);
        let mut standalone: Vec<&'static TintedTheme> = singles.into_iter().flat_map(|(_, members)| members).collect();
        standalone.sort_by_cached_key(|theme| theme.name.to_lowercase());
        families.push((STANDALONE_FAMILY, standalone));
        families
    })
}

/// `GET /themes`
#[derive(Debug, Template, WebTemplate)]
#[template(path = "themes/index.html")]
pub struct ThemesPage {
    pub page: Page,
    pub gallery: Gallery,
}

impl ThemesPage {
    pub fn new(page: Page, variant: VariantFilter, query: Option<&str>) -> Self {
        let gallery = Gallery::new(&page, GALLERY_URL, variant, query);
        Self { page: page.titled("Themes"), gallery }
    }
}

/// `GET /settings/appearance`
#[derive(Debug, Template, WebTemplate)]
#[template(path = "settings/appearance.html")]
pub struct AppearancePage {
    pub page: Page,
    pub settings_nav: Vec<SettingsLink>,
    pub gallery: Gallery,
}

impl AppearancePage {
    pub fn new(page: Page, variant: VariantFilter, query: Option<&str>) -> Self {
        let gallery = Gallery::new(&page, SettingsSection::Appearance.url(), variant, query);
        Self { page: page.titled("Appearance"), settings_nav: SettingsSection::Appearance.nav(), gallery }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(theme: Theme) -> Page {
        Page {
            site_name: "Rbit".to_owned(),
            title: None,
            viewer: None,
            path: GALLERY_URL.to_owned(),
            theme,
            has_unread_notifications: false,
        }
    }

    fn all_cards(gallery: &Gallery) -> impl Iterator<Item = &ThemeCard> {
        gallery.defaults.iter().chain(gallery.families.iter().flat_map(|family| &family.cards))
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
        let gallery = Gallery::new(&page(Theme::Dark), GALLERY_URL, VariantFilter::All, None);
        let slugs: Vec<_> = gallery.defaults.iter().map(|card| card.slug).collect();
        assert_eq!(slugs, ["system", "light", "dark"]);
        let current: Vec<_> = all_cards(&gallery).filter(|card| card.is_current).map(|card| card.slug).collect();
        assert_eq!(current, ["dark"]);
        assert_eq!(gallery.shown, themes::tinted().len() + 2);
        assert!(all_cards(&gallery).all(|card| !card.hidden));
    }

    #[test]
    fn variant_filter_and_search_hide_cards() {
        let dark = Gallery::new(&page(Theme::System), GALLERY_URL, VariantFilter::Dark, None);
        assert!(all_cards(&dark).filter(|card| !card.hidden).all(|card| card.variant != "light"));
        assert!(dark.filters.iter().any(|link| link.is_active && link.key == "dark"));

        let search = Gallery::new(&page(Theme::System), GALLERY_URL, VariantFilter::All, Some("  gruvbox   DARK "));
        assert_eq!(search.query, "gruvbox DARK");
        let visible: Vec<_> = all_cards(&search).filter(|card| !card.hidden).collect();
        assert!(!visible.is_empty());
        assert!(visible.iter().all(|card| card.name.to_lowercase().contains("gruvbox") || card.slug == "system"));
        assert!(search.families.iter().any(|family| family.hidden), "families without matches are hidden");
        assert_eq!(search.next, "/themes?q=gruvbox%20DARK");
        assert_eq!(search.filters[1].url, "/themes?variant=dark&q=gruvbox%20DARK");
    }

    #[test]
    fn renders_live_preview_cards() {
        let tinted = &themes::tinted()[0];
        let html = ThemesPage::new(page(Theme::Tinted(tinted)), VariantFilter::All, None).render().unwrap();
        assert!(html.contains(&format!("<html lang=\"en\" data-theme=\"{}\" data-scheme=\"{}\">", tinted.slug, tinted.variant.as_str())));
        assert!(html.contains(&format!("data-theme=\"{}\" value=\"{}\"", tinted.slug, tinted.slug)), "cards carry their own theme");
        assert!(html.contains("/assets/themes/all.css?v="), "the gallery links every theme");
        assert!(html.contains(&format!("/assets/themes/{}.css?v=", tinted.slug)), "the active theme's own sheet");
        assert!(html.contains("<form method=\"post\" action=\"/themes\""), "works without scripts");
        assert!(!html.contains("style="), "no inline styles under the CSP");
    }
}
