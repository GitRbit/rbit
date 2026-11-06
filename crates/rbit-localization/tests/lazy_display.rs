//! Independent owned-text oracle protects every existing catalog transformation.
use askama::Template;
use rbit_localization::{Catalog, Locale, LocalizedDisplay, Message};
use std::fmt::Write;
#[derive(Template)]
#[template(source = "<p>{{ value }}</p><i title=\"{{ value }}\"></i>", ext = "html")]
struct Lazy<'a> {
    value: LocalizedDisplay<'a>,
}
#[derive(Template)]
#[template(source = "<p>{{ value }}</p><i title=\"{{ value }}\"></i>", ext = "html")]
struct Owned<'a> {
    value: &'a str,
}
fn messages(site: &str) -> Vec<Message<'_>> {
    let mut messages =
        vec![Message::ExploreSubtitle { site }, Message::SearchLabel, Message::CountForQuery { count: u64::MAX, query: site }];
    messages.extend([i64::MIN, -1, 0, 1, 2, i64::MAX].map(|count| Message::Stars { count }));
    messages.extend([(0, 0), (1, 1), (1, 2), (u32::MAX, u32::MAX)].map(|(page, pages)| Message::PageOf { page, pages }));
    messages
}
#[test]
fn lazy_output_is_byte_equivalent_to_owned_text_including_escaping_and_fallback() {
    for locale in [Locale::English, Locale::PseudoExpanded, Locale::PseudoRtl] {
        for site in ["", "Example Forge", "雪 café🦀 \u{2066}עברית\u{2069}", "<script>bad</script>&\"' </title>\n"] {
            let catalog = Catalog::new(locale);
            for message in messages(site) {
                let owned = catalog.text(message);
                let lazy = catalog.display(message);
                assert_eq!(lazy.to_string(), owned.as_str(), "{locale:?} {}", message.id());
                assert_eq!(Lazy { value: lazy }.render().unwrap(), Owned { value: owned.as_str() }.render().unwrap());
            }
        }
    }
}
/// The destination need not be a heap String. This exercises ordinary `fmt::Write`
/// without unsafe/global allocator instrumentation or claims about measured speed.
struct Fixed {
    bytes: [u8; 256],
    used: usize,
}
impl std::fmt::Write for Fixed {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let end = self.used.checked_add(value.len()).filter(|&end| end <= self.bytes.len()).ok_or(std::fmt::Error)?;
        self.bytes[self.used..end].copy_from_slice(value.as_bytes());
        self.used = end;
        Ok(())
    }
}
#[test]
fn english_dynamic_labels_write_into_a_caller_owned_fixed_destination() {
    let catalog = Catalog::new(Locale::English);
    for message in [
        Message::ExploreSubtitle { site: "<untrusted>&雪" },
        Message::Stars { count: i64::MAX },
        Message::PageOf { page: u32::MAX, pages: u32::MAX },
    ] {
        let mut output = Fixed { bytes: [0; 256], used: 0 };
        write!(&mut output, "{}", catalog.display(message)).unwrap();
        assert_eq!(&output.bytes[..output.used], catalog.text(message).as_str().as_bytes());
    }
}

#[test]
fn lazy_messages_propagate_destination_failure() {
    struct Refuse;
    impl std::fmt::Write for Refuse {
        fn write_str(&mut self, _: &str) -> std::fmt::Result {
            Err(std::fmt::Error)
        }
    }
    for locale in [Locale::English, Locale::PseudoExpanded, Locale::PseudoRtl] {
        let message = Catalog::new(locale).display(Message::ExploreSubtitle { site: "untrusted" });
        assert!(write!(&mut Refuse, "{message}").is_err());
    }
}
