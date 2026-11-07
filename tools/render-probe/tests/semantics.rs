//! Identical compiled HTML and escaping, with no per-case output-buffer ownership.

use askama::Template;
use rbit_render_probe::{CASE_COUNT, COMPILED_MODES, FixtureData, Mode, ProbeError, Row, render_into};

fn data<'a>(value: &'a str, rows: &'a [Row<'a>]) -> FixtureData<'a> {
    FixtureData { title: value, site_name: value, viewer: Some(value), description: value, footer: value, dark: true, rows }
}

#[derive(Template)]
#[template(source = "{{ value }}", ext = "html")]
struct OrdinaryEscaping<'a> {
    value: &'a str,
}

#[test]
fn every_compiled_case_escapes_untrusted_text_and_attribute_values() {
    let malicious = "</title><script>alert(1)</script><img src=x onerror=alert(1)>\"&'";
    let escaped = OrdinaryEscaping { value: malicious }.render().unwrap();
    let rows = [Row { key: malicious, label: malicious }];
    let fixture = data(malicious, &rows);
    for &mode in COMPILED_MODES {
        let mut output = String::new();
        for case in 0..CASE_COUNT {
            output.clear();
            render_into(mode, case, &fixture, &mut output).unwrap();
            assert!(!output.contains("<script>") && !output.contains("<img src="));
            assert!(output.contains(&format!("<title>{escaped}</title>")));
            assert!(output.contains(&format!("data-key=\"{escaped}\"")));
            assert!(output.contains(&format!("data-case=\"{case}\"")));
            assert_eq!(output.matches(escaped.as_str()).count(), 8, "each of eight data slots escapes exactly once");
        }
    }
}

#[cfg(all(feature = "inheritance", feature = "segments", feature = "blocks"))]
#[test]
fn inheritance_segments_and_blocks_produce_identical_html_across_all_cases() {
    for rows in [vec![], vec![Row { key: "a\"b", label: "Unicode: שלום & café <value>" }]] {
        for viewer in [None, Some("alice <viewer>")] {
            for dark in [false, true] {
                let fixture = FixtureData { viewer, dark, ..data("Untrusted <title> & \"text\"", &rows) };
                let mut inherited = String::new();
                let mut segmented = String::new();
                let mut blocked = String::new();
                for case in 0..CASE_COUNT {
                    inherited.clear();
                    segmented.clear();
                    blocked.clear();
                    render_into(Mode::Inheritance, case, &fixture, &mut inherited).unwrap();
                    render_into(Mode::Segments, case, &fixture, &mut segmented).unwrap();
                    render_into(Mode::Blocks, case, &fixture, &mut blocked).unwrap();
                    assert_eq!(inherited, segmented, "case {case}: segmented shell preserves HTML");
                    assert_eq!(inherited, blocked, "case {case}: extracted block preserves HTML");
                }
            }
        }
    }
}

#[test]
fn caller_buffer_is_appended_to_and_reused() {
    let fixture = data("short", &[]);
    for &mode in COMPILED_MODES {
        let mut output = String::with_capacity(32_768);
        let pointer = output.as_ptr();
        let capacity = output.capacity();
        output.push_str("prefix");
        render_into(mode, 0, &fixture, &mut output).unwrap();
        assert!(output.starts_with("prefix<!doctype html>"));
        assert_eq!(output.as_ptr(), pointer);
        assert_eq!(output.capacity(), capacity);
        output.clear();
        render_into(mode, CASE_COUNT - 1, &fixture, &mut output).unwrap();
        assert_eq!(output.as_ptr(), pointer);
    }
}

#[test]
fn invalid_case_and_uncompiled_mode_return_errors_before_writing() {
    let fixture = data("short", &[]);
    let mut output = String::from("unchanged");
    for &mode in COMPILED_MODES {
        assert!(matches!(render_into(mode, CASE_COUNT, &fixture, &mut output), Err(ProbeError::CaseOutOfRange { .. })));
        assert_eq!(output, "unchanged");
    }
    for mode in [Mode::Inheritance, Mode::Segments, Mode::Blocks] {
        if !COMPILED_MODES.contains(&mode) {
            assert!(matches!(render_into(mode, 0, &fixture, &mut output), Err(ProbeError::ModeUnavailable(_))));
            assert_eq!(output, "unchanged");
        }
    }
}
