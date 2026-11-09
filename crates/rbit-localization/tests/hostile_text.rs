//! Localization safety (`docs/design/rewrite/security-contracts.md`, group 6): catalog output
//! is plain text that Askama escapes exactly once, in text and in attributes, for every
//! locale, the `en-XA`/`ar-XB` diagnostics included. Arguments carrying markup, quotes,
//! `&`, template delimiters and bidirectional controls are copied verbatim, never parsed.
use std::{fmt::Display, marker::PhantomData};

use askama::{Template, filters::HtmlSafe};
use rbit_localization::{
    ActionsSecretsMessage, Catalog, CollabSentence, Locale, Localized, LocalizedText, MessageFamily, RepoBrowseMessage, ReviewMessage,
    Rich, RichPart, WikiMessage, settings::profile::NotificationsText, shell::home::HomeMessage,
};

/// Markup, both quotes, `&`, an entity, Askama/Jinja delimiters and bidi controls,
/// including a stray pop-directional-isolate that tries to close an isolate early.
const HOSTILE: &str = "<script>alert(\"x\")</script> &amp; 'q' {{ page.site_name }} {% if true %}\u{202E}gpj.exe\u{202C} \u{2069}x";

const LOCALES: [Locale; 11] = [
    Locale::English,
    Locale::Spanish,
    Locale::ChineseSimplified,
    Locale::PortugueseBrazil,
    Locale::Japanese,
    Locale::German,
    Locale::Russian,
    Locale::French,
    Locale::Korean,
    Locale::PseudoExpanded,
    Locale::PseudoRtl,
];

#[derive(Template)]
#[template(source = "<p>{{ value }}</p><a title=\"{{ value }}\" aria-label='{{ value }}'>x</a>", ext = "html")]
struct Boundary<'a, T: Display> {
    value: &'a T,
}

#[derive(Template)]
#[template(source = "<p>{{ value }}</p><a title=\"{{ value }}\" aria-label='{{ value }}'>x</a>", ext = "html")]
struct PlainBoundary<'a> {
    value: &'a str,
}

/// Renders a rich sentence the way the issue and pull request templates do: each slot's
/// own markup, and the text between slots through ordinary interpolation.
#[derive(Template)]
#[template(
    source = "<span{% if rich.fallback() %} lang=\"en\"{% endif %}>{% for part in rich.parts() %}{% if part.is_slot(0) %}<time datetime=\"2026-01-01\">then</time>{% else %}{{ part.text() }}{% endif %}{% endfor %}</span>",
    ext = "html"
)]
struct RichBoundary<'a> {
    rich: &'a Rich,
}

/// `true` exactly when `T` implements Askama's `HtmlSafe`, which turns escaping off.
struct Probe<T: ?Sized>(PhantomData<T>);
trait NotHtmlSafe {
    const SAFE: bool = false;
}
impl<T: ?Sized> NotHtmlSafe for T {}
impl<T: ?Sized + HtmlSafe> Probe<T> {
    const SAFE: bool = true;
}

// Checked at build time: no catalog output type opts out of escaping.
const _: () = {
    assert!(<Probe<u64>>::SAFE, "the probe recognizes Askama's own safe types");
    assert!(!<Probe<Localized>>::SAFE);
    assert!(!<Probe<LocalizedText>>::SAFE);
    assert!(!<Probe<str>>::SAFE, "rich parts are rendered through `RichPart::text`, a plain `&str`");
    assert!(!<Probe<String>>::SAFE);
};

/// Asserts that `value` renders exactly like its plain string through Askama, so it is
/// escaped once, and that the hostile argument reached the text verbatim.
fn assert_escaped_once(value: &(impl Display + ?Sized), raw: &str, locale: Locale) {
    assert!(raw.contains("<script>alert(\"x\")</script> &amp; 'q' {{ page.site_name }} {% if true %}"), "{locale:?}: {raw}");
    assert!(raw.contains("\u{202E}gpj.exe\u{202C}"), "{locale:?}: bidi controls are kept as text");
    let rendered = Boundary { value: &value }.render().unwrap();
    assert_eq!(rendered, PlainBoundary { value: raw }.render().unwrap(), "{locale:?}");
    assert!(!rendered.contains("<script"), "{locale:?}: {rendered}");
    assert!(!rendered.contains("\"x\"") && !rendered.contains("'q'"), "{locale:?}: quotes are escaped in attributes");
    assert!(rendered.contains("&#60;script&#62;alert(&#34;x&#34;)&#60;/script&#62; &#38;amp; &#39;q&#39;"), "{locale:?}: {rendered}");
    assert!(!rendered.contains("&#38;#"), "{locale:?}: escaped twice: {rendered}");
    assert!(rendered.contains("{{ page.site_name }} {% if true %}"), "{locale:?}: delimiters stay literal text");
}

