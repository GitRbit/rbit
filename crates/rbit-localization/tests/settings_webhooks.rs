//! The English-only webhook families: every key renders its English text, Spanish has no
//! entries yet and falls back to it, and the diagnostic catalogs derive from it.
use std::{collections::BTreeSet, fmt::Debug};

use rbit_localization::{
    Catalog, Locale, MessageFamily,
    settings::webhooks::{WebhookMessage, WebhookText},
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
        assert!(!Catalog::new(Locale::PseudoRtl).localized(message).fallback(), "{message:?}");
    }
    messages.iter().map(|message| message.id()).collect()
}

#[test]
fn webhook_keys_render_in_english_and_fall_back_in_spanish() {
    let mut ids = check(WebhookMessage::ALL);
    ids.extend(check(&[
        WebhookText::MaxRepository(20),
        WebhookText::MaxOrganization(20),
        WebhookText::StopsDeliveries("ci.example.com"),
        WebhookText::RetryingCode(502),
        WebhookText::Created("3 hours ago"),
        WebhookText::OneAttempt(1),
        WebhookText::Attempts(2),
        WebhookText::NextAttempt("now"),
        WebhookText::ListTitle("ada/demo"),
        WebhookText::NewTitle("ada/demo"),
        WebhookText::EditTitle("ada/demo"),
        WebhookText::DeliveryTitle("0f8fad5b", "ada/demo"),
    ]));
    let distinct: BTreeSet<_> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len(), "every webhook key is distinct");
    assert!(ids.iter().all(|id| id.starts_with("webhooks.")));
}

#[test]
fn webhook_arguments_are_copied_into_the_english_text() {
    let english = Catalog::new(Locale::English);
    assert_eq!(english.localized(WebhookText::RetryingCode(502)).as_str(), "Retrying · 502");
    assert_eq!(
        english.localized(WebhookText::MaxRepository(20)).as_str(),
        "This repository has the maximum of 20 webhooks. Delete one to add another."
    );
}
