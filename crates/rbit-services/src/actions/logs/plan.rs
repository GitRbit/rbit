//! Select complete durable indexed lines without scanning the full log/index.
use super::{LogLimits, LogRange, LogWindow};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
};

#[derive(Debug, Clone, Copy)]
pub(super) struct WindowRequest {
    pub start: u64,
    pub end: Option<u64>,
    pub range: LogRange,
    pub limits: LogLimits,
}

#[derive(Debug)]
pub(super) struct WindowPlan {
    pub begin: u64,
    pub ends: Vec<u64>,
    pub window: LogWindow,
}

pub(super) fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn offset(index: &mut File, line: u64) -> io::Result<u64> {
    index.seek(SeekFrom::Start(line.checked_mul(8).ok_or_else(|| invalid("log index overflow"))?))?;
    let mut bytes = [0; 8];
    index.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

/// A captured complete-entry extent, additionally clamped to committed progress.
/// Appended but not-yet-committed offsets must not appear in a page snapshot.
pub(super) fn durable_extent(index: &mut File, lines: u64, bytes: u64) -> io::Result<u64> {
    let mut left = 0;
    let mut right = (index.metadata()?.len() / 8).min(lines);
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

pub(super) fn plan(index: &mut File, stored: u64, request: WindowRequest, mut window: LogWindow) -> io::Result<WindowPlan> {
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
