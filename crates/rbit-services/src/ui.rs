//! Trusted built-in plugin contributions and presentation-only host snapshots.

pub use rbit_plugin_api::{
    UiContent, UiContribution, UiContributionId, UiIcon, UiLink, UiMessage, UiNoticeTone, UiSlot, UiTarget, UiVisibility,
};
pub use rbit_plugin_runtime::UiEntry as PublishedUiContribution;
pub use rbit_plugin_runtime::{UiEntry, UiSnapshot};

#[cfg(any(feature = "actions", feature = "i18n"))]
use rbit_plugin_api::UiContributionSet;

#[cfg(any(feature = "actions", feature = "i18n"))]
fn message(key: &str, fallback: &str) -> UiMessage {
    UiMessage { namespace: "core".to_owned(), key: key.to_owned(), fallback: fallback.to_owned() }
}

#[cfg(any(feature = "actions", feature = "i18n"))]
fn link(key: &str, label: &str, target: &str, icon: UiIcon) -> UiLink {
    UiLink { label: message(key, label), target: target.parse().expect("built-in UI target is valid"), icon }
}

#[cfg(feature = "actions")]
pub(crate) fn actions() -> UiContributionSet {
    let entry = |id: &str, slot, order, visibility, content| UiContribution {
        id: id.parse().expect("built-in UI contribution ID is valid"),
        slot,
        order,
        visibility,
        content,
    };
    UiContributionSet::new(vec![
        entry(
            "actions-repository-tab",
            UiSlot::RepositoryTab,
            60,
            UiVisibility::RepositoryFeatureEnabled,
            UiContent::Link(link("plugin-actions", "Actions", "/{owner}/{repo}/actions", UiIcon::Play)),
        ),
        entry(
            "actions-repository-settings",
            UiSlot::RepositorySettings,
            60,
            UiVisibility::RepositoryAdmin,
            UiContent::Card {
                title: message("plugin-actions", "Actions"),
                body: message(
                    "ui-actions-repository-description",
                    "Run workflows on pushes and pull requests, with the secrets and variables they use.",
                ),
                action: Some(link("plugin-actions", "Actions", "/{owner}/{repo}/settings/actions", UiIcon::Play)),
            },
        ),
        entry(
            "actions-account-settings",
            UiSlot::AccountSettings,
            80,
            UiVisibility::SignedIn,
            UiContent::Link(link("plugin-actions", "Actions", "/settings/actions", UiIcon::Play)),
        ),
        entry(
            "actions-admin-runners",
            UiSlot::AdminNav,
            90,
            UiVisibility::InstanceAdmin,
            UiContent::Link(link("admin-nav-runners", "Runners", "/admin/runners", UiIcon::Terminal)),
        ),
        entry(
            "actions-organization-settings",
            UiSlot::OrganizationSettings,
            60,
            UiVisibility::OrganizationAdmin,
            UiContent::Card {
                title: message("plugin-actions", "Actions"),
                body: message(
                    "ui-actions-organization-description",
                    "Secrets and variables for the workflows of every repository of this organization.",
                ),
                action: Some(link("plugin-actions", "Actions", "/{owner}/settings/actions", UiIcon::Play)),
            },
        ),
    ])
}

#[cfg(feature = "i18n")]
pub(crate) fn i18n() -> UiContributionSet {
    UiContributionSet::new(vec![UiContribution {
        id: "i18n-account-language".parse().expect("built-in UI contribution ID is valid"),
        slot: UiSlot::AccountSettings,
        order: 90,
        visibility: UiVisibility::SignedIn,
        content: UiContent::Link(link("locale-title", "Language", "/language", UiIcon::Globe)),
    }])
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "actions")]
    #[test]
    fn actions_builtin_descriptors_validate_for_their_plugin_identity() {
        let contributions = super::actions();
        contributions.validate(&"rbit.actions".parse().unwrap()).unwrap();
        assert_eq!(contributions.contributions.len(), 5);
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn i18n_builtin_descriptors_validate_for_their_plugin_identity() {
        let contributions = super::i18n();
        contributions.validate(&"rbit.i18n".parse().unwrap()).unwrap();
        assert_eq!(contributions.contributions.len(), 1);
    }
}
