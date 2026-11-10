//! A step's log lines as the job page's log console: line anchors, timestamps, folded
//! groups, and the runner's workflow-command markers drawn as highlighted lines.
//!
//! The runner turns workflow commands into markers (`##[group]`, `##[error]` …, see
//! rbit-runner's `commands`) and may start each line with its time
//! (`2026-11-10T12:00:01.123Z `, as GitHub's raw logs do). `::add-mask::` values never
//! reach the log: the runner masks them, and the server masks secrets again.
//!
//! Every line is a row `#step-<step>-line-<n>`, `n` counted from 1 within the step, with a
//! link to itself. A group is a `<details>` whose summary is its first line; a new group
//! ends the one before, as on GitHub. Groups are folded unless they hold an error or are
//! still being written. Log text is always escaped; colours are `ansi-*` classes.

use super::ansi::{ansi_html, plain};
use std::fmt::Write as _;

/// What a line is, by its marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Group,
    EndGroup,
    Error,
    Warning,
    Notice,
    Debug,
    Command,
}

impl Kind {
    const MARKERS: [(&'static str, Self); 7] = [
        ("##[group]", Self::Group),
        ("##[endgroup]", Self::EndGroup),
        ("##[error]", Self::Error),
        ("##[warning]", Self::Warning),
        ("##[notice]", Self::Notice),
        ("##[debug]", Self::Debug),
        ("##[command]", Self::Command),
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Group => "group",
            Self::EndGroup => "endgroup",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Notice => "notice",
            Self::Debug => "debug",
            Self::Command => "command",
        }
    }

    /// The label before an annotation's message, as GitHub's log shows it (log text, not
    /// page text: it is never translated).
    const fn label(self) -> Option<&'static str> {
        match self {
            Self::Error => Some("Error: "),
            Self::Warning => Some("Warning: "),
            Self::Notice => Some("Notice: "),
            _ => None,
        }
    }
}

/// One log line taken apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    /// The runner's RFC 3339 UTC time, when the line starts with one.
    pub time: Option<&'a str>,
    pub kind: Kind,
    /// The rest, without the time and the marker.
    pub text: &'a str,
}

/// Whether `text` starts with `YYYY-MM-DDTHH:MM:SS[.fraction]Z ` and how long that is.
fn timestamp_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| bytes.get(range).is_some_and(|part| part.iter().all(u8::is_ascii_digit));
    let fixed = digits(0..4)
        && bytes.get(4) == Some(&b'-')
        && digits(5..7)
        && bytes.get(7) == Some(&b'-')
        && digits(8..10)
        && bytes.get(10) == Some(&b'T')
        && digits(11..13)
        && bytes.get(13) == Some(&b':')
        && digits(14..16)
        && bytes.get(16) == Some(&b':')
        && digits(17..19);
    if !fixed {
        return None;
    }
    let mut end = 19;
    if bytes.get(end) == Some(&b'.') {
        let fraction = bytes[end + 1..].iter().take_while(|byte| byte.is_ascii_digit()).count();
        if !(1..=9).contains(&fraction) {
            return None;
        }
        end += 1 + fraction;
    }
    (bytes.get(end) == Some(&b'Z') && bytes.get(end + 1) == Some(&b' ')).then_some(end + 2)
}

/// `line` taken apart into its time, marker and text.
pub fn parse(line: &str) -> Line<'_> {
    let (time, rest) = match timestamp_len(line) {
        Some(length) => (Some(&line[..length - 1]), &line[length..]),
        None => (None, line),
    };
    for (marker, kind) in Kind::MARKERS {
        if let Some(text) = rest.strip_prefix(marker) {
            return Line { time, kind, text };
        }
    }
    Line { time, kind: Kind::Plain, text: rest }
}

/// A rendered window of a step's lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rendered {
    pub html: String,
    /// The plain text of the last error line, for the step's failure summary.
    pub last_error: Option<String>,
}

/// How a window is rendered.
#[derive(Debug, Clone, Copy)]
pub struct Console<'a> {
    /// The step's key in line ids: its number, `setup` or `complete`.
    pub step: &'a str,
    /// The step-relative number of the window's first line, from 1.
    pub first: usize,
    /// More lines may follow: a group still being written stays unfolded.
    pub running: bool,
    /// A live append: a group end without its start in the window is kept as a marker
    /// (`data-endgroup`) so the page can end the group it has open.
    pub live: bool,
}

