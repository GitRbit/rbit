//! Wire and validation edges remain compatible with the legacy label contracts.

use rbit_collaboration::labels::{LabelId, LabelInput, LabelSnapshot, label_fields, normalise_color};

#[test]
fn ids_and_snapshots_retain_unconstrained_integer_and_named_json_contracts() {
    for value in [i64::MIN, -1, 0, i64::MAX] {
        assert_eq!(serde_json::to_value(LabelId(value)).unwrap(), serde_json::json!(value));
        assert_eq!(serde_json::from_value::<LabelId>(serde_json::json!(value)).unwrap(), LabelId(value));
    }
    let snapshot = LabelSnapshot { id: LabelId(-1), name: "雪".into(), color: "#aabbcc".into() };
    let json = serde_json::json!({"id":-1,"name":"雪","color":"#aabbcc"});
    assert_eq!(serde_json::to_value(&snapshot).unwrap(), json);
    assert_eq!(serde_json::from_value::<LabelSnapshot>(json).unwrap(), snapshot);
}

#[test]
fn unicode_limits_trimming_control_rejection_and_colour_error_order_stay_exact() {
    let mut input = LabelInput { name: format!(" {} ", "雪".repeat(50)), color: " AABBCC ".into(), description: "語".repeat(100) };
    let fields = label_fields(&input).unwrap();
    assert_eq!(fields.name, "雪".repeat(50));
    assert_eq!(fields.color, "#aabbcc");
    input.name.push('雪');
    assert_eq!(label_fields(&input).unwrap_err(), "label name must be at most 50 characters");
    input.name = "valid".into();
    input.description.push('\n');
    assert!(label_fields(&input).is_ok(), "outer whitespace is trimmed before control validation");
    input.description = "a\nb".into();
    input.color = "invalid".into();
    assert_eq!(label_fields(&input).unwrap_err(), "label description must not contain control characters");
    assert_eq!(normalise_color(" #AbCdEf ").unwrap(), "#abcdef");
    assert!(normalise_color("#ａａｂｂｃｃ").is_err());
}
