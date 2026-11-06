use super::*;

fn account() -> Account {
    Account {
        id: AccountId(-1),
        kind: AccountKind::User,
        name: "Alice".parse().unwrap(),
        display_name: None,
        bio: Some("profile".into()),
        created_at: Timestamp(-1),
    }
}

#[test]
fn account_fields_and_display_fallback_remain_compatible() {
    let mut value = account();
    assert_eq!(value.label(), "Alice");
    value.display_name = Some(String::new());
    assert_eq!(value.label(), "Alice");
    value.display_name = Some("  ".into());
    assert_eq!(value.label(), "  ", "legacy labels test emptiness, not trimmed emptiness");
    value.display_name = Some("Alice Example".into());
    assert_eq!(value.label(), "Alice Example");
    assert!(!value.is_organization());
    value.kind = AccountKind::Organization;
    assert!(value.is_organization());
    assert_eq!(value.created_at.seconds(), -1);
    let user = User { account: value, email: "alice@example.com".parse().unwrap(), is_admin: true };
    assert_eq!(user.id(), AccountId(-1));
    assert_eq!(user.name().as_str(), "Alice");
    assert!(user.is_admin);
}

#[test]
fn account_kind_keeps_parse_values_and_existing_serde_variant_names() {
    for (value, machine, json) in [(AccountKind::User, "user", "\"User\""), (AccountKind::Organization, "organization", "\"Organization\"")]
    {
        assert_eq!(value.as_str(), machine);
        assert_eq!(AccountKind::parse(machine), Some(value));
        assert_eq!(serde_json::to_string(&value).unwrap(), json);
        assert_eq!(serde_json::from_str::<AccountKind>(json).unwrap(), value);
    }
    for invalid in ["User", "Organization", " user ", "", "team"] {
        assert_eq!(AccountKind::parse(invalid), None);
    }
}
