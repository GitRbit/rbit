//! The administration families and the settings index cards: every key renders its
//! English text and is translated in Spanish.
use std::{collections::BTreeSet, fmt::Debug};

use rbit_localization::{
    Catalog, Locale, MessageFamily,
    settings::{
        admin::{AdminMessage, AdminText, DashboardMessage, MonitoringMessage},
        admin_pages::{AdminPageMessage, AdminPageText, InstanceSettingsMessage},
        cards::CardMessage,
    },
};

/// Renders every message in English and Spanish, and returns their ids.
fn check<M: MessageFamily + Debug>(messages: &[M]) -> Vec<&'static str> {
    for &message in messages {
        let english = Catalog::new(Locale::English).localized(message);
        assert!(!english.as_str().is_empty(), "{message:?}");
        assert!(!english.fallback(), "{message:?} is English");
        let spanish = Catalog::new(Locale::Spanish).localized(message);
        assert!(!spanish.as_str().is_empty(), "{message:?}");
        assert!(!spanish.fallback(), "{message:?} is translated in Spanish");
    }
    messages.iter().map(|message| message.id()).collect()
}

#[test]
fn administration_keys_render_in_english_and_translate_to_spanish() {
    let mut ids = Vec::new();
    ids.extend(check(AdminMessage::ALL));
    ids.extend(check(DashboardMessage::ALL));
    ids.extend(check(MonitoringMessage::ALL));
    ids.extend(check(AdminPageMessage::ALL));
    ids.extend(check(InstanceSettingsMessage::ALL));
    ids.extend(check(&[
        AdminText::UsersDetail("2", "0"),
        AdminText::UsersDetailOne("0"),
        AdminText::RepositoriesDetail("3", "1"),
        AdminText::Summary("1", "30", "112"),
        AdminText::ReleaseNotes("1.0.0"),
        AdminText::DigestsAt("08:00 UTC"),
        AdminText::DeleteLabel("acme/site"),
        AdminText::ActionsFor("alice"),
        AdminText::PasswordLength(12),
        AdminText::ChartLabel("Response time", "4.2", "ms avg"),
        AdminText::UpdatedAt("10:00:05"),
        AdminText::Coverage(12, 60),
        AdminText::InAdministration("Users"),
        AdminText::SetPasswordFor("alice"),
        AdminText::DeletePageTitle("alice"),
    ]));
    ids.extend(check(&[AdminPageText::ShareSecurely(12), AdminPageText::ReplacesPassword("alice"), AdminPageText::DeleteUserTail("3")]));
    let distinct: BTreeSet<_> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len(), "every administration key is distinct");
    assert!(ids.iter().all(|id| id.starts_with("admin.")));
}

#[test]
fn settings_cards_render_in_english_and_translate_to_spanish() {
    let ids = check(CardMessage::ALL);
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());
}

#[test]
fn administration_arguments_are_copied_into_the_english_text() {
    let english = Catalog::new(Locale::English);
    assert_eq!(english.localized(AdminText::Summary("1", "30", "112")).as_str(), "1–30 of 112");
    assert_eq!(english.localized(AdminText::InAdministration("Users")).as_str(), "Users · Administration");
    assert_eq!(english.localized(AdminText::UsersDetailOne("2")).as_str(), "1 admin · 2 suspended");
}
