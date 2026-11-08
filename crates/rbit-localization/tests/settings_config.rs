//! The configuration schema's text: every key renders in English, falls back to it in
//! Spanish, and has its own id; an unknown key renders as itself.
use std::collections::BTreeSet;

use rbit_localization::{
    Catalog, Locale, MessageFamily,
    settings::config::{ConfigMessage, ConfigPageText, ConfigText},
};

#[test]
fn every_schema_key_renders_in_english_and_falls_back_in_spanish() {
    let all = ConfigText::all();
    assert!(all.len() > 300, "fields, sections and choices are listed");
    let mut ids = BTreeSet::new();
    for message in all {
        assert!(message.exists(), "{message:?}");
        assert!(ids.insert(message.id()), "{message:?} has a distinct id");
        assert!(message.id().starts_with("admin.configuration."));
        let english = message.text(&Locale::English);
        assert!(!english.as_str().is_empty() && !english.fallback(), "{message:?}");
        let spanish = message.text(&Locale::Spanish);
        assert_eq!(spanish.as_str(), english.as_str(), "{message:?}");
        assert!(spanish.fallback(), "{message:?}");
    }
    for &message in ConfigMessage::ALL {
        assert!(ids.insert(message.id()), "{message:?} has a distinct id");
        assert!(Catalog::new(Locale::Spanish).localized(message).fallback());
    }
}

#[test]
fn fields_are_keyed_by_section() {
    let english = |message: ConfigText<'_>| message.text(&Locale::English).to_string();
    assert_eq!(english(ConfigText::Label { section: "server", key: "bind" }), "Listen address");
    assert_eq!(english(ConfigText::SectionTitle("server")), "Server");
    assert_eq!(english(ConfigText::Choice { section: "mail", key: "transport", value: "smtp" }), "SMTP server");
    assert_ne!(
        ConfigText::Label { section: "server", key: "bind" }.id(),
        ConfigText::Label { section: "ssh", key: "bind" }.id(),
        "the same field key in two sections has two entries"
    );
    let save = ConfigPageText::SaveSection("Server").text(&Locale::English);
    assert_eq!(save.as_str(), "Save Server settings");
}

#[test]
fn an_unknown_key_renders_as_itself() {
    let unknown = ConfigText::Label { section: "server", key: "made_up" };
    assert!(!unknown.exists());
    assert_eq!(unknown.id(), "admin.configuration.unknown");
    assert_eq!(unknown.text(&Locale::English).as_str(), "made_up");
}
