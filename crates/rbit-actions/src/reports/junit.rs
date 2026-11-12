//! A bounded scanner for `JUnit` XML: `<testcase>` elements and the `<failure>` or `<error>`
//! inside them. It reads tags and attributes, skips comments, processing instructions and
//! declarations, keeps `CDATA` as text, and decodes the five XML entities and numeric
//! references; nothing else is resolved, so a hostile report costs at most its size.

use super::{FailedTest, MAX_FAILED_TESTS, MAX_TEST_MESSAGE, TestReport};

/// The test case being read.
#[derive(Default)]
struct Case {
    id: String,
    path: Option<String>,
    line: Option<u64>,
    failed: bool,
    message: Option<String>,
    text: String,
    in_failure: bool,
}

/// Adds what `JUnit` document `xml` says to `report`.
pub fn parse_junit(xml: &str, report: &mut TestReport) {
    let mut case: Option<Case> = None;
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        if let Some(case) = case.as_mut().filter(|case| case.in_failure) {
            push_text(&mut case.text, &decode(&rest[..start]));
        }
        rest = &rest[start..];
        let (skip, cdata) = if rest.starts_with("<!--") {
            (rest.find("-->").map(|end| end + 3), None)
        } else if let Some(body) = rest.strip_prefix("<![CDATA[") {
            (body.find("]]>").map(|end| end + 12), body.find("]]>").map(|end| &body[..end]))
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            (rest.find('>').map(|end| end + 1), None)
        } else {
            (None, None)
        };
        if let Some(skip) = skip {
            if let (Some(text), Some(case)) = (cdata, case.as_mut().filter(|case| case.in_failure)) {
                push_text(&mut case.text, text);
            }
            rest = &rest[skip..];
            continue;
        }
        let Some(end) = tag_end(rest) else { break };
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            match name.trim() {
                "testcase" => finish(case.take(), report),
                "failure" | "error" => {
                    if let Some(case) = case.as_mut() {
                        case.in_failure = false;
                    }
                }
                _ => {}
            }
            continue;
        }
        let closed = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let name = tag.split(|c: char| c.is_whitespace()).next().unwrap_or_default();
        match name {
            "testcase" => {
                finish(case.take(), report);
                let attr = |key| attribute(tag, key);
                let (class, test) = (attr("classname").unwrap_or_default(), attr("name").unwrap_or_default());
                let id = if class.is_empty() { test } else { format!("{class} {test}") };
                let next = Case { id, path: attr("file"), line: attr("line").and_then(|line| line.parse().ok()), ..Case::default() };
                if closed { finish(Some(next), report) } else { case = Some(next) }
            }
            "failure" | "error" => {
                if let Some(case) = case.as_mut() {
                    case.failed = true;
                    case.message = case.message.take().or_else(|| attribute(tag, "message"));
                    case.in_failure = !closed;
                }
            }
            _ => {}
        }
    }
    finish(case, report);
}

/// Counts finished test case `case` and keeps it when it failed.
fn finish(case: Option<Case>, report: &mut TestReport) {
    let Some(case) = case else { return };
    report.total += 1;
    if !case.failed {
        return;
    }
    report.failed += 1;
    if report.items.len() >= MAX_FAILED_TESTS {
        return;
    }
    let (found_path, found_line, after) = panic_location(&case.text);
    let message = after
        .or_else(|| case.message.clone().filter(|message| !message.trim().is_empty()))
        .or_else(|| case.text.lines().map(str::trim).find(|line| !line.is_empty()).map(str::to_owned))
        .unwrap_or_default();
    report.items.push(FailedTest {
        id: clip(&case.id),
        path: case.path.or(found_path),
        line: case.line.or(found_line),
        message: clip(message.trim()),
    });
}

/// Where a Rust test panicked, from `thread '…' panicked at path:line:col:` and the line
/// after it, which is the panic's message.
fn panic_location(text: &str) -> (Option<String>, Option<u64>, Option<String>) {
    let mut lines = text.lines().skip_while(|line| !line.contains("panicked at "));
    let Some(line) = lines.next() else { return (None, None, None) };
    let place = line.split("panicked at ").nth(1).unwrap_or_default().trim().trim_end_matches(':');
    let mut parts = place.rsplitn(3, ':');
    let (_column, line_number, path) = (parts.next(), parts.next(), parts.next());
    let message = lines.map(str::trim).find(|line| !line.is_empty()).map(str::to_owned);
    match (path, line_number.and_then(|number| number.parse().ok())) {
        (Some(path), Some(number)) => (Some(path.to_owned()), Some(number), message),
        _ => (None, None, message),
    }
}

/// The index of the `>` closing the tag at the start of `rest`, outside quoted values.
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote = None;
    for (index, c) in rest.char_indices().skip(1) {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), _) if open == c => quote = None,
            (None, '>') => return Some(index),
            _ => {}
        }
    }
    None
}

