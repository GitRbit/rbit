//! Existing yaml acceptance fixtures, separated from production logic.
use super::{ErrorKind, Limits, Location, Node, ScalarValue, has_header, parse_document};

fn parse(source: &str) -> Node {
    parse_document(source, &Limits::default()).unwrap()
}

fn scalars(node: &Node) -> Vec<(ScalarValue, Option<String>)> {
    node.as_sequence().unwrap().iter().map(|item| item.as_scalar().map(|s| (s.value.clone(), s.plain.clone())).unwrap()).collect()
}

#[test]
fn scalars_keep_their_value_and_plain_spelling() {
    let doc =
        parse("- on\n- yes\n- true\n- 3.10\n- 0x1F\n- ~\n- \"3.10\"\n- 'it''s'\n- plain text\n- !!str 3.10\n- |-\n  2.0\n- .nan\n-\n");
    let plain = |text: &str| Some(text.to_owned());
    let got = scalars(&doc);
    assert_eq!(got[0], (ScalarValue::String("on".into()), plain("on")), "plain, so a runner reads it as it would have");
    assert_eq!(got[1], (ScalarValue::String("yes".into()), plain("yes")));
    assert_eq!(got[2], (ScalarValue::Bool(true), plain("true")));
    assert_eq!(got[3], (ScalarValue::Float(3.1), plain("3.10")));
    assert_eq!(got[4], (ScalarValue::Int(31), plain("0x1F")));
    assert_eq!(got[5], (ScalarValue::Null, plain("~")));
    assert_eq!(got[6], (ScalarValue::String("3.10".into()), None));
    assert_eq!(got[7], (ScalarValue::String("it's".into()), None));
    assert_eq!(got[8], (ScalarValue::String("plain text".into()), plain("plain text")));
    assert_eq!(got[9], (ScalarValue::String("3.10".into()), None), "a tagged string is not plain");
    assert_eq!(got[10], (ScalarValue::String("2.0".into()), None), "a block scalar is not plain");
    assert!(matches!(got[11], (ScalarValue::Float(value), _) if value.is_nan()));
    assert_eq!(got[12].0, ScalarValue::Null);
    let texts: Vec<_> = doc.as_sequence().unwrap().iter().map(|item| item.as_scalar().unwrap().text()).collect();
    assert_eq!(texts[..10], ["on", "yes", "true", "3.10", "0x1F", "~", "3.10", "it's", "plain text", "3.10"]);
}

#[test]
fn mappings_keep_order_locations_and_resolve_aliases() {
    let doc = parse("b: &x {k: 1}\na: *x\n'on': [x]\nm:\n  <<: *x\n  j: 2\n");
    let mapping = doc.as_mapping().unwrap();
    let keys: Vec<_> = mapping.iter().map(|(key, _)| key.name.as_str()).collect();
    assert_eq!(keys, ["b", "a", "on", "m"]);
    assert_eq!(mapping.entry("a").unwrap().0.at, Location::new(2, 1));
    assert_eq!(mapping.get("a").unwrap().at.line, 2, "an alias is located where it is used");
    assert!(mapping.get("a").unwrap().same_value(mapping.get("b").unwrap()));
    let merged = mapping.get("m").unwrap().as_mapping().unwrap();
    assert_eq!(merged.get("k").and_then(Node::as_text).as_deref(), Some("1"));
    assert_eq!(merged.get("j").and_then(Node::as_text).as_deref(), Some("2"));
}

#[test]
fn same_value_compares_data_like_go() {
    let doc = parse("- 1.10\n- 1.1\n- 1\n- 1.0\n- '1'\n- {a: [1]}\n- {a: [1]}\n- {a: [2]}\n");
    let items = doc.as_sequence().unwrap();
    assert!(items[0].same_value(&items[1]));
    assert!(!items[2].same_value(&items[3]));
    assert!(!items[2].same_value(&items[4]));
    assert!(items[5].same_value(&items[6]));
    assert!(!items[5].same_value(&items[7]));
}

#[test]
fn errors_carry_their_location() {
    let error = parse_document("a: [1, 2\nb: 3", &Limits::default()).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Parse);
    assert_eq!(error.at, Some(Location::new(2, 2)));
    assert!(!error.message.contains(" at line "), "{}", error.message);
    let duplicate = parse_document("k: 1\nk: 2\n", &Limits::default()).unwrap_err();
    assert!(duplicate.message.contains("duplicate"), "{}", duplicate.message);
    assert!(parse_document("a: 1\n---\nb: 2\n", &Limits::default()).is_err(), "one document only");
    assert!(parse_document("[a]: b\n", &Limits::default()).is_err(), "keys are strings");
    assert!(parse("").is_null());
}

#[test]
fn budgets_stop_hostile_documents() {
    let limits = Limits::default();
    let deep = format!("{}1{}", "[".repeat(limits.max_depth + 1), "]".repeat(limits.max_depth + 1));
    assert!(parse_document(&deep, &limits).is_err());
    // Eleven levels of ten aliases each: 10^11 nodes if expanded.
    let laughs = (1..12u8)
        .map(|level| {
            let (previous, name) = (char::from(b'a' + level - 1), char::from(b'a' + level));
            format!("{name}: &{name} [{}]\n", vec![format!("*{previous}"); 10].join(", "))
        })
        .collect::<Vec<_>>()
        .concat();
    let laughs = format!("a: &a [x, x, x, x, x, x, x, x, x, x]\n{laughs}");
    assert!(parse_document(&laughs, &limits).is_err(), "billion laughs");
}

#[test]
fn headers_in_front_of_a_scalar() {
    for before in ["a: |", "a: |-\n  ", "a: >+2\n", "- !!str ", "x: !custom  "] {
        assert!(has_header(before), "{before}");
    }
    for before in ["a: ", "- ", "[x, ", "a:\n  ", "", "a: &anchor ", "b: c>d, "] {
        assert!(!has_header(before), "{before}");
    }
    let doc = parse("a: |\n  yes\nb: !!str yes\nc:\n  yes\n");
    let mapping = doc.as_mapping().unwrap();
    let plain: Vec<_> = ["a", "b", "c"].iter().map(|key| mapping.get(key).unwrap().as_scalar().unwrap().plain.clone()).collect();
    assert_eq!(plain, [None, None, Some("yes".to_owned())]);
}
