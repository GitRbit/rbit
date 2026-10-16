//! Shared page chrome.

use rustit_core::User;

use crate::{AppState, assets, views::components::Avatar};

/// Data every full page needs: rendered by `templates/base.html`.
#[derive(Debug, Clone)]
pub struct Page {
    pub site_name: String,
    pub title: Option<String>,
    pub viewer: Option<NavUser>,
    /// Request path, used to highlight the active navigation item.
    pub path: String,
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
        }
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
