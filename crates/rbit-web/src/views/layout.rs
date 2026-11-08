//! Shared page chrome.

use rbit_core::User;
use rbit_localization::{Catalog, Locale, Localized, MessageFamily};

use crate::{AppState, assets, themes::Theme, views::components::Avatar};

#[path = "layout/chrome.rs"]
mod chrome;
pub use chrome::PageChrome;

/// Data every full page needs: rendered by `templates/base.html`.
#[derive(Debug, Clone)]
pub struct Page {
    /// Supported document language/direction; defaults to English outside adapters.
    pub locale: Locale,
    /// Legacy feature content remains explicitly English until its catalog opts in.
    pub content_locale: Locale,
    pub updates: rbit_services::updates::UpdateStatus,
    pub site_name: String,
    pub branding: rbit_core::config::BrandingConfig,
    pub title: Option<String>,
    pub viewer: Option<NavUser>,
    /// Request path, used to highlight the active navigation item.
    pub path: String,
    /// A signed-in viewer has an unread notification: the header's bell shows a dot.
    pub has_unread_notifications: bool,
    /// The visitor's colour theme (from the `rbit-theme` cookie), rendered into `<html>`.
    pub theme: Theme,
}

/// The signed-in user as shown in the top bar.
#[derive(Debug, Clone)]
pub struct NavUser {
    pub name: String,
    pub label: String,
    pub avatar: Avatar,
    pub is_admin: bool,
    /// Instance settings may restrict organisation creation to administrators.
    pub can_create_org: bool,
    // Actions: interface and secrets (U).
    /// The instance has Actions: personal settings list their Actions section.
    pub has_actions: bool,
}

impl Page {
    pub const fn chrome(&self) -> PageChrome {
        PageChrome::new(self.locale)
    }

    pub fn new(state: &AppState, viewer: Option<&User>, path: &str) -> Self {
        let mut settings = state.services.instance.current();
        settings.branding.show_version &= state.config.updates.report_version;
        Self {
            locale: Locale::default(),
            content_locale: Locale::English,
            updates: state.services.updates.status(),
            site_name: settings.site_name,
            branding: settings.branding,
            title: None,
            viewer: viewer.map(|user| NavUser {
                name: user.name().to_string(),
                label: user.account.label().to_owned(),
                avatar: Avatar::for_account(&user.account),
                is_admin: user.is_admin,
                can_create_org: state.services.orgs.can_create(user),
                has_actions: state.services.actions.is_some(),
            }),
            path: path.to_owned(),
            has_unread_notifications: false,
            theme: Theme::default(),
        }
    }

    /// The supported document language and direction for this request, for a page whose
    /// content comes from the original exhaustive families (Explore, Actions, language
    /// settings): in a locale they do not cover, that content is English, and so is `<main>`.
    #[must_use]
    pub fn with_locale(mut self, locale: Locale) -> Self {
        self.locale = locale;
        self.content_locale = if locale.has_original_catalog() { locale } else { Locale::English };
        self
    }

    /// The language of text from the original exhaustive families on this page: the
    /// document's, or English where those families have no catalog for it.
    pub const fn original_locale(&self) -> Locale {
        if self.locale.has_original_catalog() { self.locale } else { Locale::English }
    }

    #[must_use]
    pub fn with_document_locale(mut self, locale: Locale) -> Self {
        self.locale = locale;
        self
    }

    /// Declares the main content in the document's language: for pages whose text all
    /// comes from the catalog through [`Page::text`].
    #[must_use]
    pub const fn with_catalog_content(mut self) -> Self {
        self.content_locale = self.locale;
        self
    }

