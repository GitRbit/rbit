//! Shared page chrome for view unit tests, with explicit per-test overrides.
//!
//! Feature fixtures choose their own path and can use the normal Page builders or
//! struct update syntax for theme, viewer, branding, title and locale differences.

use super::layout::Page;

pub(crate) fn page(path: &str) -> Page {
    Page {
        locale: rbit_localization::Locale::default(),
        content_locale: rbit_localization::Locale::English,
        updates: rbit_services::updates::UpdateStatus::default(),
        site_name: "rbit".to_owned(),
        branding: rbit_core::config::BrandingConfig::default(),
        title: None,
        viewer: None,
        path: path.to_owned(),
        theme: crate::themes::Theme::System,
        has_unread_notifications: false,
    }
}
