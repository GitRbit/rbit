//! Compiled presentation with an explicitly captured immutable locale.

use askama::Template;
use rbit_messages::LocaleSnapshot;

/// Keeps template helpers on the captured catalog, including when rendering in
/// independently owned asynchronous work. This wrapper carries presentation only.
#[derive(Debug)]
pub struct Localized<T> {
    template: T,
    locale: LocaleSnapshot,
}

impl<T> Localized<T> {
    pub fn new(template: T, locale: LocaleSnapshot) -> Self {
        Self { template, locale }
    }
}

impl<T: Template> std::fmt::Display for Localized<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.render_into(f).map_err(|_| std::fmt::Error)
    }
}
impl<T: Template> askama::FastWritable for Localized<T> {
    fn write_into(&self, w: &mut dyn std::fmt::Write, values: &dyn askama::Values) -> askama::Result<()> {
        self.render_into_with_values(w, values)
    }
}
impl<T: Template> Template for Localized<T> {
    const SIZE_HINT: usize = T::SIZE_HINT;
    fn render_into_with_values(&self, w: &mut dyn std::fmt::Write, values: &dyn askama::Values) -> askama::Result<()> {
        crate::i18n::render_scope(self.locale.clone(), || self.template.render_into_with_values(w, values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbit_messages::{Direction, MessageRef, Messages};
    use std::sync::Arc;

    #[derive(Debug)]
    struct Label(&'static str);
    impl Messages for Label {
        fn format(&self, _: &MessageRef<'_>) -> String {
            self.0.to_owned()
        }
    }
    fn locale(label: &'static str) -> LocaleSnapshot {
        LocaleSnapshot::new("fr".to_owned(), Direction::Ltr, 9, Arc::new(Label(label)))
    }

    #[derive(Template)]
    #[template(source = "<p>{{ crate::i18n::text(\"core\", \"browser-copied\") }}</p>", ext = "html")]
    struct Fragment;

    struct StreamingText<'a>(&'a std::cell::Cell<bool>);
    impl std::fmt::Display for StreamingText<'_> {
        fn fmt(&self, writer: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            assert_eq!(crate::i18n::text("core", "browser-copied"), "Captured");
            writer.write_str("prefix")?;
            self.0.set(true);
            writer.write_str("tail")
        }
    }

    #[derive(Template)]
    #[template(source = "{{ text }}", ext = "html")]
    struct StreamingFragment<'a> {
        text: StreamingText<'a>,
    }

    struct BoundedWriter {
        bytes: usize,
        limit: usize,
    }
    impl std::fmt::Write for BoundedWriter {
        fn write_str(&mut self, value: &str) -> std::fmt::Result {
            if value.len() > self.limit.saturating_sub(self.bytes) {
                return Err(std::fmt::Error);
            }
            self.bytes += value.len();
            Ok(())
        }
    }

    #[test]
    fn writer_failure_stops_template_work_and_restores_the_ambient_locale() {
        let reached_tail = std::cell::Cell::new(false);
        let template = Localized::new(StreamingFragment { text: StreamingText(&reached_tail) }, locale("Captured"));
        let mut writer = BoundedWriter { bytes: 0, limit: 3 };
        crate::i18n::render_scope(locale("Ambient"), || {
            assert!(matches!(template.render_into(&mut writer), Err(askama::Error::Fmt)));
            assert_eq!(crate::i18n::text("core", "browser-copied"), "Ambient");
        });
        // A wrapper that rendered into an intermediate String would reach the tail
        // before discovering the caller's capacity limit.
        assert!(!reached_tail.get());
        assert_eq!(writer.bytes, 0);
        assert_eq!(crate::i18n::current_snapshot().lang(), "en");
    }

    #[derive(Template)]
    #[template(
        source = "{{ askama::get_value::<String>(\"label\")? }}:{{ crate::i18n::text(\"core\", \"browser-copied\") }}",
        ext = "html"
    )]
    struct RuntimeValues;

    #[test]
    fn runtime_values_are_forwarded_and_their_errors_restore_the_ambient_locale() {
        let template = Localized::new(RuntimeValues, locale("Captured"));
        let label = "<b>".to_owned();
        let values = ("label", &label as &dyn std::any::Any);
        crate::i18n::render_scope(locale("Ambient"), || {
            assert_eq!(template.render_with_values(&values).unwrap(), "&#60;b&#62;:Captured");
            assert!(matches!(template.render(), Err(askama::Error::ValueMissing)));
            assert_eq!(crate::i18n::text("core", "browser-copied"), "Ambient");
        });
        assert_eq!(crate::i18n::current_snapshot().lang(), "en");
    }

    #[tokio::test]
    async fn detached_template_uses_captured_locale_escapes_text_and_restores_ambient_scope() {
        let template =
            crate::i18n::scope(locale("Captured <script>"), async { Localized::new(Fragment, crate::i18n::current_snapshot()) }).await;
        let rendered = tokio::spawn(async move {
            // Tokio does not inherit request-local catalog state into a new task.
            assert_eq!(crate::i18n::current_snapshot().lang(), "en");
            crate::i18n::scope(locale("Different"), async {
                let html = template.render().unwrap();
                assert_eq!(crate::i18n::text("core", "browser-copied"), "Different");
                html
            })
            .await
        })
        .await
        .unwrap();
        assert!(rendered.contains("Captured"));
        assert!(rendered.contains("&#60;script&#62;"));
        assert!(!rendered.contains("<script>"));
        assert_eq!(crate::i18n::current_snapshot().lang(), "en");
    }
}
