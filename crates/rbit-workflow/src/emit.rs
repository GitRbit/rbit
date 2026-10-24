//! Writing a [`Node`] tree back out: block-style YAML for task payloads, JSON for storage.
//!
//! The YAML is meant for a runner's parser, so correctness wins over beauty, but it stays
//! readable: plain scalars are written as they were in the source (see
//! [`crate::yaml::Scalar::plain`]), strings that are safe unquoted stay unquoted, multi-line
//! strings become literal blocks (`|`), and everything else is double-quoted with escapes.

use std::fmt::Write as _;

use crate::yaml::{Mapping, Node, Scalar, ScalarValue, Value};

/// A mapping as a block-style YAML document, ending in a newline.
pub fn document(mapping: &Mapping) -> String {
    let mut out = String::new();
    write_mapping(&mut out, mapping, 0);
    out
}

/// A node as compact JSON. YAML-only values have no JSON form: non-finite floats become
/// `null`.
pub fn json(node: &Node) -> String {
    let mut out = String::new();
    write_json(&mut out, node);
    out
}

fn write_json(out: &mut String, node: &Node) {
    match &node.value {
        Value::Scalar(scalar) => match &scalar.value {
            ScalarValue::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            ScalarValue::Int(value) => {
                let _ = write!(out, "{value}");
            }
            ScalarValue::Float(value) if value.is_finite() => out.push_str(&float(*value)),
            ScalarValue::Null | ScalarValue::Float(_) => out.push_str("null"),
            ScalarValue::String(text) => quoted(out, text),
        },
        Value::Sequence(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json(out, item);
            }
            out.push(']');
        }
        Value::Mapping(mapping) => {
            out.push('{');
            for (index, (key, value)) in mapping.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                quoted(out, &key.name);
                out.push(':');
                write_json(out, value);
            }
            out.push('}');
        }
    }
}

fn indent(out: &mut String, columns: usize) {
    out.extend(std::iter::repeat_n(' ', columns));
}

fn write_mapping(out: &mut String, mapping: &Mapping, columns: usize) {
    for (key, value) in mapping.iter() {
        indent(out, columns);
        write_key(out, &key.name);
        out.push(':');
        write_value(out, value, columns);
    }
}

/// Writes what follows `key:` or `-`: an inline scalar or empty collection, or a nested
/// block on the next lines.
fn write_value(out: &mut String, node: &Node, columns: usize) {
    match &node.value {
        Value::Scalar(scalar) => {
            out.push(' ');
            write_scalar(out, scalar, columns + 2);
            out.push('\n');
        }
        Value::Mapping(mapping) if mapping.is_empty() => out.push_str(" {}\n"),
        Value::Sequence(items) if items.is_empty() => out.push_str(" []\n"),
        Value::Mapping(mapping) => {
            out.push('\n');
            write_mapping(out, mapping, columns + 2);
        }
        Value::Sequence(items) => {
            out.push('\n');
            write_sequence(out, items, columns + 2);
        }
    }
}

fn write_sequence(out: &mut String, items: &[Node], columns: usize) {
    for item in items {
        indent(out, columns);
        out.push('-');
        match &item.value {
            // `- key: value`: the nested block's first line continues the dash's line.
            Value::Mapping(mapping) if !mapping.is_empty() => {
                let mut block = String::new();
                write_mapping(&mut block, mapping, columns + 2);
                out.push(' ');
                out.push_str(&block[columns + 2..]);
            }
            Value::Sequence(nested) if !nested.is_empty() => {
                let mut block = String::new();
                write_sequence(&mut block, nested, columns + 2);
                out.push(' ');
                out.push_str(&block[columns + 2..]);
            }
            _ => write_value(out, item, columns),
        }
    }
}

fn write_key(out: &mut String, key: &str) {
    if is_plain_safe(key) {
        out.push_str(key);
    } else {
        quoted(out, key);
    }
}

/// `columns` is the indentation of a literal block's content.
fn write_scalar(out: &mut String, scalar: &Scalar, columns: usize) {
    if let Some(plain) = &scalar.plain {
        out.push_str(plain);
        return;
    }
    match &scalar.value {
        ScalarValue::Null => out.push_str("null"),
        ScalarValue::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        ScalarValue::Int(value) => {
            let _ = write!(out, "{value}");
        }
        ScalarValue::Float(value) => out.push_str(&float(*value)),
        ScalarValue::String(text) if is_plain_safe(text) => out.push_str(text),
        ScalarValue::String(text) if is_literal_safe(text) => literal(out, text, columns),
        ScalarValue::String(text) => quoted(out, text),
    }
}

