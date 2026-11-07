//! Existing expression templates acceptance cases.
use super::super::{Expr, MAX_TEMPLATE_BYTES, Template};
use super::support::{contexts, render};
use serde_json::json;

/// The templates rbit evaluated before this evaluator existed: a strict sub-language,
/// so every one of them still means the same.
#[test]
fn the_former_subset() {
    let cases = [
        ("plain", "plain"),
        ("", ""),
        ("${{ github.ref }}", "refs/heads/main"),
        ("ci-${{ github.ref }}-x", "ci-refs/heads/main-x"),
        ("${{github.REF}}", "refs/heads/main"),
        ("${{ github.head_ref || github.ref }}", "refs/heads/main"),
        ("${{ github.head_ref || github.run_id }}", "42"),
        ("${{ github.event.pull_request.number || github.ref }}", "refs/heads/main"),
        ("${{ github.ref && 'yes' }}", "yes"),
        ("${{ github.head_ref && 'yes' }}", ""),
        ("${{ matrix.zero || 'zero is falsy' }}", "zero is falsy"),
        ("${{ null || false }}", "false"),
        ("${{ 'it''s' }}", "it's"),
        ("${{ '}}' }}", "}}"),
        ("${{ (null || 'a') && ('b' || 'c') }}", "b"),
        ("${{ 1.5 }} ${{ 2 }} ${{ -0 }} ${{ 0x10 }}", "1.5 2 -0 16"),
        ("${{ true }}${{ matrix.os }}", "truelinux"),
        ("a ${{ 'b' }} c ${{ 'd' }}", "a b c d"),
        // Formerly refused, now evaluated.
        ("${{ format('{0}', github.ref) }}", "refs/heads/main"),
        ("${{ github.ref == 'x' }}", "false"),
        ("${{ !github.ref }}", "false"),
        ("${{ github['ref'] }}", "refs/heads/main"),
        ("${{ github.sha }}", ""),
    ];
    for (template, expected) in cases {
        assert_eq!(render(template).as_deref(), Ok(expected), "{template}");
    }
}

#[test]
fn refusals() {
    for template in [
        "${{ secrets.TOKEN }}",
        "${{ github.ref | x }}",
        "${{ github. }}",
        "${{ 'open }}",
        "${{ github.ref",
        "${{ (github.ref }}",
        "${{ }}",
        "${{ 1x }}",
        "${{ success() }}",
        "${{ hashFiles('**/Cargo.lock') }}",
        "${{ x.y }}",
    ] {
        assert!(render(template).is_err(), "{template}");
    }
    let deep = format!("${{{{ {}'x'{} }}}}", "(".repeat(40), ")".repeat(40));
    assert!(render(&deep).is_err());
    assert!(Template::parse(&"x".repeat(MAX_TEMPLATE_BYTES + 1)).is_err());
    assert_eq!(render("${{ secrets.TOKEN }}").unwrap_err().0, "the `secrets` context is not available here");
}

#[test]
fn partial_rendering_keeps_what_cannot_be_evaluated() {
    assert!(Template::parse("Build ${{ matrix.os }} (${{ format( }})").is_err(), "bad syntax is still refused when parsing");
    let template = Template::parse("Build ${{ matrix.os }} for ${{ secrets.target }}").unwrap();
    assert_eq!(template.render_partially(&contexts()), "Build linux for ${{ secrets.target }}");
    assert!(!template.is_literal());
    assert_eq!(template.contexts(), ["matrix", "secrets"]);
    assert!(Template::parse("literal").unwrap().is_literal());
    let single = Template::parse("${{ matrix.os }}").unwrap();
    assert_eq!(single.as_single_expr().map(Expr::contexts), Some(vec!["matrix"]));
    assert!(Template::parse("x ${{ matrix.os }}").unwrap().as_single_expr().is_none());
}

#[test]
fn single_expressions_keep_their_type() {
    let evaluate = |template: &str| Template::parse(template).unwrap().evaluate(&contexts()).unwrap();
    assert_eq!(evaluate("${{ matrix.experimental }}"), json!(true));
    assert_eq!(evaluate("${{ github.run_id }}"), json!(42));
    assert_eq!(evaluate("${{ fromJSON('[1, 2]') }}"), json!([1, 2]));
    assert_eq!(evaluate(" ${{ matrix.experimental }}"), json!(" true"));
    assert_eq!(evaluate("plain"), json!("plain"));
    assert_eq!(crate::expr::evaluate("${{ 3 }}", &contexts()).unwrap(), json!(3));
    assert_eq!(crate::expr::render("${{ 3 }}", &contexts()).unwrap(), "3");
}
