//! The excerpt of a failure digest: the last lines of the step that ended a job, cleaned so
//! an agent reads the failure and not the noise around it.
//!
//! A stored line starts with the runner's timestamp (`2026-11-11T10:00:00.123Z `) and may
//! carry ANSI colour codes; both go. Runs of the same line are folded into one with a
//! count, so a loop that printed one warning a thousand times costs one line. The excerpt
//! says how many lines it shows of how many the step wrote, so a reader knows it is cut.

/// Lines an excerpt keeps by default: enough for a compiler error with its context.
pub const EXCERPT_LINES: usize = 30;
/// Longest line an excerpt keeps, in characters; longer ones are cut with `…`.
pub const EXCERPT_LINE_CHARS: usize = 400;

/// The last lines of a step, cleaned and folded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excerpt {
    pub lines: Vec<String>,
    /// Raw lines the excerpt covers (before folding).
    pub covered: usize,
}

/// `line` without its leading runner timestamp and without ANSI escape sequences, cut to
/// [`EXCERPT_LINE_CHARS`].
pub fn readable_line(line: &str) -> String {
    let line = strip_timestamp(line);
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            // CSI: ESC [ parameters final-byte.
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    if out.chars().count() > EXCERPT_LINE_CHARS {
        out = out.chars().take(EXCERPT_LINE_CHARS - 1).collect::<String>() + "…";
    }
    out.trim_end().to_owned()
}

/// `line` after an RFC 3339 UTC timestamp and the space that follows it, when it has one.
fn strip_timestamp(line: &str) -> &str {
    let bytes = line.as_bytes();
    let stamp = bytes.len() > 21
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[..4].iter().all(u8::is_ascii_digit);
    if !stamp {
        return line;
    }
    match line.find("Z ") {
        Some(end) if end <= 30 => &line[end + 2..],
        _ => line,
    }
}

/// The last `keep` lines of `lines`, cleaned, with repeats folded (`… (repeated 12 times)`).
/// Blank lines at the end are dropped first: they say nothing.
pub fn excerpt(lines: &[String], keep: usize) -> Excerpt {
    let cleaned: Vec<String> = lines.iter().map(|line| readable_line(line)).collect();
    let end = cleaned.iter().rposition(|line| !line.is_empty()).map_or(0, |last| last + 1);
    let mut folded: Vec<(String, usize)> = Vec::new();
    for line in &cleaned[..end] {
        match folded.last_mut() {
            Some((previous, count)) if previous == line => *count += 1,
            _ => folded.push((line.clone(), 1)),
        }
    }
    let start = folded.len().saturating_sub(keep.max(1));
    let kept = &folded[start..];
    let covered = kept.iter().map(|(_, count)| count).sum();
    let lines =
        kept.iter().map(|(line, count)| if *count > 1 { format!("{line} … (repeated {count} times)") } else { line.clone() }).collect();
    Excerpt { lines, covered }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_and_colours_go() {
        assert_eq!(readable_line("2026-11-11T21:39:47.129Z \u{1b}[31merror\u{1b}[0m: it broke"), "error: it broke");
        assert_eq!(readable_line("no stamp here"), "no stamp here");
        assert_eq!(readable_line("2026-11-11 is a date, not a stamp"), "2026-11-11 is a date, not a stamp");
        let long = "x".repeat(EXCERPT_LINE_CHARS + 10);
        assert_eq!(readable_line(&long).chars().count(), EXCERPT_LINE_CHARS);
    }

    #[test]
    fn the_excerpt_keeps_the_end_and_folds_repeats() {
        let mut lines: Vec<String> = (0..100).map(|n| format!("2026-11-11T10:00:00.000Z line {n}")).collect();
        lines.extend(std::iter::repeat_n("2026-11-11T10:00:01.000Z warning: again".to_owned(), 50));
        lines.push("2026-11-11T10:00:02.000Z error: the end".to_owned());
        lines.push(String::new());
        let kept = excerpt(&lines, 3);
        assert_eq!(kept.lines, ["line 99", "warning: again … (repeated 50 times)", "error: the end"]);
        assert_eq!(kept.covered, 52);
        assert_eq!(excerpt(&[], 30).lines, Vec::<String>::new());
    }
}
