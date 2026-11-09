//! Log windows: what a page asks for, and planning which complete, durable lines of the
//! index it gets, without scanning the whole log or index.

use std::io::{self, Read, SeekFrom};

use super::files::LogFile;

/// What a job page's step view shows at most: the last lines of the step.
pub const STEP_VIEW_LINES: usize = 10_000;
/// … and at most this many bytes of them.
pub const STEP_VIEW_BYTES: usize = 2 * 1024 * 1024;

/// Where a window of log lines starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogRange {
    /// From this line of the task log on (a live tail polls with its last `next_line`).
    From(u64),
    /// The last lines, as many as the limits allow.
    Tail,
}

/// Lines of a task's log, as a page shows them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "independent facts a page shows")]
pub struct LogWindow {
    /// The task-log line number of `lines[0]`.
    pub first_line: u64,
    pub lines: Vec<String>,
    /// Where the next window starts.
    pub next_line: u64,
    /// Lines before `first_line` that this window left out.
    pub more_before: bool,
    /// The task still runs: more lines may come.
    pub live: bool,
    /// The task wrote more than `[actions] max_log_bytes`; later lines were dropped.
    pub truncated: bool,
    /// Retention or the instance log budget removed the log.
    pub expired: bool,
}

/// Maximum retained lines and bytes for one step, with at least one stored line.
#[derive(Debug, Clone, Copy)]
pub struct LogLimits {
    pub lines: usize,
    pub bytes: usize,
}

/// Lines `start..end` (`None`: to the end) of the log, windowed by `range` and limits.
#[derive(Debug, Clone, Copy)]
pub struct WindowRequest {
    pub start: u64,
    pub end: Option<u64>,
    pub range: LogRange,
    pub limits: LogLimits,
}

/// The data extent a window reads and the window it fills.
#[derive(Debug)]
pub struct WindowPlan {
    pub begin: u64,
    pub ends: Vec<u64>,
    pub window: LogWindow,
}

/// Index and data opens and decoder work of reads; never task IDs or log contents.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReadWork {
    pub index_opens: u64,
    pub data_opens: u64,
    pub decoders: u64,
    pub inflated_bytes: u64,
}

/// A reader that counts the bytes it produced (decompressed bytes, for gzip).
pub(crate) struct Inflated<R> {
    pub reader: R,
    pub bytes: u64,
}

impl<R: Read> Read for Inflated<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = self.reader.read(output)?;
        self.bytes = self.bytes.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        Ok(count)
    }
}

pub(crate) fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn offset(index: &mut impl LogFile, line: u64) -> io::Result<u64> {
    index.seek(SeekFrom::Start(line.checked_mul(8).ok_or_else(|| invalid("log index overflow"))?))?;
    let mut bytes = [0; 8];
    index.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

/// A captured complete-entry extent, additionally clamped to committed progress.
/// Appended but not-yet-committed offsets must not appear in a page snapshot.
pub fn durable_extent(index: &mut impl LogFile, lines: u64, bytes: u64) -> io::Result<u64> {
    let mut left = 0;
    let mut right = (index.length()? / 8).min(lines);
    while left < right {
        let middle = left + (right - left) / 2;
        if offset(index, middle)? <= bytes {
            left = middle + 1;
        } else {
            right = middle;
        }
    }
    Ok(left)
}

/// Plans `request` over the first `stored` index entries; lines are never longer than the
/// ingestion cap allows, and the index must grow strictly.
pub fn plan(index: &mut impl LogFile, stored: u64, request: WindowRequest, mut window: LogWindow) -> io::Result<WindowPlan> {
    let end = request.end.map_or(stored, |end| end.min(stored));
    let max = u64::try_from(request.limits.lines.max(1)).unwrap_or(u64::MAX);
    let (first, last) = match request.range {
        LogRange::From(line) => {
            let first = line.clamp(request.start, end.max(request.start));
            (first, end.min(first.saturating_add(max)))
        }
        LogRange::Tail => (end.saturating_sub(max).max(request.start), end),
    };
    window.first_line = first;
    window.next_line = first;
    window.more_before = first > request.start;
    if first >= last {
        return Ok(WindowPlan { begin: 0, ends: Vec::new(), window });
    }
    let mut begin = if first == 0 { 0 } else { offset(index, first - 1)? };
    let count = usize::try_from(last - first).map_err(|_| invalid("log index window exceeds address space"))?;
    let length = count.checked_mul(8).ok_or_else(|| invalid("log index window overflow"))?;
    index.seek(SeekFrom::Start(first.checked_mul(8).ok_or_else(|| invalid("log index overflow"))?))?;
    let mut bytes = vec![0; length];
    index.read_exact(&mut bytes)?;
    let mut ends = Vec::with_capacity(count);
    let mut previous = begin;
    let maximum_line = u64::try_from(rbit_workflow::protocol::MAX_LOG_LINE_BYTES).unwrap_or(u64::MAX).saturating_add(1);
    for chunk in bytes.as_chunks::<8>().0 {
        let end = u64::from_le_bytes(*chunk);
        if end <= previous {
            return Err(invalid("nonmonotonic log index"));
        }
        if end - previous > maximum_line {
            return Err(invalid("indexed line exceeds ingestion cap"));
        }
        ends.push(end);
        previous = end;
    }
    let budget = u64::try_from(request.limits.bytes).unwrap_or(u64::MAX);
    match request.range {
        LogRange::From(_) => {
            let count = ends.iter().take_while(|end| **end - begin <= budget).count().max(1);
            ends.truncate(count);
        }
        LogRange::Tail => {
            let finish = previous;
            let mut skip = 0;
            while skip + 1 < ends.len() && finish - begin > budget {
                begin = ends[skip];
                skip += 1;
            }
            ends.drain(..skip);
            window.first_line += u64::try_from(skip).unwrap_or_default();
        }
    }
    window.next_line = window.first_line + u64::try_from(ends.len()).unwrap_or_default();
    window.more_before = window.first_line > request.start;
    Ok(WindowPlan { begin, ends, window })
}
