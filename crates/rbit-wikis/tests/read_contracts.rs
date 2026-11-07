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
fn history_and_catalog_binding_have_checked_finite_representations() {
    assert!(WikiHistoryLimit::new(0).is_none());
    assert!(WikiHistoryLimit::new(51).is_none());
    assert_eq!(WikiHistoryLimit::new(50).unwrap().get(), 50);
    assert!(WikiStoreId::try_from([0; 16]).is_err());
    assert_eq!(WikiStoreId::try_from([1; 16]).unwrap().as_bytes(), &[1; 16]);
}
