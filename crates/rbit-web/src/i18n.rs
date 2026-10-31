//! Request-scoped, immutable presentation messages. Protocol values never pass here.

use std::borrow::Cow;

use rbit_messages::{LocaleSnapshot, MessageRef};

tokio::task_local! {
    static MESSAGES: LocaleSnapshot;
}

/// Render outside a request (including template tests) with the built-in English catalog.
#[must_use]
pub fn current_snapshot() -> LocaleSnapshot {
    MESSAGES.try_with(Clone::clone).unwrap_or_else(|_| LocaleSnapshot::english(0))
}

/// Resolve text from the same catalog snapshot for the complete request.
#[must_use]
pub fn text(namespace: &str, key: &str) -> Cow<'static, str> {
    MESSAGES
        .try_with(|messages| {
            if messages.lang() == "en"
                && let Some(english) = rbit_messages::english_static(namespace, key)
            {
                return Cow::Borrowed(english);
            }
            Cow::Owned(messages.text(namespace, key))
        })
        .unwrap_or_else(|_| {
            rbit_messages::english_static(namespace, key)
                .map_or_else(|| Cow::Owned(LocaleSnapshot::english(0).text(namespace, key)), Cow::Borrowed)
        })
}

/// Format named typed arguments; Askama escapes the resulting text as usual.
#[must_use]
pub fn format_ref(message: &MessageRef<'_>) -> String {
    MESSAGES.try_with(|messages| messages.format(message)).unwrap_or_else(|_| LocaleSnapshot::english(0).format(message))
}

pub(crate) async fn scope<T>(snapshot: LocaleSnapshot, future: impl Future<Output = T>) -> T {
    MESSAGES.scope(snapshot, future).await
}

/// A bounded typed count used by the language-pack administrator page.
#[must_use]
pub fn pack_count(count: usize) -> String {
    use rbit_messages::{Argument, Value};
    let count = f64::from(u32::try_from(count).unwrap_or(u32::MAX));
    format_ref(&MessageRef {
        namespace: "core",
        key: "language-pack-count",
        args: &[Argument { name: "count", value: Value::Number(count) }],
    })
}

/// The browser receives a small text-only subset, with host-formatted argument slots.
pub fn browser_messages(snapshot: &LocaleSnapshot) -> String {
    use rbit_messages::{Argument, Value};
    const KEYS: &[(&str, &[&str])] = &[
        ("browser-copied", &[]),
        ("browser-passkey-cancelled", &[]),
        ("browser-passkey-registered", &[]),
        ("browser-passkey-unknown", &[]),
        ("browser-passkey-address", &[]),
        ("browser-passkey-failed", &[]),
        ("browser-passkey-password", &[]),
        ("browser-markdown-editor", &[]),
        ("browser-write", &[]),
        ("browser-preview", &[]),
        ("browser-preview-loading", &[]),
        ("browser-preview-failed", &[]),
        ("browser-mermaid-diagram", &[]),
        ("browser-diagram-source", &[]),
        ("browser-mirror-reconnecting", &[]),
        ("browser-mention-limit", &[]),
        ("browser-reply-limit", &[]),
        ("browser-language-files", &[]),
        ("browser-files-loading", &[]),
        ("browser-close", &[]),
        ("browser-files-failed", &[]),
        ("browser-retry", &[]),
        ("browser-file-loading", &[]),
        ("browser-open-file", &[]),
        ("browser-file-failed", &[]),
        ("browser-branches-loading", &[]),
        ("browser-refs-loading", &[]),
        ("browser-refs-more", &[]),
        ("browser-branches-empty", &[]),
        ("browser-refs-empty", &[]),
        ("browser-branches-failed", &[]),
        ("browser-refs-failed", &[]),
        ("browser-form-loading", &[]),
        ("browser-form-failed", &[]),
        ("browser-open-document", &[]),
        ("browser-update-available", &[]),
        ("browser-graph-help", &[]),
        ("browser-theme-builtin", &[]),
        ("browser-theme-applied", &[]),
        ("browser-theme-apply", &[]),
        ("browser-theme-one", &["count"]),
        ("browser-theme-many", &["count"]),
        ("browser-theme-author", &["author"]),
        ("browser-comment-line", &["line"]),
        ("browser-graph-selected", &["track"]),
        ("browser-commit-one", &["count"]),
        ("browser-commit-many", &["count"]),
        ("browser-theme-zero", &["count"]),
        ("browser-theme-two", &["count"]),
        ("browser-theme-few", &["count"]),
        ("browser-theme-other", &["count"]),
        ("browser-commit-zero", &["count"]),
        ("browser-commit-two", &["count"]),
        ("browser-commit-few", &["count"]),
        ("browser-commit-other", &["count"]),
        ("browser-contribution-day", &["count", "date"]),
        ("browser-contribution-summary", &["count", "percent"]),
        ("browser-contribution-summary-partial", &["count", "percent"]),
    ];
    let messages: std::collections::BTreeMap<_, _> = KEYS
        .iter()
        .map(|(key, names)| {
            let slots: Vec<String> = names.iter().map(|name| format!("\u{e000}{name}\u{e001}")).collect();
            let args: Vec<Argument<'_>> =
                names.iter().zip(&slots).map(|(name, slot)| Argument { name, value: Value::Text(slot) }).collect();
            (*key, snapshot.format(&MessageRef { namespace: "core", key, args: &args }))
        })
        // A long valid catalog must not inflate every page's browser payload.
        // Omitted entries retain each enhancement's bundled English fallback.
        .filter(|(_, text)| text.len() <= 1024)
        .collect();
    let encoded = serde_json::to_string(&messages).unwrap_or_default();
    if encoded.len() <= 64 * 1024 { encoded } else { "{}".to_owned() }
}

#[must_use]
pub fn pack_coverage(count: &usize, total: &usize) -> String {
    use rbit_messages::{Argument, Value};
    format_ref(&MessageRef {
        namespace: "core",
        key: "language-pack-coverage",
        args: &[
            Argument { name: "count", value: Value::Number(f64::from(u32::try_from(*count).unwrap_or(u32::MAX))) },
            Argument { name: "total", value: Value::Number(f64::from(u32::try_from(*total).unwrap_or(u32::MAX))) },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct RepeatedText {
        text: String,
    }

    impl rbit_messages::Messages for RepeatedText {
        fn format(&self, _: &MessageRef<'_>) -> String {
            self.text.clone()
        }
    }

    #[test]
    fn browser_catalog_bounds_each_message_and_the_complete_encoded_payload() {
        for text in ["x".repeat(1025), "\0".repeat(1024)] {
            let snapshot =
                LocaleSnapshot::new("fr".to_owned(), rbit_messages::Direction::Ltr, 7, std::sync::Arc::new(RepeatedText { text }));
            assert_eq!(browser_messages(&snapshot), "{}");
        }
        let encoded = browser_messages(&LocaleSnapshot::english(0));
        assert!(encoded.len() <= 64 * 1024);
        let catalog: std::collections::BTreeMap<String, String> = serde_json::from_str(&encoded).unwrap();
        assert!(catalog.contains_key("browser-copied"));
    }
}
