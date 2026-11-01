use super::*;
use askama::{FastWritable, Values, filters::HtmlSafe};
use rbit_messages::{Direction, LocaleSnapshot, MessageRef, Messages};
use rbit_ui::template::Localized;
use std::{cell::Cell, fmt, sync::Arc};

#[derive(Template)]
#[template(source = "<main>{{ content }}</main>", ext = "html")]
struct HostShell<T: Template + HtmlSafe> {
    content: T,
}
fn content() -> SummaryContent {
    SummaryContent {
        heading: "<job> step 1".into(),
        run_url: "/runs/1?value=\"bad\"".into(),
        html: None,
        source: "<script>untrusted()</script>\n& source tail".into(),
        previous: Some("/previous?x=\"bad\"".into()),
        next: Some("/next".into()),
    }
}
#[test]
fn host_composition_escapes_source_heading_and_links_without_escaping_compiled_markup_twice() {
    let output = HostShell { content: content() }.render().unwrap();
    assert!(output.starts_with("<main><section"));
    assert!(output.contains("data-actions-summary"));
    assert!(output.contains("&#60;script&#62;untrusted()&#60;/script&#62;"));
    assert!(output.contains("&#60;job&#62; step 1"));
    assert!(output.contains("/runs/1?value=&#34;bad&#34;"));
    assert!(output.contains("/previous?x=&#34;bad&#34;"));
    assert!(!output.contains("<script>"));
    assert!(!output.contains("&#38;#60;script"));
}
#[test]
fn trusted_host_sanitizer_output_is_preserved_while_fallback_source_is_not_rendered() {
    let mut content = content();
    content.html = Some("<p>already sanitized <strong>summary</strong></p>".into());
    let output = HostShell { content }.render().unwrap();
    assert!(output.contains("<p>already sanitized <strong>summary</strong></p>"));
    assert!(!output.contains("source tail"));
    assert!(!output.contains("<script>"));
}
struct Writer {
    used: usize,
    limit: usize,
}
impl fmt::Write for Writer {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if value.len() > self.limit - self.used {
            return Err(fmt::Error);
        }
        self.used += value.len();
        Ok(())
    }
}
struct Probe<'a>(&'a Cell<bool>);
impl fmt::Display for Probe<'_> {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        panic!("nested template must use FastWritable directly");
    }
}
impl HtmlSafe for Probe<'_> {}
impl FastWritable for Probe<'_> {
    fn write_into(&self, writer: &mut dyn fmt::Write, values: &dyn Values) -> askama::Result<()> {
        self.render_into_with_values(writer, values)
    }
}
impl Template for Probe<'_> {
    const SIZE_HINT: usize = 16;
    fn render_into_with_values(&self, writer: &mut dyn fmt::Write, _: &dyn Values) -> askama::Result<()> {
        writer.write_str("prefix")?;
        self.0.set(true);
        writer.write_str("tail")?;
        Ok(())
    }
}
#[test]
fn nesting_uses_fast_writer_and_propagates_outer_failure_before_rendering_the_tail() {
    let reached_tail = Cell::new(false);
    let template = HostShell { content: Probe(&reached_tail) };
    let mut writer = Writer { used: 0, limit: 6 };
    assert!(matches!(template.render_into(&mut writer), Err(askama::Error::Fmt)));
    assert!(!reached_tail.get(), "an intermediary render String would have reached the tail");
    assert_eq!(writer.used, 6);
    let mut actual = Writer { used: 0, limit: 128 };
    assert!(matches!(HostShell { content: content() }.render_into(&mut actual), Err(askama::Error::Fmt)));
    assert!(actual.used <= actual.limit);
    // The real document bridge imposes a 2 MiB outer cap. Even oversized
    // trusted host output must stop at that writer rather than bypassing it.
    let limit = 2 * 1024 * 1024;
    let mut oversized = content();
    oversized.html = Some("x".repeat(limit));
    let mut outer = Writer { used: 0, limit };
    assert!(matches!(HostShell { content: oversized }.render_into(&mut outer), Err(askama::Error::Fmt)));
    assert!(outer.used <= limit);
}
#[derive(Debug)]
struct Label(&'static str);
impl Messages for Label {
    fn format(&self, _: &MessageRef<'_>) -> String {
        self.0.into()
    }
}
fn locale(label: &'static str) -> LocaleSnapshot {
    LocaleSnapshot::new("fr".into(), Direction::Ltr, 9, Arc::new(Label(label)))
}
#[test]
fn outer_captured_locale_reaches_nested_labels_and_restores_the_ambient_locale() {
    let template = Localized::new(HostShell { content: content() }, locale("Captured <label>"));
    rbit_ui::i18n::render_scope(locale("Ambient"), || {
        let output = template.render().unwrap();
        assert!(output.contains("Captured &#60;label&#62;"));
        assert!(!output.contains("Ambient"));
        assert_eq!(rbit_ui::i18n::text("core", "browser-copied"), "Ambient");
    });
}
