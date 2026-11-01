use super::*;
use std::fmt;
include!("names.rs");

// Independent frozen old macro, not an include of the new production template.
#[derive(Template)]
#[template(path = "tests/legacy-icon.html")]
struct LegacyIcon<'a> {
    name: &'a str,
    class: &'a str,
}
#[derive(Template)]
#[template(source = "<span>{{ icon }}</span>", ext = "html")]
struct Nested<'a> {
    icon: Icon<'a>,
}
#[test]
fn every_existing_name_and_unknown_fallback_match_frozen_macro_bytes() {
    assert_eq!(NAMES.len(), 76);
    for name in NAMES.iter().copied().chain(["", "unknown-icon", "Folder", "<svg onload=alert(1)>"]) {
        for class in ["", "size-4 shrink-0", "icon\" onload=\"alert(1)<&"] {
            assert_eq!(
                Icon::new(name, class).render().unwrap(),
                LegacyIcon { name, class }.render().unwrap(),
                "name={name:?}, class={class:?}"
            );
        }
    }
}
#[test]
fn nested_markup_retains_whitespace_accessibility_and_escaped_class() {
    // The selected fixed shape outlives a temporary dynamic name; only class is borrowed.
    let icon = {
        let name = String::from("check");
        Icon::new(&name, "icon\" onload=\"alert(1)<&")
    };
    let output = Nested { icon }.render().unwrap();
    assert_eq!(output, format!("<span>{}</span>", icon.render().unwrap()));
    assert!(output.starts_with("<span>\n<svg "));
    assert!(output.ends_with("</svg>\n</span>"));
    assert!(output.contains("aria-hidden=\"true\""));
    assert!(output.contains("class=\"icon&#34; onload=&#34;alert(1)&#60;&#38;\""));
    assert!(!output.contains(" onload=\""));
    let unknown = Icon::new("unknown-icon", "").render().unwrap();
    assert!(!unknown.contains("<path") && !unknown.contains("<circle"));
}
struct Writer {
    bytes: usize,
    limit: usize,
    rejected: usize,
}
impl fmt::Write for Writer {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if value.len() > self.limit - self.bytes {
            self.rejected = value.len();
            return Err(fmt::Error);
        }
        self.bytes += value.len();
        Ok(())
    }
}
#[test]
fn direct_and_nested_writes_propagate_capacity_failure_and_allow_exact_capacity() {
    for name in NAMES {
        let icon = Icon::new(name, "size-4");
        let mut bounded = Writer { bytes: 0, limit: 8, rejected: 0 };
        assert!(matches!(icon.render_into(&mut bounded), Err(askama::Error::Fmt)));
        assert!(bounded.bytes <= bounded.limit);
        let nested = Nested { icon };
        let length = nested.render().unwrap().len();
        let mut exact = Writer { bytes: 0, limit: length, rejected: 0 };
        nested.render_into(&mut exact).unwrap();
        assert_eq!(exact.bytes, length);
        let mut short = Writer { bytes: 0, limit: length - 1, rejected: 0 };
        assert!(matches!(nested.render_into(&mut short), Err(askama::Error::Fmt)));
        assert!(short.bytes <= short.limit);
    }
}

#[test]
fn nested_icon_reaches_the_bounded_writer_as_a_stream_not_a_completed_html_string() {
    let icon = Icon::new("file", "size-4");
    let rendered_length = icon.render().unwrap().len();
    let mut writer = Writer { bytes: 0, limit: "<span>".len(), rejected: 0 };
    assert!(matches!(Nested { icon }.render_into(&mut writer), Err(askama::Error::Fmt)));
    assert_eq!(writer.bytes, "<span>".len());
    assert!(
        writer.rejected > 0 && writer.rejected < rendered_length,
        "the first child prefix reaches the caller before the complete child markup"
    );
}