const ICON_ERROR: &str = r#"<svg class="log-icon" viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6.25"/><path d="m5.75 5.75 4.5 4.5m0-4.5-4.5 4.5"/></svg>"#;
const ICON_WARNING: &str = r#"<svg class="log-icon" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2.25 14.25 13.5H1.75Z"/><path d="M8 6.5v3m0 2v.01"/></svg>"#;
const ICON_NOTICE: &str =
    r#"<svg class="log-icon" viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6.25"/><path d="M8 7.5v3.5m0-6v.01"/></svg>"#;
const CHEVRON: &str = r#"<svg class="log-chevron" viewBox="0 0 16 16" aria-hidden="true"><path d="m6 4 4 4-4 4"/></svg>"#;

impl Console<'_> {
    /// One row: its number link, its time and its text. `tag` is `div` or `summary`.
    fn row(&self, out: &mut String, tag: &str, number: usize, line: &Line<'_>) {
        let id = format!("step-{}-line-{number}", self.step);
        let group = if line.kind == Kind::Group { " log-group-head" } else { "" };
        let _ = write!(
            out,
            r##"<{tag} class="log-line{group}" id="{id}" data-kind="{}"><a class="log-num" href="#{id}" data-line="{number}" aria-label="{number}"></a>"##,
            line.kind.name()
        );
        if let Some(time) = line.time {
            let shown = time.get(11..19).unwrap_or(time);
            let _ = write!(out, r#"<time class="log-time" datetime="{time}">{shown}</time>"#);
        }
        out.push_str(r#"<span class="log-text">"#);
        match line.kind {
            Kind::Group => out.push_str(CHEVRON),
            Kind::Error => out.push_str(ICON_ERROR),
            Kind::Warning => out.push_str(ICON_WARNING),
            Kind::Notice => out.push_str(ICON_NOTICE),
            _ => {}
        }
        if let Some(label) = line.kind.label() {
            let _ = write!(out, r#"<span class="log-label">{label}</span>"#);
        }
        ansi_html(line.text, out);
        let _ = write!(out, "</span></{tag}>");
    }

    /// `lines` as rows and groups.
    pub fn render(&self, lines: &[String]) -> Rendered {
        let mut rendered = Rendered::default();
        // The open group: its summary row, its body and whether it holds an error.
        let mut group: Option<(String, String, bool)> = None;
        let close = |out: &mut String, group: &mut Option<(String, String, bool)>, ended: bool| {
            if let Some((head, body, failed)) = group.take() {
                let open = failed || (!ended && self.running);
                let unended = if ended { "" } else { " data-unended" };
                let _ = write!(
                    out,
                    r#"<details class="log-group"{}{unended}>{head}<div class="log-group-body">{body}</div></details>"#,
                    if open { " open" } else { "" }
                );
            }
        };
        for (index, raw) in lines.iter().enumerate() {
            let number = self.first + index;
            let line = parse(raw);
            match line.kind {
                Kind::Group => {
                    close(&mut rendered.html, &mut group, true);
                    let mut head = String::new();
                    self.row(&mut head, "summary", number, &line);
                    group = Some((head, String::new(), false));
                }
                Kind::EndGroup => {
                    if group.is_some() {
                        close(&mut rendered.html, &mut group, true);
                    } else if self.live {
                        rendered.html.push_str(r"<span data-endgroup hidden></span>");
                    }
                }
                kind => {
                    if kind == Kind::Error {
                        rendered.last_error = Some(plain(line.text));
                    }
                    match &mut group {
                        Some((_, body, failed)) => {
                            *failed |= kind == Kind::Error;
                            self.row(body, "div", number, &line);
                        }
                        None => self.row(&mut rendered.html, "div", number, &line),
                    }
                }
            }
        }
        close(&mut rendered.html, &mut group, false);
        rendered
    }
}

#[cfg(test)]
#[path = "log_lines_tests.rs"]
mod tests;
