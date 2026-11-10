use super::*;

fn lines(text: &[&str]) -> Vec<String> {
    text.iter().map(|line| (*line).to_owned()).collect()
}

fn console(step: &str, first: usize) -> Console<'_> {
    Console { step, first, running: false, live: false }
}

#[test]
fn lines_are_taken_apart_into_time_marker_and_text() {
    assert_eq!(parse("plain"), Line { time: None, kind: Kind::Plain, text: "plain" });
    assert_eq!(parse("##[group]Run make"), Line { time: None, kind: Kind::Group, text: "Run make" });
    let stamped = parse("2026-11-10T12:00:01.1234567Z ##[error]it broke (file=a.rs,line=3)");
    assert_eq!(stamped, Line { time: Some("2026-11-10T12:00:01.1234567Z"), kind: Kind::Error, text: "it broke (file=a.rs,line=3)" });
    assert_eq!(parse("2026-11-10T12:00:01Z hi").time, Some("2026-11-10T12:00:01Z"));
    for unstamped in ["2026-11-10T12:00:01Z", "2026-11-10 12:00:01Z x", "2026-11-10T12:00:01.Z x", "2026-1x-10T12:00:01Z x"] {
        assert_eq!(parse(unstamped).time, None, "{unstamped}");
    }
    assert_eq!(parse("not ##[error] a marker").kind, Kind::Plain);
    assert_eq!(parse("::error::raw commands stay text").kind, Kind::Plain, "only the runner's markers count");
}

#[test]
fn every_line_has_its_anchor_and_number_link() {
    let rendered = console("3", 41).render(&lines(&["one", "two <b>"]));
    assert!(rendered.html.contains(r##"<div class="log-line" id="step-3-line-41" data-kind="plain"><a class="log-num" href="#step-3-line-41" data-line="41" aria-label="41"></a><span class="log-text">one</span></div>"##));
    assert!(rendered.html.contains(r#"id="step-3-line-42""#));
    assert!(rendered.html.contains("two &lt;b&gt;") && !rendered.html.contains("<b>"));
    assert!(console("setup", 1).render(&lines(&["x"])).html.contains(r#"id="step-setup-line-1""#));
}

#[test]
fn groups_fold_and_open_around_errors() {
    let text = [
        "before",
        "##[group]Run make",
        "  make all",
        "##[endgroup]",
        "##[group]Second",
        "inside",
        "##[error]boom",
        "##[endgroup]",
        "after",
    ];
    let rendered = console("0", 1).render(&lines(&text));
    let html = &rendered.html;
    assert_eq!(html.matches("<details").count(), 2);
    assert!(
        html.contains(r#"<details class="log-group"><summary class="log-line log-group-head" id="step-0-line-2" data-kind="group">"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<details class="log-group" open><summary class="log-line log-group-head" id="step-0-line-5""#),
        "a group with an error is open"
    );
    assert!(!html.contains("line-4\"") && !html.contains("endgroup"), "a group's end is not a row, but keeps its number");
    assert!(html.contains(r#"id="step-0-line-9""#) && html.ends_with("after</span></div>"));
    assert_eq!(rendered.last_error.as_deref(), Some("boom"));
}

#[test]
fn a_new_group_ends_the_last_and_an_unended_group_stays_open_while_running() {
    let text = lines(&["##[group]A", "a", "##[group]B", "b"]);
    let finished = console("1", 1).render(&text).html;
    assert_eq!(finished.matches("<details").count(), 2);
    assert!(finished.contains(r#"<details class="log-group" data-unended>"#), "{finished}");
    let running = Console { running: true, ..console("1", 1) }.render(&text).html;
    assert!(running.contains(r#"<details class="log-group" open data-unended>"#), "{running}");
}

#[test]
fn a_live_window_keeps_the_end_of_a_group_it_did_not_start() {
    let text = lines(&["still inside", "##[endgroup]", "outside"]);
    assert!(!console("2", 7).render(&text).html.contains("data-endgroup"));
    let live = Console { live: true, running: true, ..console("2", 7) }.render(&text).html;
    let marker = live.find("data-endgroup").unwrap();
    assert!(live.find("step-2-line-7").unwrap() < marker && marker < live.find("step-2-line-9").unwrap());
}

#[test]
fn annotations_are_highlighted_with_an_icon_and_their_label() {
    let html = console("0", 1).render(&lines(&["##[error]it <broke>", "##[warning]careful", "##[notice]fyi", "##[debug]x"])).html;
    assert!(html.contains(r#"data-kind="error""#) && html.contains(r#"<span class="log-label">Error: </span>it &lt;broke&gt;"#));
    assert!(html.contains(r#"<span class="log-label">Warning: </span>careful"#));
    assert!(html.contains(r#"<span class="log-label">Notice: </span>fyi"#));
    assert_eq!(html.matches(r#"class="log-icon""#).count(), 3);
    assert!(html.contains(r#"data-kind="debug"><a"#));
}

#[test]
fn times_are_shown_as_the_time_of_day_and_colours_as_classes() {
    let html = console("0", 1).render(&lines(&["2026-11-10T12:00:01.5Z \u{1b}[31mred\u{1b}[0m"])).html;
    assert!(html.contains(r#"<time class="log-time" datetime="2026-11-10T12:00:01.5Z">12:00:01</time>"#), "{html}");
    assert!(html.contains(r#"<span class="ansi-f1">red</span>"#));
    assert!(!html.contains('\u{1b}'));
}
