//! The repository transfer page.
//!
//! Milestone 2 workstream A (access) owns this file.
//!
//! `/{owner}/{repo}/settings/transfer` offers the accounts the viewer may move the
//! repository to (site administrators may type any name) and asks for the full
//! `owner/name` as confirmation.

use askama::Template;
use askama_web::WebTemplate;
use rustit_core::Account;

use crate::views::{
    components::{Avatar, FormError, SettingsCard},
    layout::Page,
    repo::RepoHeader,
};

/// Cards shown in the danger zone of the repository settings page (e.g. "Transfer ownership").
pub fn danger_zone_cards(header: &RepoHeader) -> Vec<SettingsCard> {
    vec![SettingsCard {
        url: format!("{}/transfer", header.settings_url),
        icon: "transfer",
        title: "Transfer ownership",
        description: "Move this repository to another user or organization. Its URL changes.",
    }]
}

/// One account offered as the new owner.
#[derive(Debug, Clone)]
pub struct OwnerOption {
    pub name: String,
    pub label: String,
    pub avatar: Avatar,
    pub is_organization: bool,
    pub selected: bool,
}

/// The form's values, blank or as submitted.
#[derive(Debug, Clone, Default)]
pub struct TransferForm {
    pub new_owner: String,
    pub confirm: String,
}

/// `/{owner}/{repo}/settings/transfer`.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "access/transfer.html")]
pub struct TransferPage {
    pub page: Page,
    pub header: RepoHeader,
    /// `owner/name`, which must be typed to confirm.
    pub full_name: String,
    pub settings_url: String,
    pub action_url: String,
    pub owners: Vec<OwnerOption>,
    /// Site administrators may enter any user or organisation.
    pub free_entry: bool,
    pub form: TransferForm,
    pub error: FormError,
}

impl TransferPage {
    pub fn new(page: Page, header: RepoHeader, suggestions: &[Account], free_entry: bool) -> Self {
        let full_name = format!("{}/{}", header.owner, header.name);
        let mut transfer = Self {
            page: page.titled(format!("Transfer · {full_name}")),
            settings_url: header.settings_url.clone(),
            action_url: format!("{}/transfer", header.settings_url),
            owners: suggestions
                .iter()
                .map(|account| OwnerOption {
                    name: account.name.to_string(),
                    label: account.label().to_owned(),
                    avatar: Avatar::for_account(account),
                    is_organization: account.is_organization(),
                    selected: false,
                })
                .collect(),
            full_name,
            header,
            free_entry,
            form: TransferForm::default(),
            error: FormError::default(),
        };
        if let Some(first) = transfer.owners.first_mut().filter(|_| !free_entry) {
            first.selected = true;
        }
        transfer
    }

    /// Re-renders with `error` above the form, keeping what was submitted.
    #[must_use]
    pub fn rejected(mut self, form: TransferForm, error: FormError) -> Self {
        for owner in &mut self.owners {
            owner.selected = owner.name.eq_ignore_ascii_case(form.new_owner.trim());
        }
        self.form = form;
        self.error = error;
        self
    }

    /// Whether there is anywhere to move the repository to.
    pub fn has_targets(&self) -> bool {
        self.free_entry || !self.owners.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use rustit_core::{AccountId, AccountKind, Timestamp};

    use super::*;

    fn account(id: i64, name: &str, kind: AccountKind) -> Account {
        Account { id: AccountId(id), kind, name: name.parse().unwrap(), display_name: None, bio: None, created_at: Timestamp(0) }
    }

    fn header() -> RepoHeader {
        RepoHeader {
            owner: "alice".to_owned(),
            name: "rocket".to_owned(),
            home_url: "/alice/rocket".to_owned(),
            settings_url: "/alice/rocket/settings".to_owned(),
            can_admin: true,
            active_tab: "settings",
            ..RepoHeader::default()
        }
    }

    fn page() -> Page {
        Page {
            site_name: "Rustit".to_owned(),
            title: None,
            viewer: None,
            path: "/alice/rocket/settings/transfer".to_owned(),
            theme: crate::themes::Theme::System,
            has_unread_notifications: false,
        }
    }

    #[test]
    fn danger_zone_links_to_the_page() {
        let cards = danger_zone_cards(&header());
        assert_eq!(cards[0].url, "/alice/rocket/settings/transfer");
        assert_eq!(cards[0].icon, "transfer");
    }

    #[test]
    fn members_pick_from_their_organisations() {
        let owners = [account(2, "acme", AccountKind::Organization), account(3, "beta", AccountKind::Organization)];
        let form = TransferForm { new_owner: "beta".to_owned(), confirm: "alice/rocket".to_owned() };
        let html = TransferPage::new(page(), header(), &owners, false).rejected(form, FormError::new("Clash.")).render().unwrap();
        assert!(html.contains(r#"action="/alice/rocket/settings/transfer""#));
        assert!(html.contains(r#"value="acme""#) && html.contains(r#"value="beta" checked"#));
        assert!(html.contains("Clash."));
        assert!(html.contains("alice/rocket"));
        assert!(!html.contains(r#"list="transfer-owners""#), "no free entry for non-administrators");
    }

    #[test]
    fn site_admins_may_type_any_name_and_others_may_have_nowhere_to_go() {
        let html = TransferPage::new(page(), header(), &[account(1, "root", AccountKind::User)], true).render().unwrap();
        assert!(html.contains(r#"list="transfer-owners""#));
        let nowhere = TransferPage::new(page(), header(), &[], false);
        assert!(!nowhere.has_targets());
        let html = nowhere.render().unwrap();
        assert!(html.contains("no organization"), "explains why nothing can be chosen");
    }
}