/// A float that reads back as a float: `3.0`, not `3`; `1e300`, not 301 digits.
fn float(value: f64) -> String {
    if value.is_nan() {
        ".nan".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { ".inf".to_owned() } else { "-.inf".to_owned() }
    } else if value.fract() != 0.0 {
        format!("{value}")
    } else if value.abs() < 1e16 {
        format!("{value:.1}")
    } else {
        format!("{value:e}")
    }
}

/// Whether a plain scalar might resolve to something other than a string in YAML 1.1 or
/// 1.2 (null, booleans of either version, numbers). Conservative: when in doubt, quote.
pub(crate) fn looks_like_non_string(text: &str) -> bool {
    const WORDS: [&str; 16] =
        ["null", "~", "true", "false", "yes", "no", "on", "off", "y", "n", ".nan", ".inf", "+.inf", "-.inf", "<<", "="];
    let lower = text.to_ascii_lowercase();
    if WORDS.contains(&lower.as_str()) {
        return true;
    }
    let digits = lower.trim_start_matches(['+', '-']);
    digits.starts_with(|c: char| c.is_ascii_digit()) || digits.starts_with('.') && digits[1..].starts_with(|c: char| c.is_ascii_digit())
}

/// Whether `text` can be written as a plain scalar in block context and read back as the
/// same string.
fn is_plain_safe(text: &str) -> bool {
    !text.is_empty()
        && !looks_like_non_string(text)
        && !text.starts_with(|c: char| "-?:,[]{}#&*!|>'\"%@`".contains(c) || c.is_whitespace())
        && !text.ends_with(|c: char| c == ':' || c.is_whitespace())
        && !text.contains(": ")
        && !text.contains(" #")
        && text.chars().all(|c| !c.is_control() && !matches!(c, '\u{85}' | '\u{2028}' | '\u{2029}' | '\u{feff}'))
}

/// Whether `text` can be a literal block scalar: several lines of printable text (tabs
/// allowed), no line with trailing whitespace, and a first non-empty line that does not
/// start with whitespace (it sets the block's indentation).
fn is_literal_safe(text: &str) -> bool {
    text.contains('\n')
        && text.split('\n').find(|line| !line.is_empty()).is_some_and(|line| !line.starts_with([' ', '\t']))
        && text.split('\n').all(|line| !line.ends_with([' ', '\t']))
        && text.chars().all(|c| c == '\n' || c == '\t' || !c.is_control() && !matches!(c, '\u{85}' | '\u{2028}' | '\u{2029}' | '\u{feff}'))
}

/// `|`, `|-` or `|+` by the number of trailing newlines, then the lines at `columns`.
fn literal(out: &mut String, text: &str, columns: usize) {
    let body = text.trim_end_matches('\n');
    let chomping = match text.len() - body.len() {
        0 => "-",
        1 => "",
        _ => "+",
    };
    out.push('|');
    out.push_str(chomping);
    for line in text.strip_suffix('\n').unwrap_or(text).split('\n') {
        out.push('\n');
        if !line.is_empty() {
            indent(out, columns);
            out.push_str(line);
        }
    }
}

