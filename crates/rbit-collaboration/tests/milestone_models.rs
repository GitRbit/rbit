//! Independent wire, Unicode/byte and calendar boundaries of the milestone models.

use rbit_collaboration::{
    IssueState,
    milestones::{MilestoneId, MilestoneInput, is_calendar_day, milestone_fields},
};

#[test]
fn shared_state_and_ids_keep_original_wire_order_identifiers_and_parse_strictness() {
    assert_eq!(IssueState::ALL, &[IssueState::Open, IssueState::Closed]);
    for (state, text) in [(IssueState::Open, "open"), (IssueState::Closed, "closed")] {
        assert_eq!(state.as_str(), text);
        assert_eq!(state.to_string(), text);
        assert_eq!(IssueState::parse(text), Some(state));
        assert_eq!(serde_json::to_value(state).unwrap(), serde_json::json!(text));
        assert_eq!(serde_json::from_value::<IssueState>(serde_json::json!(text)).unwrap(), state);
    }
    for value in ["", "Open", "CLOSED", " open", "closed ", "merged"] {
        assert_eq!(IssueState::parse(value), None);
        assert!(serde_json::from_value::<IssueState>(serde_json::json!(value)).is_err());
    }
    for value in [i64::MIN, -1, 0, i64::MAX] {
        assert_eq!(serde_json::to_value(MilestoneId(value)).unwrap(), serde_json::json!(value));
        assert_eq!(serde_json::from_value::<MilestoneId>(serde_json::json!(value)).unwrap(), MilestoneId(value));
    }
}

#[test]
fn calendar_oracle_covers_century_leaps_month_ends_and_noncanonical_input() {
    for (year, leap) in [(1600, true), (1700, false), (1900, false), (2000, true), (2024, true), (2100, false), (2400, true)] {
        let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        for (month, maximum) in (1..=12).zip(days) {
            for day in 1..=31 {
                assert_eq!(is_calendar_day(&format!("{year:04}-{month:02}-{day:02}")), day <= maximum);
            }
        }
    }
    for value in
        ["0000-01-01", "2024-00-01", "2024-13-01", "2024-01-00", "2024-1-01", "2024-01-1", "２０２４-01-01", "2024/01/01", " 2024-01-01"]
    {
        assert!(!is_calendar_day(value), "{value}");
    }
}

#[test]
fn titles_count_scalars_descriptions_count_normalized_bytes_and_errors_keep_precedence() {
    let mut input =
        MilestoneInput { title: format!(" {} ", "雪".repeat(100)), description: "a\r\nb\rc\0".into(), due_on: " 2000-02-29 ".into() };
    let fields = milestone_fields(&input).unwrap();
    assert_eq!(fields.title, "雪".repeat(100));
    assert_eq!(fields.description, "a\nb\rc\0", "description controls and bare CR retain existing acceptance");
    assert_eq!(fields.due_on.as_deref(), Some("2000-02-29"));
    input.title = "a\nb".into();
    input.description = "x".repeat(10001);
    input.due_on = "tomorrow".into();
    assert_eq!(milestone_fields(&input).unwrap_err(), "milestone title must not contain control characters");
    input.title = "valid".into();
    assert_eq!(milestone_fields(&input).unwrap_err(), "milestone description must be at most 10000 bytes");
    input.description = "é".repeat(5000);
    assert_eq!(milestone_fields(&input).unwrap_err(), "due date must be a day written YYYY-MM-DD");
    input.due_on = " \t ".into();
    assert!(milestone_fields(&input).unwrap().due_on.is_none());
    input.description = "\r\n".repeat(10000);
    assert_eq!(milestone_fields(&input).unwrap().description.len(), 10000);
    input.title = "雪".repeat(101);
    assert_eq!(milestone_fields(&input).unwrap_err(), "milestone title must be at most 100 characters");
}
