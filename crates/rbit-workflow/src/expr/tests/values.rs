//! Existing expression values acceptance cases.
use super::super::{Expr, Status, format_number, number_value, to_number, to_text, truthy};
use super::support::{contexts, value};
use serde_json::json;

/// GitHub's documented literals, operators and coercions.
#[test]
fn literals_operators_and_coercion() {
    let cases = [
        ("null", json!(null)),
        ("false", json!(false)),
        ("711", json!(711)),
        ("-9.2", json!(-9.2)),
        ("0xff", json!(255)),
        ("-2.99e-2", json!(-0.0299)),
        ("'Mona the Octocat'", json!("Mona the Octocat")),
        ("'It''s open source!'", json!("It's open source!")),
        // Equality coerces different kinds to numbers.
        ("null == 0", json!(true)),
        ("'' == 0", json!(true)),
        ("'' == null", json!(true)),
        ("true == 1", json!(true)),
        ("false == '0'", json!(true)),
        ("1 == '1.0'", json!(true)),
        ("1 == ' 1 '", json!(true)),
        ("'0x10' == 16", json!(true)),
        ("'abc' == 1", json!(false)),
        ("'abc' != 1", json!(true)),
        ("'Abc' == 'aBC'", json!(true)),
        ("'straße' == 'STRASSE'", json!(true)),
        ("1 == 1.0", json!(true)),
        ("fromJSON('[]') == fromJSON('[]')", json!(false)),
        ("fromJSON('[]') == 0", json!(false)),
        ("github.event == github.event", json!(true)),
        ("github.event == github['EVENT']", json!(true)),
        ("github.event.issue == github.event", json!(false)),
        ("vars.fruits == vars.fruits", json!(true)),
        // Ordering.
        ("1 < 2", json!(true)),
        ("'a' < 'B'", json!(true)),
        ("'b' <= 'B'", json!(true)),
        ("null < 1", json!(true)),
        ("1 < '2'", json!(true)),
        ("'abc' < 1", json!(false)),
        ("'abc' >= 1", json!(false)),
        ("2 >= 2", json!(true)),
        ("true > false", json!(true)),
        ("vars.fruits > 0", json!(false)),
        // Logic returns operands.
        ("'a' && 'b'", json!("b")),
        ("0 && 'x'", json!(0)),
        ("'' || 'x'", json!("x")),
        ("null || 0 || ''", json!("")),
        ("!null", json!(true)),
        ("!vars.fruits", json!(false)),
        ("!!'x'", json!(true)),
        ("1 < 2 == true", json!(true)),
        ("!(1 == 2) && 3 > 2", json!(true)),
        // Paths.
        ("github.event.issue.labels[1].name", json!("help wanted")),
        ("github.event.issue.labels[5]", json!(null)),
        ("github.event.issue.labels[-1]", json!(null)),
        ("github.event.issue.labels[0.9].name", json!("bug")),
        ("github.event.issue.labels['0']", json!(null)),
        ("github.missing.deeper", json!(null)),
        ("github.ref.length", json!(null)),
        ("needs.build.outputs.version", json!("1.2")),
        ("needs['build']['OUTPUTS'].version", json!("1.2")),
        ("vars.fruits.*.name", json!(["apple", "orange", "pear"])),
        ("vars.fruits.*.missing", json!([])),
        ("needs.*.result", json!(["success", "skipped"])),
        ("vars.vegetables.*.ediblePortions", json!([["hearts", "stems", "leaves"], ["roots", "stems", "leaves"], ["roots", "stalks"]])),
        ("vars.vegetables.*.ediblePortions.*", json!(["hearts", "stems", "leaves", "roots", "stems", "leaves", "roots", "stalks"])),
        ("vars.fruits.*[1]", json!([])),
        ("github.ref.*", json!([])),
        ("fromJSON('{\"a\": [1, 2]}').a[1]", json!(2)),
        ("fromJSON('[{\"a\": 1}, {\"b\": 2}]').*.a", json!([1])),
    ];
    for (expression, expected) in cases {
        assert_eq!(value(expression), expected, "{expression}");
    }
}