    /// The page rendered in `theme`.
    #[must_use]
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    #[must_use]
    pub fn titled(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// `<title>` text: `Page · Site`, or just the site name.
    pub fn document_title(&self) -> String {
        match &self.title {
            Some(title) => format!("{title} · {}", self.site_name),
            None => self.site_name.clone(),
        }
    }

    /// Content-hashed URL of an embedded asset.
    pub fn asset(&self, path: &str) -> String {
        assets::url(path)
    }

    pub fn logo_url(&self) -> String {
        if self.branding.logo_url.is_empty() { self.asset("logo-384.avif") } else { self.branding.logo_url.clone() }
    }

    pub fn favicon_url(&self) -> String {
        if self.branding.favicon_url.is_empty() { self.asset("icon-32.png") } else { self.branding.favicon_url.clone() }
    }

    pub fn app_icon_url(&self) -> String {
        if self.branding.app_icon_url.is_empty() { self.asset("apple-touch-icon.png") } else { self.branding.app_icon_url.clone() }
    }

    pub fn welcome_title(&self) -> &str {
        if self.branding.welcome_title.is_empty() { &self.site_name } else { &self.branding.welcome_title }
    }

    pub fn is_signed_in(&self) -> bool {
        self.viewer.is_some()
    }

    // Actions: interface and secrets (U).
    /// Whether a signed-in viewer is on an instance with Actions (settings navigation).
    pub fn has_actions(&self) -> bool {
        self.viewer.as_ref().is_some_and(|viewer| viewer.has_actions)
    }

    /// A catalog message in the document's language, or its English fallback. Templates
    /// show it through `ui::localized`, which marks only the fallback English.
    pub fn text<M: MessageFamily>(&self, message: M) -> Localized {
        Catalog::new(self.locale).localized(message)
    }

    /// Whether the current path is `prefix` or below it (for active nav styling).
    pub fn is_at(&self, prefix: &str) -> bool {
        self.path == prefix || (prefix != "/" && self.path.starts_with(prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use sha2::{Digest, Sha256};

    #[derive(Template)]
    #[template(source = "{% extends \"base.html\" %}", ext = "html")]
    struct Fixture {
        page: Page,
    }

    #[test]
    fn initial_styles_render_verbatim_and_need_no_network_requests() {
        for slug in ["system", "dark", "base24-berlin"] {
            let page = Page {
                locale: rbit_localization::Locale::default(),
                content_locale: rbit_localization::Locale::English,
                updates: rbit_services::updates::UpdateStatus::default(),
                site_name: "rbit".into(),
                branding: rbit_core::config::BrandingConfig::default(),
                title: None,
                viewer: None,
                path: "/".into(),
                has_unread_notifications: false,
                theme: Theme::parse(slug).unwrap(),
            };
            let html = Fixture { page }.render().unwrap();
            assert!(!html.contains("rel=\"stylesheet\""));
            assert!(!html.contains("rel=\"manifest\""));
            assert!(html.contains("/assets/app.bundle.js?v="));
            let mut paths = vec!["app.css".to_owned()];
            paths.extend(Theme::parse(slug).unwrap().stylesheet_path());
            for (style, path) in html.split("<style ").skip(1).zip(&paths) {
                let css = style.split_once('>').unwrap().1.split_once("</style>").unwrap().0;
                assert_eq!(css, assets::inline_css(path));
                assert!(!css.to_ascii_lowercase().contains("</style"));
                assert_eq!(assets::style_source(path), format!("'sha256-{}'", STANDARD.encode(Sha256::digest(css.as_bytes()))));
            }
            assert_eq!(html.matches("<style ").count(), paths.len());
        }
    }

    #[test]
    fn update_footer_notice_remains_generic_when_running_version_is_hidden() {
        let branding = rbit_core::config::BrandingConfig { show_version: false, ..rbit_core::config::BrandingConfig::default() };
        let page = Page {
            locale: rbit_localization::Locale::default(),
            content_locale: rbit_localization::Locale::English,
            updates: rbit_services::updates::UpdateStatus {
                revision: 1,
                state: rbit_services::updates::UpdateState::Available,
                release: Some(rbit_services::updates::AvailableRelease {
                    version: "99.0.0".into(),
                    url: "https://github.com/GitRbit/rbit/releases/tag/v99.0.0".into(),
                }),
            },
            site_name: "rbit".into(),
            branding,
            title: None,
            viewer: None,
            path: "/".into(),
            has_unread_notifications: false,
            theme: Theme::default(),
        };
        let html = Fixture { page }.render().unwrap();
        assert!(html.contains("Update available"));
        assert!(html.contains("https://github.com/GitRbit/rbit/releases/tag/v99.0.0"));
        assert!(!html.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
    }

    #[test]
    fn chrome_of_a_locale_without_its_original_catalog_is_marked_english() {
        use rbit_localization::Locale;
        let german = Fixture { page: crate::views::test_support::page("/").with_locale(Locale::German) }.render().unwrap();
        assert!(german.contains("<html lang=\"de\" dir=\"ltr\" data-catalog-key=\"1.de\""));
        assert!(german.contains("<a href=\"#main\" lang=\"en\""));
        assert!(german.contains("<header lang=\"en\""));
        assert!(german.contains("<main id=\"main\" lang=\"en\""), "the original families' content is English");
        assert!(german.contains("<nav lang=\"en\" class=\"flex max-w-full"));
        assert!(german.contains(">Skip to content</a>"));
        let errors = crate::views::test_support::page("/").with_locale(Locale::Japanese).with_catalog_content();
        assert_eq!(errors.content_locale, Locale::Japanese, "catalog pages keep their own language");
        let spanish = Fixture { page: crate::views::test_support::page("/").with_locale(Locale::Spanish) }.render().unwrap();
        assert!(spanish.contains("<header data-document-catalog=\"1.es\""));
        assert!(spanish.contains("<main id=\"main\" lang=\"es\""));
        assert!(!spanish.contains("<header lang") && !spanish.contains("<nav lang") && !spanish.contains("<a href=\"#main\" lang"));
    }
}
