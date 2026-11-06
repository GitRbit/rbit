use super::{NameError, validate_slug};

#[test]
fn validation_preserves_error_priority_and_original_reserved_value() {
    const KIND: &str = "username";
    for (value, max, reserved, expected) in [
        ("", 0, &[][..], NameError::Empty { kind: KIND }),
        ("a/b", 2, &[][..], NameError::TooLong { kind: KIND, max: 2 }),
        (".é", 2, &[][..], NameError::TooLong { kind: KIND, max: 2 }),
        (".é", 3, &[][..], NameError::InvalidCharacter { kind: KIND }),
        ("../", 3, &[][..], NameError::InvalidCharacter { kind: KIND }),
        (".hidden", 39, &[".hidden"][..], NameError::InvalidBoundary { kind: KIND }),
        ("Settings", 39, &["settings"][..], NameError::Reserved { kind: KIND, value: "Settings".into() }),
    ] {
        assert_eq!(validate_slug(KIND, value, max, reserved), Err(expected));
    }
}

#[test]
fn grammar_keeps_case_boundaries_traversal_and_git_suffix_rules() {
    for accepted in ["A", "a-B_c.d", "project.gitx", "x.locked", "x.y", "Z9"] {
        assert_eq!(validate_slug("repository name", accepted, 100, &[]), Ok(()));
    }
    for rejected in ["x..y", "a.GIT", "Project.LoCk"] {
        assert_eq!(
            validate_slug("repository name", rejected, 100, &[]),
            Err(NameError::Reserved { kind: "repository name", value: rejected.into() })
        );
    }
    for rejected in ["has space", "a/b", "a\\b", "a:b", "a\0b", "é", "雪"] {
        assert_eq!(validate_slug("repository name", rejected, 100, &[]), Err(NameError::InvalidCharacter { kind: "repository name" }));
    }
    for rejected in [".hidden", "trailing-", "_leading", "a."] {
        assert_eq!(validate_slug("repository name", rejected, 100, &[]), Err(NameError::InvalidBoundary { kind: "repository name" }));
    }
    assert!(validate_slug("username", &"a".repeat(39), 39, &[]).is_ok());
    assert_eq!(validate_slug("username", &"a".repeat(40), 39, &[]), Err(NameError::TooLong { kind: "username", max: 39 }));
}

#[test]
fn reserved_routes_belong_to_the_calling_feature() {
    assert!(validate_slug("username", "settings", 39, &["settings"]).is_err());
    assert!(validate_slug("repository name", "settings", 100, &[]).is_ok());
    assert!(validate_slug("username", " Settings ", 39, &["settings"]).is_err(), "shared grammar never trims input");
}

#[test]
fn public_error_messages_keep_every_existing_variant_and_field() {
    for (error, expected) in [
        (NameError::Empty { kind: "username" }, "username must not be empty"),
        (NameError::TooLong { kind: "repository name", max: 100 }, "repository name must be at most 100 characters"),
        (NameError::InvalidCharacter { kind: "team name" }, "team name may only contain letters, digits, '-', '_' and '.'"),
        (NameError::InvalidBoundary { kind: "username" }, "username must start and end with a letter or digit"),
        (NameError::Reserved { kind: "username", value: "Settings".into() }, "\"Settings\" is reserved and cannot be used as a username"),
        (NameError::InvalidEmail, "that does not look like a valid email address"),
        (NameError::NotANumber { kind: "issue number" }, "issue number must be a positive whole number"),
    ] {
        assert_eq!(error.to_string(), expected);
        assert_eq!(error.clone(), error);
        assert!(std::error::Error::source(&error).is_none());
    }
}