/// GitHub's documented function examples.
#[test]
fn functions() {
    let cases = [
        ("contains('Hello world', 'llo')", json!(true)),
        ("contains('Hello world', 'LLO')", json!(true)),
        ("contains('Hello world', 'xyz')", json!(false)),
        ("contains(github.event.issue.labels.*.name, 'bug')", json!(true)),
        ("contains(github.event.issue.labels.*.name, 'BUG')", json!(true)),
        ("contains(fromJSON('[\"push\", \"pull_request\"]'), github.event_name)", json!(true)),
        ("contains(fromJSON('[1, 2]'), '2')", json!(true)),
        ("contains(123, 2)", json!(true)),
        ("startsWith('Hello world', 'He')", json!(true)),
        ("startsWith('Hello world', 'he')", json!(true)),
        ("startsWith('Hello world', 'world')", json!(false)),
        ("endsWith('Hello world', 'ld')", json!(true)),
        ("endsWith('Hello world', 'LD')", json!(true)),
        ("format('Hello {0} {1} {2}', 'Mona', 'the', 'Octocat')", json!("Hello Mona the Octocat")),
        ("format('{{Hello {0} {1} {2}!}}', 'Mona', 'the', 'Octocat')", json!("{Hello Mona the Octocat!}")),
        ("format('{0}{0}{1}', 1, null)", json!("11")),
        ("format('{0}', vars.fruits)", json!("Array")),
        ("format('{0}', github.event)", json!("Object")),
        ("join(github.event.issue.labels.*.name, ', ')", json!("bug, help wanted")),
        ("join(fromJSON('[1, true, null, \"x\"]'))", json!("1,true,,x")),
        ("join('abc', '-')", json!("abc")),
        ("join(github.event)", json!("")),
        ("join(fromJSON('[]'))", json!("")),
        ("toJSON(fromJSON('{\"a\": [1]}'))", json!("{\n  \"a\": [\n    1\n  ]\n}")),
        ("toJSON('x')", json!("\"x\"")),
        ("toJSON(2)", json!("2")),
        ("toJSON(null)", json!("null")),
        ("fromJSON('{\"a\": 1}').a", json!(1)),
        ("fromJSON('true')", json!(true)),
        ("fromJSON(' 3.5 ')", json!(3.5)),
        ("fromJSON(toJSON(vars.fruits)) == vars.fruits", json!(false)),
        ("success()", json!(true)),
        ("failure()", json!(false)),
        ("cancelled()", json!(false)),
        ("always()", json!(true)),
    ];
    for (expression, expected) in cases {
        assert_eq!(value(expression), expected, "{expression}");
    }
    for (expression, message) in [
        ("format('{0} {1}', 'a')", "`{1}` has no value"),
        ("format('{x}', 'a')", "must start an argument"),
        ("format('{0', 'a')", "must start an argument"),
        ("format('}', 'a')", "single `}`"),
        ("fromJSON('{')", "not JSON"),
        ("fromJSON('')", "not JSON"),
        ("hashFiles('*.lock')", "only available on the runner"),
    ] {
        let error = Expr::parse(expression).unwrap().evaluate(&contexts().with_status(Status::Success)).unwrap_err();
        assert!(error.0.contains(message), "{expression}: {error}");
    }
}

#[test]
fn truthiness_and_text() {
    let falsy = [json!(null), json!(false), json!(0), json!(-0.0), json!("")];
    assert!(falsy.iter().all(|value| !truthy(value)));
    let truthy_values = [json!(true), json!(-1), json!("0"), json!("false"), json!([]), json!({})];
    assert!(truthy_values.iter().all(truthy));
    assert_eq!(format_number(1e20), "100000000000000000000");
    assert_eq!(format_number(0.1), "0.1");
    assert_eq!(format_number(f64::INFINITY), "Infinity");
    assert_eq!(format_number(f64::NAN), "NaN");
    assert_eq!(to_text(&json!(u64::MAX)), u64::MAX.to_string());
    assert_eq!(to_text(&number_value(2.0)), "2");
    assert_eq!(to_text(&number_value(-0.0)), "-0");
    assert_eq!(number_value(1e300), json!(1e300));
    for (text, number) in [("", 0.0), (" 12 ", 12.0), ("-1.5e1", -15.0), ("+3", 3.0), ("0x1f", 31.0), ("-0x1", -1.0)] {
        assert_eq!(to_number(&json!(text)), number, "{text}");
    }
    for text in ["abc", "inf", "NaN", "1x", "0x", "--1"] {
        assert!(to_number(&json!(text)).is_nan(), "{text}");
    }
}
