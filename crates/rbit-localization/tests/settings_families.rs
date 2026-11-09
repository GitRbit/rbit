//! The account settings families: every key renders its English text, Spanish translates
//! it, and the diagnostic catalogs derive from the English.
use std::{collections::BTreeSet, fmt::Debug};

use rbit_localization::{
    Catalog, Locale, MessageFamily,
    settings::{
        account::{AccountMessage, AccountText},
        keys::{KeyLabel, KeysMessage},
        profile::{AppearanceMessage, GalleryMessage, GalleryText, NotificationsMessage, NotificationsText, ProfileMessage},
        security::{SecurityMessage, SecurityText},
        tokens::{TokenText, TokensMessage},
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
        assert!(!Catalog::new(Locale::PseudoExpanded).localized(message).fallback(), "{message:?}");
    }
    messages.iter().map(|message| message.id()).collect()
}

#[test]
fn settings_keys_render_in_english_and_translate_to_spanish() {
    let mut ids = Vec::new();
    ids.extend(check(KeysMessage::ALL));
    ids.extend(check(AccountMessage::ALL));
    ids.extend(check(ProfileMessage::ALL));
    ids.extend(check(AppearanceMessage::ALL));
    ids.extend(check(NotificationsMessage::ALL));
    ids.extend(check(GalleryMessage::ALL));
    ids.extend(check(TokensMessage::ALL));
    ids.extend(check(SecurityMessage::ALL));
    ids.extend(check(&[KeyLabel::Delete("laptop")]));
    ids.extend(check(&[AccountText::InitialPassword("rbit"), AccountText::PasswordHint(12)]));
    ids.extend(check(&[NotificationsText::DefaultHour("08:00 UTC")]));
    ids.extend(check(&[GalleryText::SearchCount(500)]));
    ids.extend(check(&[
        TokenText::Created("ci"),
        TokenText::RevokeLabel("ci"),
        TokenText::Days(30),
        TokenText::CreatedOn("Oct 5, 2026"),
        TokenText::LastUsed("3 hours ago"),
        TokenText::Expires("Nov 14, 2026"),
        TokenText::Expired("Oct 1, 2026"),
    ]));
    ids.extend(check(&[
        SecurityText::AddToApp("rbit"),
        SecurityText::UnlinkLabel("GitHub"),
        SecurityText::SignOutLabel("Firefox on Linux"),
        SecurityText::WithPasskey("Remove"),
    ]));
    let distinct: BTreeSet<_> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len(), "every settings key is distinct");
    assert!(ids.iter().all(|id| id.starts_with("settings.")));
}

#[test]
fn arguments_are_copied_into_the_english_text() {
    let english = Catalog::new(Locale::English);
    assert_eq!(english.localized(KeyLabel::Delete("laptop")).as_str(), "Delete laptop");
    assert_eq!(english.localized(TokenText::Created("ci")).as_str(), "Token “ci” created");
    assert_eq!(
        english.localized(AccountText::PasswordHint(12)).as_str(),
        "At least 12 characters. A few unrelated words make a strong, memorable password."
    );
    let rtl = Catalog::new(Locale::PseudoRtl).localized(KeyLabel::Delete("laptop"));
    assert!(rtl.as_str().contains("\u{2068}laptop\u{2069}"), "diagnostics isolate arguments");
}

#[test]
fn text_takes_the_page_locale() {
    let spanish = KeysMessage::Title.text(&Locale::Spanish);
    assert_eq!(spanish.as_str(), "Claves SSH");
    assert!(!spanish.fallback());
    assert!(!KeysMessage::Title.text(&Locale::English).fallback());
}
