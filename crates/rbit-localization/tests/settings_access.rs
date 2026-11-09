//! The repository access and branch protection families: every key renders its English
//! text and is translated in Spanish.
use std::{collections::BTreeSet, fmt::Debug};

use rbit_localization::{
    Catalog, Locale, MessageFamily,
    settings::{
        access::{AccessMessage, AccessText},
        protection::{ProtectionMessage, ProtectionText, RuleBadge},
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
fn access_keys_render_in_english_and_translate_to_spanish() {
    let mut ids = Vec::new();
    ids.extend(check(AccessMessage::ALL));
    ids.extend(check(ProtectionMessage::ALL));
    ids.extend(check(&[
        AccessText::PermissionFor("bob"),
        AccessText::SavePermissionFor("bob"),
        AccessText::RemoveLabel("bob"),
        AccessText::Added("Oct 15, 2026"),
        AccessText::CollaboratorsTitle("acme/rocket"),
        AccessText::TransferTitle("acme/rocket"),
    ]));
    ids.extend(check(&[
        ProtectionText::MaxRules(50),
        ProtectionText::ChecksHint(20),
        ProtectionText::Matches("main"),
        ProtectionText::MatchesMore("a, b", 3),
        ProtectionText::ListTitle("acme/rocket"),
        ProtectionText::NewTitle("acme/rocket"),
        ProtectionText::EditTitle("main", "acme/rocket"),
    ]));
    ids.extend(check(&[
        RuleBadge::NoForcePushes,
        RuleBadge::NoDeletion,
        RuleBadge::PullRequest,
        RuleBadge::Approvals(2),
        RuleBadge::StatusChecks,
        RuleBadge::Restricted,
        RuleBadge::AdminsBypass,
    ]));
    let distinct: BTreeSet<_> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len(), "every key is distinct");
}

#[test]
fn counts_and_names_are_written_into_the_text() {
    let english = Catalog::new(Locale::English);
    assert_eq!(english.localized(RuleBadge::Approvals(1)).as_str(), "1 approval");
    assert_eq!(english.localized(RuleBadge::Approvals(3)).as_str(), "3 approvals");
    assert_eq!(english.localized(ProtectionText::MatchesMore("a, b", 3)).as_str(), "Matches a, b and 3 more");
    assert_eq!(english.localized(AccessText::RemoveLabel("bob")).as_str(), "Remove bob");
}
