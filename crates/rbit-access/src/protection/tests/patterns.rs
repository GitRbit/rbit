use super::super::*;

fn pattern(value: &str) -> BranchPattern {
    value.parse().unwrap()
}

#[test]
fn patterns_are_validated() {
    for valid in ["main", "release/*", "release/**", "**/hotfix", "v?.x", "feature/*-wip", "a.b/c_d", "日本"] {
        assert_eq!(pattern(valid).as_str(), valid);
    }
    assert_eq!(pattern("  main ").as_str(), "main", "trimmed");
    let cases = [
        ("", PatternError::Empty),
        ("a b", PatternError::Whitespace),
        ("a\tb", PatternError::Whitespace),
        ("a~b", PatternError::Character('~')),
        ("a^b", PatternError::Character('^')),
        ("a:b", PatternError::Character(':')),
        ("[ab]", PatternError::Character('[')),
        ("a\\b", PatternError::Character('\\')),
        ("a..b", PatternError::Sequence("..")),
        ("a@{b", PatternError::Sequence("@{")),
        ("a//b", PatternError::Sequence("//")),
        ("@", PatternError::Character('@')),
        ("/main", PatternError::Edge),
        ("main/", PatternError::Edge),
        ("main.", PatternError::Edge),
        (".hidden", PatternError::Segment),
        ("a/.b", PatternError::Segment),
        ("x.lock", PatternError::Segment),
        ("a/***", PatternError::Stars),
    ];
    for (value, error) in cases {
        assert_eq!(BranchPattern::parse(value), Err(error), "{value:?}");
    }
    assert_eq!(BranchPattern::parse(&"a".repeat(256)), Err(PatternError::TooLong));
    assert!(BranchPattern::parse(&"a".repeat(255)).is_ok());
    assert!(!pattern("main").is_glob() && pattern("m*").is_glob() && pattern("m?").is_glob());
}

#[test]
fn pattern_matching_table() {
    let table: &[(&str, &str, bool)] = &[
        ("main", "main", true),
        ("main", "Main", cfg!(any(windows, target_os = "macos"))),
        ("main", "main2", false),
        ("main", "x/main", false),
        ("*", "main", true),
        ("*", "release/1.0", false),
        ("release/*", "release/1.0", true),
        ("release/*", "release/1.0/hotfix", false),
        ("release/*", "release", false),
        ("release/*", "Release/1.0", cfg!(any(windows, target_os = "macos"))),
        ("release/**", "release/1.0/hotfix", true),
        ("release/**", "release/1.0", true),
        ("release/**", "release", false),
        ("**", "a/b/c", true),
        ("**/main", "main", true),
        ("**/main", "a/b/main", true),
        ("**/main", "a/mainx", false),
        ("a/**/b", "a/b", true),
        ("a/**/b", "a/x/y/b", true),
        ("a/**/b", "ab", false),
        ("v?", "v1", true),
        ("v?", "v10", false),
        ("v?", "v/", false),
        ("?", "", false),
        ("*-wip", "feature-wip", true),
        ("*-wip", "feature/x-wip", false),
        ("f*e*x", "feature-x", true),
        ("f*e*x", "feature-y", false),
        ("**-wip", "a/b-wip", true),
        ("日*", "日本", true),
    ];
    for (glob, branch, expected) in table {
        assert_eq!(pattern(glob).matches(branch), *expected, "{glob:?} vs {branch:?}");
    }
}

#[test]
fn matching_long_names_stays_fast() {
    let glob = pattern("*a*a*a*a*a*a*a*a*b");
    assert!(!glob.matches(&"a".repeat(250)));
}

#[test]
fn settings_validation_and_defaults() {
    let defaults = RuleSettings::default();
    assert!(defaults.deny_force_push && defaults.deny_deletion && defaults.block_on_changes_requested);
    assert!(!defaults.require_pull_request && !defaults.restrict_pushers && !defaults.allow_admin_bypass);
    assert_eq!(defaults.validate(), Ok(()));
    let approvals = RuleSettings { required_approvals: 2, ..defaults };
    assert_eq!(approvals.validate(), Err(RuleError::ApprovalsWithoutPullRequest));
    assert_eq!(RuleSettings { require_pull_request: true, ..approvals }.validate(), Ok(()));
    let too_many = RuleSettings { require_pull_request: true, required_approvals: 11, ..defaults };
    assert_eq!(too_many.validate(), Err(RuleError::TooManyApprovals));
}
