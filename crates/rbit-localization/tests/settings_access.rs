//! The English-only repository access family: every key renders
//! its English text and falls back to it in Spanish.
use std::{collections::BTreeSet, fmt::Debug};

use rbit_localization::{
    Catalog, Locale, MessageFamily,
    settings::access::{AccessMessage, AccessText},
};

/// Renders every message in English and Spanish, and returns their ids.
fn check<M: MessageFamily + Debug>(messages: &[M]) -> Vec<&'static str> {
    for &message in messages {
        let english = Catalog::new(Locale::English).localized(message);
        assert!(!english.as_str().is_empty(), "{message:?}");
        assert!(!english.fallback(), "{message:?} is English");
        let spanish = Catalog::new(Locale::Spanish).localized(message);
        assert_eq!(spanish.as_str(), english.as_str(), "{message:?} falls back to English");
        assert!(spanish.fallback(), "{message:?} is marked as a fallback in Spanish");
    }
    messages.iter().map(|message| message.id()).collect()
}

#[test]
fn access_keys_render_in_english_and_fall_back_in_spanish() {
    let mut ids = Vec::new();
    ids.extend(check(AccessMessage::ALL));
    ids.extend(check(&[
        AccessText::PermissionFor("bob"),
        AccessText::SavePermissionFor("bob"),
        AccessText::RemoveLabel("bob"),
        AccessText::Added("Oct 15, 2026"),
        AccessText::CollaboratorsTitle("acme/rocket"),
        AccessText::TransferTitle("acme/rocket"),
    ]));
    let distinct: BTreeSet<_> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len(), "every key is distinct");
}

#[test]
fn names_are_written_into_the_text() {
    let english = Catalog::new(Locale::English);
    assert_eq!(english.localized(AccessText::RemoveLabel("bob")).as_str(), "Remove bob");
}
