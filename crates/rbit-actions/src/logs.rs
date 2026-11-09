//! Task log rules (§5.5, §11.10 and §12.6 of the Actions design): the masking pass, how an
//! append by line offset is accepted, how a line is stored, and the byte budget. Files,
//! the line index, gzip, retention and reads stay in the composition's adapter.
//!
//! * An append carries lines `from…` of the task's log, at most [`MAX_LINES_PER_CALL`]
//!   ([`split_body`]). Lines the server already has are dropped, so a resend overlaps
//!   harmlessly; a gap names the line to resend from ([`accept_append`]).
//! * A stored line is masked, loses a trailing `\r` and is cut to the protocol's line limit
//!   with a marker ([`clean_line`]).
//! * Beyond `[actions] max_log_bytes`, lines are counted but dropped after one marker line;
//!   every stored line's end offset is indexed ([`layout`]).
//! * The masker replaces every secret value, and every line of a multi-line one, of at
//!   least [`MIN_MASKED_CHARS`] characters, longest match first ([`Masker`]).

mod masking;
#[cfg(test)]
mod tests;

pub use masking::{MASK, MIN_MASKED_CHARS, Masker};

/// Most lines one append may carry.
pub const MAX_LINES_PER_CALL: usize = 10_000;

/// An append body as lines: split on `\n`, a final newline ending the last line rather than
/// starting an empty one. `Err` when it carries more than [`MAX_LINES_PER_CALL`].
pub fn split_body(text: &str) -> Result<Vec<&str>, String> {
    let mut lines: Vec<&str> = if text.is_empty() { Vec::new() } else { text.split('\n').collect() };
    if text.ends_with('\n') {
        lines.pop();
    }
    if lines.len() > MAX_LINES_PER_CALL {
        return Err(format!("send at most {MAX_LINES_PER_CALL} lines per call"));
    }
    Ok(lines)
}

/// What an append of `count` lines starting at line `from` does to a log of `current` lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Append {
    /// Lines are missing before `from`: resend from `current`.
    Gap { current: u64 },
    /// Every line is already stored; the log stays at `current`.
    Nothing { current: u64 },
    /// Store the lines after the first `skip`, which the log already has.
    Store { skip: usize },
}

/// Decides an append of `count` lines from line `from` to a log of `current` lines.
pub fn accept_append(from: u64, current: u64, count: usize) -> Append {
    if from > current {
        return Append::Gap { current };
    }
    let skip = usize::try_from(current - from).unwrap_or(usize::MAX);
    if skip >= count { Append::Nothing { current } } else { Append::Store { skip } }
}

/// A line as stored: masked, without a trailing `\r`, at most `max_bytes` (the protocol's
/// line limit), cut on a character boundary and marked when longer.
pub fn clean_line(masker: &Masker, line: &str, max_bytes: usize) -> String {
    const MARKER: &str = " [line truncated]";
    let mut line = masker.mask(line.strip_suffix('\r').unwrap_or(line));
    if line.len() > max_bytes {
        let mut end = max_bytes - MARKER.len();
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        line.truncate(end);
        line.push_str(MARKER);
    }
    line
}

/// How much of a task's log is written: the protocol's line count, the bytes stored and
/// whether later lines were dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogProgress {
    pub lines: u64,
    pub bytes: u64,
    pub truncated: bool,
}

/// What appending lines writes: the log bytes, the index entries (one little-endian `u64`
/// per stored line, where it ends) and the progress after them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub text: Vec<u8>,
    pub ends: Vec<u8>,
    pub progress: LogProgress,
}

/// Lays `lines` out after `progress` within `max_bytes`: every line counts; once the next
/// stored line would pass the budget, one marker line is stored and the rest are dropped.
pub fn layout(mut progress: LogProgress, lines: &[String], max_bytes: u64) -> Layout {
    let mut text = Vec::new();
    let mut ends = Vec::new();
    for line in lines {
        progress.lines += 1;
        if progress.truncated {
            continue;
        }
        let size = u64::try_from(line.len()).unwrap_or(u64::MAX).saturating_add(1);
        if progress.bytes.saturating_add(size) > max_bytes {
            let marker = format!("rbit: the log was cut here: a task may write at most {max_bytes} bytes\n");
            progress.bytes += u64::try_from(marker.len()).unwrap_or_default();
            text.extend_from_slice(marker.as_bytes());
            progress.truncated = true;
            continue;
        }
        text.extend_from_slice(line.as_bytes());
        text.push(b'\n');
        progress.bytes += size;
        ends.extend_from_slice(&progress.bytes.to_le_bytes());
    }
    Layout { text, ends, progress }
}
