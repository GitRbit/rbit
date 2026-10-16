//! Shared page chrome.

use rustit_core::User;

use crate::{AppState, assets, themes::Theme, views::components::Avatar};

/// Data every full page needs: rendered by `templates/base.html`.
#[derive(Debug, Clone)]
pub struct Page {
    pub site_name: String,
    pub title: Option<String>,
    pub viewer: Option<NavUser>,
    /// Request path, used to highlight the active navigation item.
    pub path: String,
    /// The visitor's colour theme (from the `rustit-theme` cookie), rendered into `<html>`.
    pub theme: Theme,
}

/// One entry of the header's theme menu.
#[derive(Debug, Clone)]
pub struct ThemeOption {
    pub slug: &'static str,
    pub label: &'static str,
    pub is_current: bool,
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
}

impl Page {
    pub fn new(state: &AppState, viewer: Option<&User>, path: &str) -> Self {
        Self {
            site_name: state.services.instance.current().site_name,
            title: None,
            viewer: viewer.map(|user| NavUser {
                name: user.name().to_string(),
                label: user.account.label().to_owned(),
                avatar: Avatar::for_account(&user.account),
                is_admin: user.is_admin,
                can_create_org: state.services.orgs.can_create(user),
            }),
            path: path.to_owned(),
            theme: Theme::default(),
        }
    }

    /// The page rendered in `theme`.
    #[must_use]
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// System / Light / Dark for the header menu, with the active one marked.
    pub fn theme_options(&self) -> [ThemeOption; 3] {
        Theme::BUILT_IN.map(|theme| ThemeOption {
            slug: theme.slug(),
            label: match theme {
                Theme::Light => "Light",
                Theme::Dark => "Dark",
                _ => "System",
            },
            is_current: theme == self.theme,
        })
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

    pub fn is_signed_in(&self) -> bool {
        self.viewer.is_some()
    }

    /// Whether the current path is `prefix` or below it (for active nav styling).
    pub fn is_at(&self, prefix: &str) -> bool {
        self.path == prefix || (prefix != "/" && self.path.starts_with(prefix))
    }
}
