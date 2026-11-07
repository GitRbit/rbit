//! Token-parser precedence, function and structural limits.

use super::{Accessor, Expr, Function, MAX_NESTING};

fn printed(text: &str) -> String {
    Expr::parse(text).unwrap_or_else(|error| panic!("{text}: {error}")).to_string()
}

#[test]
fn precedence_and_printing() {
    let cases = [
        ("env || vars && job", "env || vars && job"),
        ("(env || vars) && job", "(env || vars) && job"),
        ("env || (vars || job)", "env || (vars || job)"),
        ("(env || vars) || job", "(env || vars) || job"),
        ("!env == vars", "!env == vars"),
        ("!(env == vars)", "!(env == vars)"),
        ("env < vars == job > steps", "env < vars == job > steps"),
        ("env == (vars == job)", "env == (vars == job)"),
        ("(env == vars) == job", "env == vars == job"),
        ("github.event['pull_request'].head.*[0]", "github.event['pull_request'].head.*[0]"),
        ("GitHub.Ref", "github.Ref"),
        ("(github).ref", "github.ref"),
        ("('a').b", "('a').b"),
        ("(env.b).c", "(env.b).c"),
        ("STARTSWITH(env, 'x')", "startsWith(env, 'x')"),
        ("fromJSON('[1]')[0]", "fromJSON('[1]')[0]"),
        ("format('{0}''s', 1.50, -2, 0x10)", "format('{0}''s', 1.5, -2, 16)"),
        ("inputs.null || steps.true", "inputs.null || steps.true"),
        ("success()", "success()"),
    ];
    for (text, expected) in cases {
        let printed = printed(text);
        assert_eq!(printed, expected, "{text}");
        assert_eq!(Expr::parse(&printed).unwrap(), Expr::parse(text).unwrap(), "{text}");
    }
}

#[test]
fn trees() {
    let tree = Expr::parse("env || vars || job && steps").unwrap();
    assert!(matches!(&tree, Expr::Or(items) if items.len() == 3 && matches!(items[2], Expr::And(_))));
    let tree = Expr::parse("needs.*.result").unwrap();
    assert_eq!(tree, Expr::Access(Box::new(Expr::Context("needs".into())), vec![Accessor::Filter, Accessor::Property("result".into())]));
    let tree = Expr::parse("success() && contains(needs.*.result, 'failure') || always()").unwrap();
    assert_eq!(tree.contexts(), ["needs"]);
    assert_eq!(tree.functions(), [Function::Success, Function::Contains, Function::Always]);
    assert!(tree.uses_status() && !Expr::parse("env == 1").unwrap().uses_status());
}

#[test]
fn refusals() {
    let cases = [
        ("", "empty"),
        ("env.", "property name"),
        ("env.1", "property name"),
        ("env[0", "`[` is not closed"),
        ("(env", "`(` is not closed"),
        ("env vars", "unexpected `vars`"),
        ("env ==", "incomplete"),
        ("unknown.x", "not a context"),
        ("nope(1)", "not a function"),
        ("contains(env)", "takes 2 arguments"),
        ("join()", "1 or 2"),
        ("format()", "at least 1"),
        ("success(1)", "takes 0"),
        ("contains(env vars)", "separated by `,`"),
        ("'a'.b", "no properties"),
        ("1[0]", "no properties"),
        (")", "unexpected `)`"),
    ];
    for (text, message) in cases {
        let error = Expr::parse(text).unwrap_err();
        assert!(error.0.contains(message), "{text}: {error}");
    }
}

#[test]
fn nesting_is_bounded() {
    let at = |depth: usize, open: &str, close: &str| format!("{}env{}", open.repeat(depth), close.repeat(depth));
    assert!(Expr::parse(&at(MAX_NESTING, "(", ")")).is_ok());
    assert!(Expr::parse(&at(MAX_NESTING + 1, "(", ")")).is_err());
    assert!(Expr::parse(&at(MAX_NESTING, "!", "")).is_ok());
    assert!(Expr::parse(&at(MAX_NESTING + 1, "!", "")).is_err());
    assert!(Expr::parse(&at(MAX_NESTING + 1, "env[", "]")).is_err());
    assert!(Expr::parse(&at(MAX_NESTING + 1, "toJSON(", ")")).is_err());
    assert!(Expr::parse(&vec!["env"; MAX_NESTING + 1].join(" == ")).is_ok());
    assert!(Expr::parse(&vec!["env"; MAX_NESTING + 2].join(" == ")).is_err(), "each chained comparison nests");
    assert!(Expr::parse(&vec!["env"; 500].join(" || ")).is_ok(), "a long || chain is flat");
    assert!(Expr::parse(&format!("github{}", ".a".repeat(500))).is_ok(), "a long path is flat");
    assert!(Expr::parse(&"env || ".repeat(1000)).unwrap_err().0.contains("at most"), "too long");
}
