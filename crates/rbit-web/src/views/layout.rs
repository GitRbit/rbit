//! Shared page chrome.

use rbit_core::User;

use crate::{AppState, assets, themes::Theme, views::components::Avatar};

/// Data every full page needs: rendered by `templates/base.html`.
#[derive(Debug, Clone)]
pub struct Page {
    pub site_name: String,
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
            has_unread_notifications: false,
            theme: Theme::default(),
        }
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

    pub fn is_signed_in(&self) -> bool {
        self.viewer.is_some()
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
                site_name: "rbit".into(),
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
}