/// The decoded value of attribute `key` of `tag`.
fn attribute(tag: &str, key: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(at) = rest.find(key) {
        let before = rest[..at].chars().last();
        let after = rest[at + key.len()..].trim_start();
        rest = &rest[at + key.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else { continue };
        let value = value.trim_start();
        let quote = value.chars().next().filter(|c| matches!(c, '"' | '\''))?;
        let body = &value[1..];
        return body.find(quote).map(|end| decode(&body[..end]));
    }
    None
}

/// `text` with XML entities and character references decoded.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .map(|hex| u32::from_str_radix(hex, 16))
                .or_else(|| entity.strip_prefix('#').map(str::parse))
                .and_then(Result::ok)
                .and_then(char::from_u32),
        };
        if let Some(c) = decoded {
            out.push(c);
            rest = &rest[end + 1..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

/// Adds `more` to failure text `text`, which stops growing at 64 KiB.
fn push_text(text: &mut String, more: &str) {
    const MAX_TEXT: usize = 64 * 1024;
    if text.len() < MAX_TEXT {
        let room = MAX_TEXT - text.len();
        text.push_str(clip_to(more, room));
    }
}

fn clip_to(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// `text` cut to [`MAX_TEST_MESSAGE`] bytes, with `…` when cut.
fn clip(text: &str) -> String {
    if text.len() <= MAX_TEST_MESSAGE { text.to_owned() } else { format!("{}…", clip_to(text, MAX_TEST_MESSAGE - 3)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEXTEST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="3" failures="1">
  <!-- a comment with <testcase> inside -->
  <testsuite name="rbit-e2e::e2e" tests="3">
    <testcase name="pulls::merge_rejects_stale_head" classname="rbit-e2e::e2e" time="1.2">
      <failure type="test failure">thread 'pulls::merge_rejects_stale_head' panicked at crates/rbit-e2e/tests/e2e/pulls.rs:412:5:
assertion `left == right` failed
  left: 405</failure>
      <system-out>noise</system-out>
    </testcase>
    <testcase name="issues::list" classname="rbit-e2e::e2e" time="0.1"/>
    <testcase name="odd &amp; named" classname="x"><skipped/></testcase>
  </testsuite>
</testsuites>"#;

    #[test]
    fn nextest_failures_carry_their_place_and_message() {
        let mut report = TestReport::default();
        parse_junit(NEXTEST, &mut report);
        assert_eq!((report.total, report.failed), (3, 1));
        let failed = &report.items[0];
        assert_eq!(failed.id, "rbit-e2e::e2e pulls::merge_rejects_stale_head");
        assert_eq!((failed.path.as_deref(), failed.line), (Some("crates/rbit-e2e/tests/e2e/pulls.rs"), Some(412)));
        assert_eq!(failed.message, "assertion `left == right` failed");
    }

    #[test]
    fn other_reports_use_their_attributes_and_cdata() {
        let xml = r#"<testsuite><testcase classname="tests.test_api" name="test_get" file="tests/test_api.py" line="12">
            <error message="KeyError: &apos;id&apos; &#x26; more"><![CDATA[Traceback <here>]]></error></testcase>
            <testcase name="no_class"><failure/></testcase></testsuite>"#;
        let mut report = TestReport::default();
        parse_junit(xml, &mut report);
        assert_eq!((report.total, report.failed), (2, 2));
        let first = &report.items[0];
        assert_eq!(
            (first.id.as_str(), first.path.as_deref(), first.line),
            ("tests.test_api test_get", Some("tests/test_api.py"), Some(12))
        );
        assert_eq!(first.message, "KeyError: 'id' & more");
        assert_eq!((report.items[1].id.as_str(), report.items[1].message.as_str()), ("no_class", ""));
    }

    #[test]
    fn hostile_input_is_bounded_and_never_panics() {
        let mut report = TestReport::default();
        for input in ["<", "<testcase", "<testcase name=\"a", "&#xFFFFFFFF;", "<![CDATA[", "<!--", "<testcase name='a'><failure>&bogus;"] {
            parse_junit(input, &mut report);
        }
        let case = format!("<testcase name=\"t\"><failure message=\"{}\"/></testcase>", "x".repeat(2000));
        let many = case.repeat(200);
        let mut report = TestReport::default();
        parse_junit(&many, &mut report);
        assert_eq!((report.total, report.failed, report.items.len()), (200, 200, MAX_FAILED_TESTS));
        assert!(report.items[0].message.len() <= MAX_TEST_MESSAGE);
        assert_eq!(decode("a &lt;b&gt; &#65; &unknown; &"), "a <b> A &unknown; &");
    }
}
