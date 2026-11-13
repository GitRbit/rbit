use rbit_wikis::{
    MAX_WIKI_CONTENT_BYTES, WikiContent, WikiContentError, WikiNameError, WikiPath, WikiStoreId, WikiTitle, reading::WikiHistoryLimit,
};

#[test]
fn content_is_bounded_text_and_debug_never_contains_private_source() {
    let text = WikiContent::try_from("private sentinel <script>text</script>".to_owned()).unwrap();
    assert!(!format!("{text:?}").contains("private sentinel"));
    assert!(WikiContent::try_from("x".repeat(MAX_WIKI_CONTENT_BYTES)).is_ok());
    assert_eq!(WikiContent::try_from("x".repeat(MAX_WIKI_CONTENT_BYTES + 1)), Err(WikiContentError::TooLarge));
    assert_eq!(WikiContent::try_from("nul\0text".to_owned()), Err(WikiContentError::Nul));
}

#[test]
fn persisted_names_must_already_be_canonical_instead_of_silently_changing_keys() {
    assert_eq!(WikiPath::from_canonical("Cafe\u{301}".into()), Err(WikiNameError::NonCanonical));
    assert_eq!(WikiTitle::from_canonical(" Title ".into()), Err(WikiNameError::NonCanonical));
    assert_eq!(WikiTitle::from_canonical("Cafe\u{301}".into()), Err(WikiNameError::NonCanonical));
    assert!(WikiPath::from_canonical("Guide".into()).is_ok());
    assert!(WikiTitle::from_canonical("Title".into()).is_ok());
}

#[test]
fn unicode17_catalog_names_keep_their_bytes_and_distinct_identities() {
    // U+1ADE is unassigned in Unicode 17, but becomes a class-230 combining
    // mark in Unicode 18, which would reorder these two marks.
    let first = "a\u{1ade}\u{05ae}b";
    let second = "a\u{05ae}\u{1ade}b";
    for value in [first, second] {
        let title = WikiTitle::try_from(value.to_owned()).unwrap();
        assert_eq!(title.as_str().as_bytes(), value.as_bytes());
        assert_eq!(WikiTitle::from_canonical(value.to_owned()).unwrap(), title);
    }
    assert_ne!(WikiTitle::from_canonical(first.into()).unwrap(), WikiTitle::from_canonical(second.into()).unwrap());
    let canonical = "Caf\u{00e9}/Guide";
    assert_eq!("Cafe\u{0301}/Guide".parse::<WikiPath>().unwrap().as_str(), canonical);
    assert_eq!(WikiPath::from_canonical(canonical.into()).unwrap().as_str().as_bytes(), canonical.as_bytes());
    assert_eq!(WikiPath::from_canonical("a\u{1ade}".into()), Err(WikiNameError::InvalidCharacter));
    assert_eq!("a\u{1ade}".parse::<WikiPath>(), Err(WikiNameError::InvalidCharacter));
}

#[test]
fn history_and_catalog_binding_have_checked_finite_representations() {
    assert!(WikiHistoryLimit::new(0).is_none());
    assert!(WikiHistoryLimit::new(51).is_none());
    assert_eq!(WikiHistoryLimit::new(50).unwrap().get(), 50);
    assert!(WikiStoreId::try_from([0; 16]).is_err());
    assert_eq!(WikiStoreId::try_from([1; 16]).unwrap().as_bytes(), &[1; 16]);
}
