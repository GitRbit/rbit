//! Annotations: the `::error`, `::warning` and `::notice` workflow commands, and the
//! problems a problem matcher found, as a run shows them beside its jobs.
//!
//! The runner prints each as one log line, `##[kind]message`, followed by ` (params)`
//! when the command had any (`file=a.rs,line=3,title=…`; see rbit-runner's `commands` and
//! `matchers`). The server reads them from the lines it stores, after masking, and keeps
//! at most [`MAX_ANNOTATIONS_PER_TASK`] per task, the first ones, as GitHub keeps a bounded
//! number per job.

/// Most annotations kept for one task (one job attempt).
pub const MAX_ANNOTATIONS_PER_TASK: usize = 50;
/// Longest annotation message kept, in bytes; longer ones are cut at a character boundary.
pub const MAX_ANNOTATION_BYTES: usize = 4096;

/// How serious an annotation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnnotationLevel {
    Error,
    Warning,
    Notice,
}

impl AnnotationLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Notice => "notice",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "error" => Some(Self::Error),
            "warning" => Some(Self::Warning),
            "notice" => Some(Self::Notice),
            _ => None,
        }
    }
}

/// One annotation line as stored: its level and the line's text after the marker, with
/// its parameters, cut to [`MAX_ANNOTATION_BYTES`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationLine {
    pub level: AnnotationLevel,
    pub text: String,
}

/// The annotation `line` (a stored log line) carries, if any. The runner starts each line
/// with its UTC time (`2026-11-10T12:00:01.123Z `); older runners did not.
pub fn annotation_line(line: &str) -> Option<AnnotationLine> {
    let rest = without_time(line).strip_prefix("##[")?;
    let (kind, text) = rest.split_once(']')?;
    let level = AnnotationLevel::parse(kind)?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(AnnotationLine { level, text: cut(text, MAX_ANNOTATION_BYTES).to_owned() })
}

/// The annotations among `lines`, the first of which is task-log line `first` (from 0),
/// with their line numbers; at most `room` of them.
pub fn annotation_lines<'a>(lines: impl IntoIterator<Item = &'a str>, first: u64, room: usize) -> Vec<(u64, AnnotationLine)> {
    lines
        .into_iter()
        .zip(first..)
        .filter_map(|(line, number)| annotation_line(line).map(|annotation| (number, annotation)))
        .take(room)
        .collect()
}

/// An annotation's text taken apart: the message and the parameters the runner appended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnnotationText<'a> {
    pub message: &'a str,
    pub title: Option<&'a str>,
    pub file: Option<&'a str>,
    pub line: Option<&'a str>,
}

/// Splits a stored annotation text into its message and the parameters of its trailing
/// ` (key=value,…)`, when every part of that is a parameter GitHub defines; otherwise the
/// whole text is the message.
pub fn annotation_text(text: &str) -> AnnotationText<'_> {
    let plain = AnnotationText { message: text, ..AnnotationText::default() };
    let Some(open) = text.strip_suffix(')').and_then(|inner| inner.rfind(" (")) else { return plain };
    let params = &text[open + 2..text.len() - 1];
    let mut parsed = AnnotationText { message: text[..open].trim_end(), ..AnnotationText::default() };
    let mut last: Option<&str> = None;
    let mut start = 0;
    // Values may hold commas (a title), so a part without `=` continues the previous value.
    let mut parts: Vec<(&str, usize, usize)> = Vec::new();
    for (index, part) in params.split(',').enumerate() {
        let offset = start;
        start += part.len() + 1;
        match part.split_once('=') {
            Some((key, _)) if KEYS.contains(&key.trim()) => {
                parts.push((key.trim(), offset + key.len() + 1, offset + part.len()));
                last = Some(key);
            }
            _ if index > 0 && last.is_some() => {
                if let Some(previous) = parts.last_mut() {
                    previous.2 = offset + part.len();
                }
            }
            _ => return plain,
        }
    }
    for (key, from, to) in parts {
        let value = params[from..to].trim();
        match key {
            "title" => parsed.title = Some(value),
            "file" => parsed.file = Some(value),
            "line" => parsed.line = Some(value),
            _ => {}
        }
    }
    if parsed.message.is_empty() { plain } else { parsed }
}

