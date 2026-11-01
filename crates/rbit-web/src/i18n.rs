//! Shared request-scoped presentation messages.
//! The implementation and its single locale context live in `rbit-ui`.

pub use rbit_ui::i18n::*;

#[cfg(test)]
mod tests {
    use askama::Template;
    use rbit_messages::{Direction, LocaleSnapshot, MessageRef, Messages};
    use std::sync::Arc;

    #[derive(Debug)]
    struct Label;
    impl Messages for Label {
        fn format(&self, _: &MessageRef<'_>) -> String {
            "Shared <catalog> & text".to_owned()
        }
    }

    #[derive(Template)]
    #[template(
        source = "{{ crate::i18n::text(\"core\", \"browser-copied\") }}|{{ rbit_ui::i18n::text(\"core\", \"browser-copied\") }}",
        ext = "html"
    )]
    struct SharedContext;

    #[tokio::test]
    async fn host_and_shared_templates_use_one_locale_context_and_escape_identically() {
        let snapshot = LocaleSnapshot::new("fr".into(), Direction::Ltr, 7, Arc::new(Label));
        super::scope(snapshot.clone(), async {
            assert_eq!(rbit_ui::i18n::current_snapshot().lang(), "fr");
            let rendered = SharedContext.render().unwrap();
            let (host, shared) = rendered.split_once('|').unwrap();
            assert_eq!(host, shared);
            assert!(host.contains("&#60;catalog&#62;"));
            assert!(!host.contains("<catalog>"));
        })
        .await;
        rbit_ui::i18n::scope(snapshot, async {
            assert_eq!(super::current_snapshot().lang(), "fr");
        })
        .await;
        assert_eq!(super::current_snapshot().lang(), "en");
    }
}
