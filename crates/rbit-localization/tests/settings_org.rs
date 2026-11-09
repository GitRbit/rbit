//! The organization families: every key renders its English text, Spanish translates it,
//! and the diagnostic catalogs derive from the English.
use std::{collections::BTreeSet, fmt::Debug};

use rbit_localization::{
    Catalog, Locale, MessageFamily,
    settings::{
        org::{OrgMessage, OrgText},
        org_teams::{TeamMessage, TeamText},
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
        assert!(!Catalog::new(Locale::PseudoRtl).localized(message).fallback(), "{message:?}");
    }
    messages.iter().map(|message| message.id()).collect()
}

#[test]
fn organization_keys_render_in_english_and_translate_to_spanish() {
    let mut ids = Vec::new();
    ids.extend(check(OrgMessage::ALL));
    ids.extend(check(TeamMessage::ALL));
    ids.extend(check(&[
        OrgText::StillOwnsOne("acme"),
        OrgText::StillOwns("acme", 3),
        OrgText::RemoveLabel("alice"),
        OrgText::RoleLabel("Make owner", "alice"),
        OrgText::MembersTitle("Acme"),
        OrgText::SettingsTitle("Acme"),
    ]));
    ids.extend(check(&[
        TeamText::Members(3),
        TeamText::Repositories(2),
        TeamText::PermissionOn("acme/rocket"),
        TeamText::SavePermissionOn("acme/rocket"),
        TeamText::TeamsTitle("Acme"),
        TeamText::NewTeamTitle("Acme"),
        TeamText::SettingsTitle("acme/core"),
    ]));
    let distinct: BTreeSet<_> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len(), "every organization key is distinct");
    assert!(ids.iter().all(|id| id.starts_with("org.")));
}

#[test]
fn organization_text_keeps_its_arguments() {
    let english = Catalog::new(Locale::English);
    assert_eq!(english.localized(OrgText::StillOwns("acme", 3)).as_str(), "acme still owns 3 repositories. Delete or transfer them first.");
    assert_eq!(english.localized(OrgText::RoleLabel("Make owner", "alice")).as_str(), "Make owner: alice");
    assert_eq!(english.localized(TeamText::Members(3)).as_str(), "3 members");
    assert_eq!(TeamMessage::OneMember.text(&Locale::Spanish).as_str(), "1 miembro");
}