/// The parameters of an annotation command (and a problem matcher's `code`).
const KEYS: [&str; 8] = ["file", "line", "endLine", "col", "endColumn", "title", "code", "column"];

/// `line` without the runner's leading `YYYY-MM-DDTHH:MM:SS[.fraction]Z ` time, the same
/// shape rbit-web's job log viewer (`views::actions::log_lines`) takes apart.
fn without_time(line: &str) -> &str {
    let bytes = line.as_bytes();
    let digits = |from: usize, to: usize| bytes.get(from..to).is_some_and(|part| part.iter().all(u8::is_ascii_digit));
    let shaped = digits(0, 4)
        && bytes.get(4) == Some(&b'-')
        && digits(5, 7)
        && bytes.get(7) == Some(&b'-')
        && digits(8, 10)
        && bytes.get(10) == Some(&b'T')
        && digits(11, 13)
        && bytes.get(13) == Some(&b':')
        && digits(14, 16)
        && bytes.get(16) == Some(&b':')
        && digits(17, 19);
    if !shaped {
        return line;
    }
    let mut end = 19;
    if bytes.get(end) == Some(&b'.') {
        let fraction = bytes[end + 1..].iter().take_while(|byte| byte.is_ascii_digit()).count();
        if !(1..=9).contains(&fraction) {
            return line;
        }
        end += 1 + fraction;
    }
    if bytes.get(end) == Some(&b'Z') && bytes.get(end + 1) == Some(&b' ') { &line[end + 2..] } else { line }
}

/// The longest prefix of `text` of at most `max` bytes ending at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotation_lines_are_found_with_their_numbers() {
        let lines = ["compiling", "##[error]it broke (file=src/a.rs,line=3)", "##[group]Run", "##[warning]careful", "##[notice] "];
        let found = annotation_lines(lines, 10, 10);
        assert_eq!(found.len(), 2, "groups and empty messages are no annotations");
        assert_eq!(found[0], (11, AnnotationLine { level: AnnotationLevel::Error, text: "it broke (file=src/a.rs,line=3)".into() }));
        assert_eq!(found[1].0, 13);
        assert_eq!(found[1].1.level, AnnotationLevel::Warning);
        assert_eq!(annotation_lines(lines, 0, 1).len(), 1, "no more than there is room for");
        assert!(annotation_line("not ##[error] at the start").is_none());
        assert!(annotation_line("##[debug]details").is_none());
    }

    #[test]
    fn timestamped_lines_carry_their_annotation() {
        let annotation = annotation_line("2026-11-10T12:00:01.123Z ##[error]it broke (file=a.rs,line=3)").unwrap();
        assert_eq!(annotation, AnnotationLine { level: AnnotationLevel::Error, text: "it broke (file=a.rs,line=3)".into() });
        assert_eq!(annotation_line("2026-11-10T12:00:01Z ##[notice]fyi").unwrap().level, AnnotationLevel::Notice);
        assert!(annotation_line("2026-11-10T12:00:01.123Z compiling").is_none());
        assert!(annotation_line("2026-11-10 ##[error]not a runner time").is_none());
    }

    #[test]
    fn long_messages_are_cut_at_a_character_boundary() {
        let line = format!("##[error]{}", "é".repeat(MAX_ANNOTATION_BYTES));
        let annotation = annotation_line(&line).unwrap();
        assert!(annotation.text.len() <= MAX_ANNOTATION_BYTES && annotation.text.chars().all(|c| c == 'é'));
    }

    #[test]
    fn parameters_are_taken_apart_only_when_they_are_parameters() {
        let text = annotation_text("it broke (file=src/a.rs,line=3,col=7)");
        assert_eq!(text, AnnotationText { message: "it broke", title: None, file: Some("src/a.rs"), line: Some("3") });
        let titled = annotation_text("fyi (title=a, b and c,file=x)");
        assert_eq!((titled.message, titled.title, titled.file), ("fyi", Some("a, b and c"), Some("x")));
        assert_eq!(annotation_text("call f(x)").message, "call f(x)");
        assert_eq!(annotation_text("values (a=1)").message, "values (a=1)", "unknown keys are part of the message");
        assert_eq!(annotation_text(" (file=a)").message, " (file=a)", "an annotation keeps a message");
        assert_eq!(annotation_text("plain").message, "plain");
    }
}