fn plain_samples(locale: Locale) -> Vec<Localized> {
    let catalog = Catalog::new(locale);
    vec![
        ReviewMessage::RemoveRequestFor { name: HOSTILE }.localized(locale),
        catalog.localized(RepoBrowseMessage::ForkTitle { owner: HOSTILE, name: "repo" }),
        catalog.localized(WikiMessage::SearchCount { count: 2, query: HOSTILE }),
        catalog.localized(ActionsSecretsMessage::NewValueOf { name: HOSTILE }),
        catalog.localized(HomeMessage::SearchIn(HOSTILE)),
        NotificationsText::DefaultHour(HOSTILE).text(&locale),
    ]
}

#[test]
fn family_text_with_hostile_arguments_is_escaped_exactly_once_in_every_locale() {
    for locale in LOCALES {
        for message in plain_samples(locale) {
            assert_escaped_once(&message, message.as_str(), locale);
            let marked = message.marked_lang();
            assert!(marked.is_none_or(|lang| lang == "en" || lang == locale.tag()), "{locale:?}: lang is a static tag");
        }
    }
}

#[test]
fn rich_sentences_escape_their_text_and_keep_slot_markup_their_own() {
    for locale in LOCALES {
        let rich = CollabSentence::RowOpened { number: HOSTILE, author: HOSTILE }.rich(locale);
        let slots: Vec<_> = rich.parts().iter().filter_map(RichPart::slot).collect();
        assert_eq!(slots, [0], "{locale:?}: an argument never becomes a slot");
        for part in rich.parts().iter().filter(|part| part.slot().is_none()) {
            if part.text().contains("script") {
                assert_escaped_once(part.text(), part.text(), locale);
            }
        }
        let rendered = RichBoundary { rich: &rich }.render().unwrap();
        assert_eq!(rendered.matches("<time").count(), 1, "{locale:?}: {rendered}");
        assert_eq!(rendered.matches('<').count(), 4, "{locale:?}: only the template's own tags: {rendered}");
        assert!(!rendered.contains("&#38;#"), "{locale:?}: {rendered}");
        let plain = rich.plain(&["then"]);
        assert_eq!(plain.matches(HOSTILE.split('\u{2069}').next().unwrap()).count(), 2, "{locale:?}: both arguments verbatim");
    }
}

#[test]
fn diagnostics_expand_wording_but_never_arguments_or_escaping() {
    let english = ReviewMessage::RemoveRequestFor { name: HOSTILE }.localized(Locale::English);
    let expanded = ReviewMessage::RemoveRequestFor { name: HOSTILE }.localized(Locale::PseudoExpanded);
    assert!(expanded.as_str().starts_with('[') && expanded.as_str().ends_with(']'));
    assert!(expanded.as_str().contains(HOSTILE), "the argument is not accented: {expanded}");
    assert_ne!(expanded.as_str().replace(HOSTILE, ""), english.as_str().replace(HOSTILE, ""));
    let wording = expanded.as_str().replace(HOSTILE, "");
    assert!(!wording.contains(['<', '>', '&', '"', '\'']), "expansion introduces no markup characters: {wording}");

    let rtl = ReviewMessage::RemoveRequestFor { name: HOSTILE }.localized(Locale::PseudoRtl);
    assert!(rtl.as_str().contains(&format!("\u{2068}{HOSTILE}\u{2069}")), "the argument is isolated as given: {rtl}");
    // The stray PDI inside the argument still closes the isolate early: isolation is a
    // layout diagnostic, not a defence against misleading identifiers (see the contract).
    assert_eq!(rtl.as_str().matches('\u{2069}').count(), 2);
}

#[test]
fn every_family_writes_through_the_same_verbatim_argument_path() {
    // Families differ only in wording: each writes arguments through `MessageWriter::argument`
    // or `RichWriter::argument`, which copy (or, in `ar-XB`, isolate) the value. A family
    // written by a test proves the path for any family, untranslated or translated.
    #[derive(Clone, Copy)]
    struct Echo<'a>(&'a str);
    impl MessageFamily for Echo<'_> {
        fn id(self) -> &'static str {
            "test.echo"
        }
        fn english(self, text: &mut rbit_localization::MessageWriter) {
            text.literal("Echo: ");
            text.argument(self.0);
        }
    }
    for locale in LOCALES {
        let echoed = Catalog::new(locale).localized(Echo(HOSTILE));
        assert_escaped_once(&echoed, echoed.as_str(), locale);
        assert_eq!(echoed.fallback(), !matches!(locale, Locale::English | Locale::PseudoExpanded | Locale::PseudoRtl));
    }
}
