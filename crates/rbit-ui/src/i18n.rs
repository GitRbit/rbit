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

/// Use one captured catalog while polling a future; nested scopes restore their parent.
/// Run presentation work with one immutable catalog, restoring the enclosing
/// catalog after each poll. Spawned tasks do not inherit this context; use
/// [`crate::template::Localized`] to carry it into separately owned rendering.
/// This scope controls text, never authentication or operation authority.
pub async fn scope<T>(snapshot: LocaleSnapshot, future: impl Future<Output = T>) -> T {
    MESSAGES.scope(snapshot, future).await
}

/// Use a captured catalog for synchronous rendering, restoring the previous scope.
/// Render synchronously with a captured catalog and restore the enclosing context
/// on return, including a template or writer error.
pub fn render_scope<T>(snapshot: LocaleSnapshot, render: impl FnOnce() -> T) -> T {
    MESSAGES.sync_scope(snapshot, render)
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

    fn snapshot(lang: &str, value: &str) -> LocaleSnapshot {
        LocaleSnapshot::new(lang.to_owned(), rbit_messages::Direction::Ltr, 7, std::sync::Arc::new(RepeatedText { text: value.to_owned() }))
    }

    #[tokio::test]
    async fn english_fallback_and_nested_async_and_render_scopes_restore_the_parent() {
        let english = text("core", "browser-copied").into_owned();
        assert_eq!(current_snapshot().lang(), "en");
        assert_eq!(english, rbit_messages::english_static("core", "browser-copied").unwrap());
        scope(snapshot("fr", "Outer"), async {
            assert_eq!(text("core", "browser-copied"), "Outer");
            scope(snapshot("de", "Inner"), async {
                tokio::task::yield_now().await;
                assert_eq!(text("core", "browser-copied"), "Inner");
                render_scope(snapshot("es", "Rendered"), || {
                    assert_eq!(text("core", "browser-copied"), "Rendered");
                });
                assert_eq!(current_snapshot().lang(), "de");
            })
            .await;
            assert_eq!(text("core", "browser-copied"), "Outer");
        })
        .await;
        assert_eq!(text("core", "browser-copied"), english);
    }

    #[tokio::test]
    async fn concurrently_polled_locales_and_cancelled_scopes_do_not_leak() {
        async fn check(lang: &str, value: &str) {
            scope(snapshot(lang, value), async {
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                    assert_eq!(text("core", "browser-copied"), value);
                    assert_eq!(current_snapshot().lang(), lang);
                }
            })
            .await;
        }
        tokio::join!(check("fr", "French"), check("de", "German"));
        scope(snapshot("fr", "Parent"), async {
            let mut cancelled = Box::pin(scope(snapshot("de", "Cancelled"), async {
                assert_eq!(current_snapshot().lang(), "de");
                std::future::pending::<()>().await;
            }));
            assert!(std::future::poll_fn(|cx| std::task::Poll::Ready(cancelled.as_mut().poll(cx).is_pending())).await);
            drop(cancelled);
            assert_eq!(text("core", "browser-copied"), "Parent");
        })
        .await;
        assert_eq!(current_snapshot().lang(), "en");
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
