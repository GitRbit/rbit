//! Independent primitive/wire oracles for the shared thread model closure.

use rbit_collaboration::threads::{IssueId, IssueKind, IssueNumber, IssueText, IssueTextError, IssueTitle, MAX_TEXT_BYTES, StateReason};

#[test]
fn canonical_numbers_ids_and_enum_serialization_preserve_existing_contracts() {
    for text in ["", "0", "01", "+1", "-1", " 1", "#1", "4294967296", "１２"] {
        assert!(IssueNumber::parse(text).is_err(), "{text}");
    }
    for value in [1, 25, u32::MAX] {
        let number = IssueNumber::new(value).unwrap();
        assert_eq!(IssueNumber::parse(&value.to_string()).unwrap(), number);
        assert_eq!(number.to_string(), format!("#{value}"));
        assert_eq!(serde_json::to_value(number).unwrap(), serde_json::json!(value));
        assert_eq!(serde_json::from_value::<IssueNumber>(serde_json::json!(value)).unwrap(), number);
    }
    assert!(serde_json::from_value::<IssueNumber>(serde_json::json!(0)).is_err());
    for value in [i64::MIN, -1, 0, i64::MAX] {
        assert_eq!(serde_json::from_value::<IssueId>(serde_json::json!(value)).unwrap(), IssueId(value));
    }
    for kind in IssueKind::ALL {
        assert_eq!(serde_json::to_value(kind).unwrap(), serde_json::json!(kind.as_str()));
    }
    for reason in StateReason::ALL {
        assert_eq!(serde_json::to_value(reason).unwrap(), serde_json::json!(reason.as_str()));
    }
    assert_eq!(IssueKind::ALL, &[IssueKind::Issue, IssueKind::Pull]);
    assert_eq!(StateReason::ALL, &[StateReason::Completed, StateReason::NotPlanned, StateReason::Merged]);
}

#[test]
fn unicode_title_body_normalization_and_error_traits_match_independent_expected_values() {
    assert_eq!(IssueTitle::parse(&format!(" {} ", "雪".repeat(256))).unwrap().as_str(), "雪".repeat(256));
    assert_eq!(IssueTitle::parse(&"雪".repeat(257)).unwrap_err().to_string(), "title must be at most 256 characters");
    assert_eq!(IssueTitle::parse("a\nb").unwrap_err(), IssueTextError::ControlCharacter { kind: "title" });
    assert_eq!(IssueText::body("a\r\nb\rc").unwrap().as_str(), "a\nb\nc");
    assert_eq!(IssueText::body(&"\r\n".repeat(MAX_TEXT_BYTES)).unwrap().as_str().len(), MAX_TEXT_BYTES);
    assert_eq!(IssueText::body(&"é".repeat(MAX_TEXT_BYTES / 2 + 1)).unwrap_err().to_string(), "description must be at most 65536 bytes");
    assert_eq!(IssueText::comment(" \n\t").unwrap_err().to_string(), "comment must not be empty");
    let error = IssueTextError::Empty { kind: "title" };
    assert!(std::error::Error::source(&error).is_none());
    assert_eq!(error.to_string(), "title must not be empty");
}
