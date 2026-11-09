//! The theme picker dialog and the gallery inside Appearance settings.
//!
//! Frontend workstream T owns this file. Every card is a live preview: it carries
//! `data-theme`, so the tokens inside it are that theme's, and `assets/themes/all.css` (linked
//! only by the gallery) defines all of them. Filtering and search work server-side for
//! visitors without JavaScript; `theme-picker.js` repeats them instantly in the browser.

use std::sync::OnceLock;

mod labels;
use labels::PickerLabels;

use askama::Template;
use askama_web::WebTemplate;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_localization::{
    Locale, Localized,
    settings::profile::{AppearanceMessage, GalleryMessage, GalleryText},
};

use crate::{
    assets,
    themes::{self, Theme, TintedTheme, Variant},
    views::{
        layout::Page,
        settings::{SettingsLink, SettingsSection},
    },
};

/// Where theme controls save the chosen theme.
pub const CHOOSE_URL: &str = "/themes";
/// Heading of the group collecting schemes that have no family of their own.
const STANDALONE_FAMILY: &str = "More themes";
/// Heading of the preserved monochrome themes' group.
const CLASSIC_FAMILY: &str = "rbit Classic";
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
#[allow(clippy::struct_excessive_bools, reason = "independent template visibility flags, not a state machine")]
pub struct Gallery {
    pub current: Theme,
    pub variant: &'static str,
    pub query: String,
    pub filters: Vec<FilterLink>,
    /// rbit's own themes, pinned first.
    pub defaults: Vec<ThemeCard>,
    /// Whether the search hides all of rbit's own themes.
    pub defaults_hidden: bool,
    /// The original monochrome themes, listed after the defaults.
    pub classic: Vec<ThemeCard>,
    /// Whether the search hides both Classic themes.
    pub classic_hidden: bool,
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
    /// The shown count's sentence and its live forms ([`PickerLabels::count_forms`]).
    pub count_labels: PickerLabels,
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
            Theme::BUILT_IN.iter().map(|&theme| card(theme, "Built in", format!("{} · built in", theme.name()))).collect();
        let classic: Vec<ThemeCard> =
            Theme::CLASSIC.iter().map(|&theme| card(theme, CLASSIC_FAMILY, format!("{} · built in", theme.name()))).collect();
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
            classic_hidden: classic.iter().all(|card| card.hidden),
            classic,
            families,
            shown,
            total: themes::tinted().len() + Theme::BUILT_IN.len() - 1 + Theme::CLASSIC.len(),
            nothing_found: !any_visible,
            licence_url: assets::url("themes/LICENSE.txt"),
            base_url,
            stylesheet: assets::url("themes/all.css"),
            asset_version: assets::build_version(),
            signed_in: page.is_signed_in(),
            count_labels: PickerLabels::new(page.locale),
        }
    }

    /// The search field's placeholder, with the number of themes.
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "Askama passes the page locale by reference")]
    pub fn search_placeholder(&self, locale: &Locale) -> Localized {
        GalleryText::SearchCount(u64::try_from(self.total).unwrap_or(u64::MAX)).text(locale)
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

/// Where the header's theme picker dialog is fetched from.
pub const PICKER_URL: &str = "/themes/picker";

/// One row of the picker dialog.
#[derive(Debug, Clone)]
pub struct PickerRow {
    pub card: ThemeCard,
    /// What the dialog's search matches: name, family, author and variant, lowercased.
    pub search: String,
    /// Starts a new letter in the long list of standalone schemes.
    pub letter: Option<char>,
}

/// A heading of the picker dialog with its rows.
#[derive(Debug, Clone)]
pub struct PickerGroup {
    pub name: &'static str,
    pub rows: Vec<PickerRow>,
}

/// `GET /themes/picker`: the header's theme picker, a `<dialog>` fragment that
/// `theme-picker.js` fetches the first time it opens, so pages don't carry every theme.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "themes/picker.html")]
pub struct ThemePicker {
    pub page: Page,
    pub labels: PickerLabels,
    pub groups: Vec<PickerGroup>,
    pub total: usize,
    pub stylesheet: String,
    pub asset_version: &'static str,
}

impl ThemePicker {
    pub fn new(page: &Page) -> Self {
        let gallery = Gallery::new(page, SettingsSection::Appearance.url(), VariantFilter::All, None);
        let labels = PickerLabels::new(page.locale);
        let row = |card: ThemeCard, family: &'static str, letter: Option<char>| {
            let search = if page.locale == rbit_localization::Locale::English {
                format!("{} {family} {} {}", card.name, card.credit, card.variant)
            } else if card.slug == "system" {
                format!("{} {family} {} {} {} {}", card.name, card.credit, card.variant, labels.variant(card.variant), labels.system())
            } else {
                format!("{} {family} {} {} {}", card.name, card.credit, card.variant, labels.variant(card.variant))
            }
            .to_lowercase();
            PickerRow { card, search, letter }
        };
        let mut groups = vec![
            PickerGroup { name: "Built in", rows: gallery.defaults.into_iter().map(|card| row(card, "", None)).collect() },
            PickerGroup { name: CLASSIC_FAMILY, rows: gallery.classic.into_iter().map(|card| row(card, CLASSIC_FAMILY, None)).collect() },
        ];
        for family in gallery.families {
            let standalone = family.name == STANDALONE_FAMILY;
            let mut previous = None;
            let rows = family
                .cards
                .into_iter()
                .map(|card| {
                    if !standalone {
                        return row(card, family.name, None);
                    }
                    let first = card.name.chars().next().map_or('#', |c| if c.is_alphabetic() { c.to_ascii_uppercase() } else { '#' });
                    let letter = (previous != Some(first)).then_some(first);
                    previous = Some(first);
                    row(card, "", letter)
                })
                .collect();
            groups.push(PickerGroup { name: family.name, rows });
        }
        Self {
            page: page.clone(),
            labels,
            groups,
            total: themes::tinted().len() + Theme::BUILT_IN.len() + Theme::CLASSIC.len(),
            stylesheet: gallery.stylesheet,
            asset_version: gallery.asset_version,
        }
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
        let title = AppearanceMessage::Title.text(&page.locale);
        Self { page: page.titled(title.as_str()), settings_nav: SettingsSection::Appearance.nav(), gallery }
    }
}

#[cfg(test)]
mod tests;