/// A double-quoted scalar; the escapes are also valid JSON.
fn quoted(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() || matches!(c, '\u{85}' | '\u{2028}' | '\u{2029}' | '\u{feff}') => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Limits, error::Location, yaml};

    fn parse(source: &str) -> Node {
        yaml::parse_document(source, &Limits::default()).unwrap()
    }

    fn round_trip(source: &str) -> String {
        let node = parse(source);
        let emitted = document(node.as_mapping().unwrap());
        let again = parse(&emitted);
        assert!(again.same_value(&node), "{source}\n---\n{emitted}");
        emitted
    }

    #[test]
    fn block_style_with_plain_spellings_kept() {
        let source = "name: CI\n'on':\n  push:\n    branches: [main]\nenv: {A: 3.10, B: '3.10', C: yes, D: \"yes\", E: ~, F: '', G: 0x1F}\njobs:\n  a:\n    runs-on: [self-hosted, linux]\n    steps:\n      - run: |\n          echo one\n          echo two\n      - {uses: x@v1, with: {k: v}}\n      - [nested, list]\n      - []\n";
        let emitted = round_trip(source);
        assert_eq!(
            emitted,
            "name: CI\n\"on\":\n  push:\n    branches:\n      - main\nenv:\n  A: 3.10\n  B: \"3.10\"\n  C: yes\n  D: \"yes\"\n  E: ~\n  F: \"\"\n  G: 0x1F\njobs:\n  a:\n    runs-on:\n      - self-hosted\n      - linux\n    steps:\n      - run: |\n          echo one\n          echo two\n      - uses: x@v1\n        with:\n          k: v\n      - - nested\n        - list\n      - []\n"
        );
    }

    #[test]
    fn awkward_strings() {
        let strings = [
            "",
            " leading",
            "trailing ",
            "a: b",
            "a #b",
            "#x",
            "- x",
            "x:",
            "\nleading newline",
            "two\nlines",
            "two\nlines\n",
            "ends\n\n",
            "tab\there",
            "quote \" and \\ backslash",
            "control \u{1}",
            "line\u{2028}separator",
            "{x}",
            "${{ matrix.os }}",
            "é ✓",
            "trailing space \nline",
            " \nx",
            "\n x",
            "\n\n",
            "\r\n",
            "yes",
            "1.0",
            "null",
            "*star",
            "!bang",
            "%percent",
            "@at",
            "`tick",
        ];
        for text in strings {
            let mut mapping = Mapping::default();
            mapping.0.push((
                yaml::Key { name: text.to_owned(), at: Location::new(1, 1) },
                Node::scalar(Scalar::string(text), Location::new(1, 1)),
            ));
            let emitted = document(&mapping);
            let back = parse(&emitted);
            let entry = back.as_mapping().unwrap().iter().next().unwrap();
            assert_eq!(entry.0.name, text, "key {text:?}\n{emitted}");
            assert_eq!(entry.1.as_scalar().map(|s| &s.value), Some(&ScalarValue::String(text.to_owned())), "value {text:?}\n{emitted}");
        }
    }

    #[test]
    fn literal_blocks() {
        let mut out = String::new();
        literal(&mut out, "a\n\nb\n", 4);
        assert_eq!(out, "|\n    a\n\n    b");
        let mut out = String::new();
        literal(&mut out, "\nx", 2);
        assert_eq!(out, "|-\n\n  x");
        let mut out = String::new();
        literal(&mut out, "x\n\n", 2);
        assert_eq!(out, "|+\n  x\n");
    }

    #[test]
    fn json_output() {
        let node = parse("a: [1, 2.5, 3.0, true, null, 'x\"y', .nan]\nb: {c: \"é\\n\"}\n");
        assert_eq!(json(&node), r#"{"a":[1,2.5,3.0,true,null,"x\"y",null],"b":{"c":"é\n"}}"#);
    }

    #[test]
    fn floats_read_back_as_floats() {
        assert_eq!(float(3.0), "3.0");
        assert_eq!(float(-0.5), "-0.5");
        assert_eq!(float(1e300), "1e300");
        assert_eq!(float(2.5), "2.5");
        assert_eq!(float(f64::NEG_INFINITY), "-.inf");
    }

    #[test]
    fn non_string_spellings_are_recognised() {
        for text in ["null", "Null", "~", "TRUE", "yes", "On", "n", "1", "-1", "+1.5", ".5", "0x10", "1e3", ".inf", "-.Inf", "<<"] {
            assert!(looks_like_non_string(text), "{text}");
        }
        for text in ["main", "v1.2", "ubuntu-latest", ".github", "-", "", "x1", "nope", "online"] {
            assert!(!looks_like_non_string(text), "{text}");
        }
    }

    mod property {
        use proptest::prelude::*;

        use super::*;

        fn scalar() -> impl Strategy<Value = ScalarValue> {
            prop_oneof![
                Just(ScalarValue::Null),
                any::<bool>().prop_map(ScalarValue::Bool),
                any::<i64>().prop_map(|value| ScalarValue::Int(value.into())),
                (-1e9f64..1e9).prop_map(ScalarValue::Float),
                any::<String>().prop_map(ScalarValue::String),
                "[a-z :#\\-\\n\"'\\\\{}\\[\\]]{0,12}".prop_map(ScalarValue::String),
            ]
        }

        fn node() -> impl Strategy<Value = Node> {
            let at = Location::new(1, 1);
            let leaf = scalar().prop_map(move |value| Node::scalar(Scalar::new(value), at));
            leaf.prop_recursive(4, 48, 6, move |inner| {
                prop_oneof![
                    prop::collection::vec(inner.clone(), 0..6).prop_map(move |items| Node::new(Value::Sequence(items), at)),
                    prop::collection::btree_map("[a-zA-Z0-9 _:#-]{1,8}", inner, 0..6).prop_map(move |entries| {
                        let entries = entries.into_iter().map(|(name, value)| (yaml::Key { name, at }, value)).collect();
                        Node::new(Value::Mapping(Mapping(entries)), at)
                    }),
                ]
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn emitted_documents_read_back_as_the_same_data(value in node()) {
                let mut mapping = Mapping::default();
                mapping.0.push((yaml::Key { name: "root".into(), at: Location::new(1, 1) }, value.clone()));
                let emitted = document(&mapping);
                let back = yaml::parse_document(&emitted, &Limits::default()).map_err(|error| TestCaseError::fail(format!("{error}\n{emitted}")))?;
                let back = back.as_mapping().and_then(|mapping| mapping.get("root")).cloned();
                prop_assert!(back.is_some_and(|back| back.same_value(&value)), "{}", emitted);
            }
        }
    }
}
