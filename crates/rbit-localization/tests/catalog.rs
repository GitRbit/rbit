use askama::Template;
use rbit_localization::{Catalog, Locale, LocalizedText, Message, RepositoryVisibility};

#[test]
fn typed_plural_and_select_messages_are_complete() {
    let catalog = Catalog::new(Locale::English);
    for (count, expected) in
        [(0, "0 repositories"), (1, "1 repository"), (2, "2 repositories"), (u64::MAX, "18,446,744,073,709,551,615 repositories")]
    {
        assert_eq!(catalog.text(Message::RepositoryCount { count }).as_str(), expected);
    }
    for (visibility, expected) in
        [(RepositoryVisibility::Public, "Public"), (RepositoryVisibility::Private, "Private"), (RepositoryVisibility::Internal, "Internal")]
    {
        assert_eq!(catalog.text(Message::Visibility { visibility }).as_str(), expected);
    }
    assert_eq!(Message::RepositoryCount { count: 2 }.id(), "repositories.count");
    assert_eq!(catalog.text(Message::PageNumber { page: 3 }).as_str(), "Page 3");
}

#[derive(Template)]
#[template(source = "<p>{{ label }}</p><a aria-label=\"{{ label }}\">link</a>", ext = "html")]
struct TextBoundary<'a> {
    label: &'a LocalizedText,
}

#[derive(Template)]
#[template(source = "<p>{{ label }}</p><a aria-label=\"{{ label }}\">link</a>", ext = "html")]
struct PlainTextBoundary<'a> {
    label: &'a str,
}

#[test]
fn arguments_are_plain_text_and_askama_escapes_them_once_in_text_and_attributes() {
    let value = "<script>\"&'{$count}";
    let label = Catalog::new(Locale::English).text(Message::OwnerRepositories { owner: value });
    assert!(label.as_str().contains(value), "catalog does not pre-escape or parse interpolation");
    let rendered = TextBoundary { label: &label }.render().unwrap();
    assert!(!rendered.contains("<script>"));
    assert!(!rendered.contains("&amp;lt;"));
    assert_eq!(rendered, PlainTextBoundary { label: label.as_str() }.render().unwrap());
    assert!(rendered.contains("{$count}"), "arguments cannot invoke catalog syntax");
}

#[test]
fn pseudo_expands_literals_and_preserves_identifiers() {
    let english = Catalog::new(Locale::English).text(Message::RepositoriesTitle);
    let pseudo = Catalog::new(Locale::PseudoExpanded).text(Message::RepositoriesTitle);
    assert!(pseudo.as_str().chars().count() > english.as_str().chars().count());
    assert!(pseudo.as_str().starts_with('['));
    let identifier = "example/MyRepo-123";
    let text = Catalog::new(Locale::PseudoExpanded).text(Message::OwnerRepositories { owner: identifier });
    assert!(text.as_str().contains(identifier));
}

#[test]
fn rtl_catalog_sets_direction_and_isolates_each_interpolation() {
    let catalog = Catalog::new(Locale::PseudoRtl);
    assert_eq!(catalog.locale().tag(), "ar-XB");
    assert_eq!(catalog.locale().direction().as_str(), "rtl");
    assert_eq!(Locale::English.direction().as_str(), "ltr");
    let text = catalog.text(Message::RepositoryLinkLabel { owner: "alice", name: "project" });
    assert!(text.as_str().contains("\u{2068}alice\u{2069}/\u{2068}project\u{2069}"));
}
