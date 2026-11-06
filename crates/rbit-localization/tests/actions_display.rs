//! The existing owned catalog is the byte/escaping oracle for deferred run labels.
use askama::Template;
use rbit_localization::{ActionsDisplay, ActionsMessage, Catalog, Locale};
use std::fmt::Write;

#[derive(Template)]
#[template(source = "<span>{{ value }}</span><button title=\"{{ value }}\" aria-label=\"{{ value }}\"></button>", ext = "html")]
struct Lazy<'a> {
    value: ActionsDisplay<'a>,
}
#[derive(Template)]
#[template(source = "<span>{{ value }}</span><button title=\"{{ value }}\" aria-label=\"{{ value }}\"></button>", ext = "html")]
struct Owned<'a> {
    value: &'a str,
}

#[test]
fn dynamic_and_static_labels_match_owned_text_and_escape_once_in_text_and_attributes() {
    for locale in [Locale::English, Locale::PseudoExpanded, Locale::PseudoRtl] {
        let catalog = Catalog::new(locale);
        for name in ["", "build", "雪 café 🦀 \u{2066}עברית\u{2069}", "<script>bad</script>&\"' </title>\n"] {
            for message in [ActionsMessage::RerunJob { name }, ActionsMessage::Summary { job: name }, ActionsMessage::Jobs] {
                let reference = catalog.actions(message);
                let lazy = catalog.actions_display(message);
                assert_eq!(lazy.to_string(), reference.as_str(), "{locale:?} {}", message.id());
                let rendered = Lazy { value: lazy }.render().unwrap();
                assert_eq!(rendered, Owned { value: reference.as_str() }.render().unwrap());
                assert!(!rendered.contains("<script>bad</script>"));
            }
        }
    }
}

#[test]
fn english_keeps_current_rerun_and_summary_wording_independent_of_the_catalog_oracle() {
    let catalog = Catalog::new(Locale::English);
    for name in ["", "build", "雪 <unsafe> & \"quotes\""] {
        assert_eq!(catalog.actions_display(ActionsMessage::RerunJob { name }).to_string(), format!("Re-run {name}"));
        assert_eq!(catalog.actions_display(ActionsMessage::Summary { job: name }).to_string(), format!("{name} summary"));
    }
}

struct Fixed {
    bytes: [u8; 256],
    used: usize,
}
impl std::fmt::Write for Fixed {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let end = self.used.checked_add(value.len()).ok_or(std::fmt::Error)?;
        self.bytes.get_mut(self.used..end).ok_or(std::fmt::Error)?.copy_from_slice(value.as_bytes());
        self.used = end;
        Ok(())
    }
}
struct Refuse;
impl std::fmt::Write for Refuse {
    fn write_str(&mut self, _: &str) -> std::fmt::Result {
        Err(std::fmt::Error)
    }
}

#[test]
fn english_dynamic_display_accepts_a_fixed_destination_and_propagates_failure() {
    let catalog = Catalog::new(Locale::English);
    for message in [ActionsMessage::RerunJob { name: "<unsafe>雪" }, ActionsMessage::Summary { job: "<unsafe>雪" }] {
        let mut destination = Fixed { bytes: [0; 256], used: 0 };
        write!(&mut destination, "{}", catalog.actions_display(message)).unwrap();
        assert_eq!(&destination.bytes[..destination.used], catalog.actions(message).as_str().as_bytes());
    }
    for locale in [Locale::English, Locale::PseudoExpanded, Locale::PseudoRtl] {
        assert!(write!(&mut Refuse, "{}", Catalog::new(locale).actions_display(ActionsMessage::Summary { job: "job" })).is_err());
    }
}
