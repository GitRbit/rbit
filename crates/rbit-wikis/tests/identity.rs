use rbit_wikis::{WikiGeneration, WikiObjectFormat, WikiPageId, WikiRevisionId};

#[test]
fn page_identity_and_generation_have_explicit_positive_bounds() {
    for invalid in [i64::MIN, -1, 0] {
        assert_eq!(WikiPageId::new(invalid), None);
        assert_eq!(WikiGeneration::new(invalid), None);
    }
    assert_eq!(WikiPageId::new(9).unwrap().get(), 9);
    assert_eq!(WikiGeneration::INITIAL.get(), 1);
    assert_eq!(WikiGeneration::INITIAL.checked_next().unwrap().get(), 2);
    assert!(WikiGeneration::new(i64::MAX).unwrap().checked_next().is_none());
}

#[test]
fn full_revision_roundtrips_both_algorithms_without_truncation_or_null_identity() {
    for (length, algorithm, bytes) in [(40, WikiObjectFormat::Sha1, 20), (64, WikiObjectFormat::Sha256, 32)] {
        let text = "a1".repeat(length / 2);
        let revision: WikiRevisionId = text.parse().unwrap();
        assert_eq!(revision.format(), algorithm);
        assert_eq!(revision.as_bytes().len(), bytes);
        assert_eq!(revision.to_string(), text);
        assert!("0".repeat(length).parse::<WikiRevisionId>().is_err());
    }
    assert_ne!("a".repeat(40).parse::<WikiRevisionId>().unwrap(), "a".repeat(64).parse::<WikiRevisionId>().unwrap());
}

#[test]
fn revisions_reject_git_expressions_uppercase_whitespace_and_non_hex_unicode() {
    for value in ["HEAD", "main~1", "--help", "deadbee", "", "a^{commit}", "refs/heads/main", "@{0}"] {
        assert!(value.parse::<WikiRevisionId>().is_err());
    }
    for value in ["A".repeat(40), "a".repeat(39), "a".repeat(41), format!(" {}", "a".repeat(40)), "\u{ff41}".repeat(40)] {
        assert!(value.parse::<WikiRevisionId>().is_err());
    }
}
