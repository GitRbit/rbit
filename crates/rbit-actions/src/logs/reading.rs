//! Reading log windows from the plain or gzip file through the [`TaskLogFiles`] port: a
//! single window, or every window of a job page from one index snapshot and one data
//! descriptor, inflating a gzip log once.

use std::io::{self, Read, Seek, SeekFrom};

use super::{
    files::{LogFile, LogPart, TaskLogFiles, optional},
    window::{Inflated, LogLimits, LogRange, LogWindow, ReadWork, WindowPlan, WindowRequest, durable_extent, invalid, plan},
};

/// Which file holds a log's lines now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Representation {
    /// The plain file, while the task runs.
    Plain,
    /// The gzip file, once finalised.
    Gzip,
    /// Retention or the instance log budget removed the log.
    Expired,
}

/// Only forward on-disk lifecycle transitions can explain a vanished representation.
pub fn retryable(previous: Representation, next: Representation) -> bool {
    use Representation::{Expired, Gzip, Plain};
    matches!((previous, next), (Plain, Gzip | Expired) | (Gzip, Expired))
}

/// A task's committed log progress, as one page read sees it.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub lines: u64,
    pub bytes: u64,
    pub representation: Representation,
    pub live: bool,
    pub truncated: bool,
}

impl Snapshot {
    fn empty(self, start: u64) -> LogWindow {
        LogWindow {
            first_line: start,
            next_line: start,
            live: self.live,
            truncated: self.truncated,
            expired: self.representation == Representation::Expired,
            ..LogWindow::default()
        }
    }
}

/// The end offsets of stored lines `from..to` (fewer when the index is shorter).
fn read_ends(index: &mut impl LogFile, from: u64, to: u64) -> io::Result<Vec<u64>> {
    let mut ends = Vec::new();
    if to <= from {
        return Ok(ends);
    }
    index.seek(SeekFrom::Start(from * 8))?;
    let mut bytes = Vec::new();
    index.take((to - from) * 8).read_to_end(&mut bytes)?;
    ends.extend(bytes.as_chunks::<8>().0.iter().map(|chunk| u64::from_le_bytes(*chunk)));
    Ok(ends)
}

/// Fills `window` with lines of one request, reading the files directly.
pub fn read_window<F: TaskLogFiles>(
    files: &F,
    gzip: bool,
    request: WindowRequest,
    window: &mut LogWindow,
    work: &mut ReadWork,
) -> io::Result<()> {
    let WindowRequest { start, end, range, limits: LogLimits { lines: max_lines, bytes: max_bytes } } = request;
    work.index_opens += 1;
    let Some(mut index) = optional(files.open(LogPart::Index))? else { return Ok(()) };
    let stored = index.length()? / 8;
    let end = end.map_or(stored, |end| end.min(stored));
    let max = u64::try_from(max_lines).unwrap_or(u64::MAX);
    let (first, last) = match range {
        LogRange::From(line) => {
            let first = line.clamp(start, end.max(start));
            (first, end.min(first.saturating_add(max)))
        }
        LogRange::Tail => (end.saturating_sub(max).max(start), end),
    };
    window.first_line = first;
    window.next_line = first;
    if first >= last {
        window.more_before = first > start;
        return Ok(());
    }
    // The end of the line before `first` is where `first` begins.
    let ends = read_ends(&mut index, first.saturating_sub(1), last)?;
    let (begin, ends) =
        if first == 0 { (0, &ends[..]) } else { (ends.first().copied().unwrap_or_default(), ends.get(1..).unwrap_or_default()) };
    let budget = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    let (first, begin, ends) = match range {
        LogRange::From(_) => {
            let count = ends.iter().take_while(|end| **end - begin <= budget).count().max(1).min(ends.len());
            (first, begin, &ends[..count])
        }
        LogRange::Tail => {
            let finish = ends.last().copied().unwrap_or(begin);
            let start_of = |line: usize| if line == 0 { begin } else { ends[line - 1] };
            let mut skip = 0;
            while skip + 1 < ends.len() && finish - start_of(skip) > budget {
                skip += 1;
            }
            (first + u64::try_from(skip).unwrap_or_default(), start_of(skip), &ends[skip..])
        }
    };
    let finish = ends.last().copied().unwrap_or(begin);
    let mut bytes = vec![0u8; usize::try_from(finish - begin).unwrap_or_default()];
    work.data_opens += 1;
    if gzip {
        let file = files.open(LogPart::Gzip)?;
        work.decoders += 1;
        let mut decoder = Inflated { reader: flate2::read::GzDecoder::new(file), bytes: 0 };
        let result = (|| {
            io::copy(&mut (&mut decoder).take(begin), &mut io::sink())?;
            decoder.read_exact(&mut bytes)
        })();
        work.inflated_bytes = work.inflated_bytes.saturating_add(decoder.bytes);
        result?;
    } else {
        let mut log = files.open(LogPart::Log)?;
        log.seek(SeekFrom::Start(begin))?;
        log.read_exact(&mut bytes)?;
    }
    let mut offset = begin;
    for end in ends {
        let from = usize::try_from(offset - begin).unwrap_or_default();
        let to = usize::try_from(*end - begin).unwrap_or_default();
        let line = bytes.get(from..to).unwrap_or_default();
        window.lines.push(String::from_utf8_lossy(line.strip_suffix(b"\n").unwrap_or(line)).into_owned());
        offset = *end;
    }
    window.first_line = first;
    window.next_line = first + u64::try_from(ends.len()).unwrap_or_default();
    window.more_before = first > start;
    Ok(())
}

/// Every window of `requests`, from one index snapshot and one data descriptor.
pub fn read_windows<F: TaskLogFiles>(
    files: &F,
    snapshot: Snapshot,
    requests: &[WindowRequest],
    work: &mut ReadWork,
) -> io::Result<Vec<LogWindow>> {
    let plans = prepare(files, snapshot, requests, work)?;
    read_plans(files, snapshot.representation == Representation::Gzip, plans, work)
}

/// Plans every request against the committed, complete index entries.
pub fn prepare<F: TaskLogFiles>(
    files: &F,
    snapshot: Snapshot,
    requests: &[WindowRequest],
    work: &mut ReadWork,
) -> io::Result<Vec<WindowPlan>> {
    let empty =
        || requests.iter().map(|request| WindowPlan { begin: 0, ends: Vec::new(), window: snapshot.empty(request.start) }).collect();
    if snapshot.representation == Representation::Expired || requests.is_empty() {
        return Ok(empty());
    }
    work.index_opens += 1;
    let Some(mut index) = optional(files.open(LogPart::Index))? else { return Ok(empty()) };
    let extent = durable_extent(&mut index, snapshot.lines, snapshot.bytes)?;
    let plans =
        requests.iter().map(|request| plan(&mut index, extent, *request, snapshot.empty(request.start))).collect::<io::Result<Vec<_>>>()?;
    if plans.iter().flat_map(|plan| &plan.ends).any(|end| *end > snapshot.bytes) {
        return Err(invalid("log index exceeds committed bytes"));
    }
    // The index is no longer read after planning; append/finalize cannot move this extent.
    drop(index);
    Ok(plans)
}

/// Opens the data file once and fills every plan.
pub fn read_plans<F: TaskLogFiles>(files: &F, gzip: bool, plans: Vec<WindowPlan>, work: &mut ReadWork) -> io::Result<Vec<LogWindow>> {
    if plans.iter().all(|plan| plan.ends.is_empty()) {
        return Ok(plans.into_iter().map(|plan| plan.window).collect());
    }
    work.data_opens += 1;
    let file = files.open(if gzip { LogPart::Gzip } else { LogPart::Log })?;
    read_data(file, gzip, plans, work)
}

/// Fills `plans` from an opened data file; a gzip file is inflated once, forward only.
pub fn read_data<R: Read + Seek>(mut file: R, gzip: bool, mut plans: Vec<WindowPlan>, work: &mut ReadWork) -> io::Result<Vec<LogWindow>> {
    let mut order: Vec<_> = plans.iter().enumerate().filter(|(_, plan)| !plan.ends.is_empty()).map(|(index, _)| index).collect();
    if order.is_empty() {
        return Ok(plans.into_iter().map(|plan| plan.window).collect());
    }
    if gzip {
        work.decoders += 1;
        let mut decoder = Inflated { reader: flate2::read::GzDecoder::new(file), bytes: 0 };
        order.sort_by_key(|index| plans[*index].begin);
        let result = forward(&mut decoder, &mut plans, &order);
        work.inflated_bytes = decoder.bytes;
        result?;
    } else {
        for index in order {
            let plan = &mut plans[index];
            file.seek(SeekFrom::Start(plan.begin))?;
            let finish = *plan.ends.last().ok_or_else(|| invalid("empty log plan"))?;
            let mut bytes = vec![0; size(finish - plan.begin)?];
            file.read_exact(&mut bytes)?;
            fill(plan, &bytes)?;
        }
    }
    Ok(plans.into_iter().map(|plan| plan.window).collect())
}

fn size(value: u64) -> io::Result<usize> {
    usize::try_from(value).map_err(|_| invalid("log window exceeds address space"))
}

fn fill(plan: &mut WindowPlan, bytes: &[u8]) -> io::Result<()> {
    let mut begin = 0;
    for end in &plan.ends {
        let end = size(*end - plan.begin)?;
        let line = bytes.get(begin..end).ok_or_else(|| invalid("invalid line extent"))?;
        plan.window.lines.push(String::from_utf8_lossy(line.strip_suffix(b"\n").unwrap_or(line)).into_owned());
        begin = end;
    }
    Ok(())
}

/// Retain only look-ahead overlapping a later selected window, never the full log.
fn forward(reader: &mut impl Read, plans: &mut [WindowPlan], order: &[usize]) -> io::Result<()> {
    let mut buffer = Vec::new();
    let mut base = 0;
    for index in order {
        let plan = &mut plans[*index];
        let position = base + u64::try_from(buffer.len()).map_err(|_| invalid("log window exceeds address space"))?;
        if plan.begin > position {
            let skip = plan.begin - position;
            if io::copy(&mut (&mut *reader).take(skip), &mut io::sink())? != skip {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            buffer.clear();
            base = plan.begin;
        } else {
            let discard = size(plan.begin - base)?;
            buffer.drain(..discard);
            base = plan.begin;
        }
        let needed = size(*plan.ends.last().ok_or_else(|| invalid("empty log plan"))? - base)?;
        if buffer.len() < needed {
            let previous = buffer.len();
            buffer.resize(needed, 0);
            reader.read_exact(&mut buffer[previous..])?;
        }
        fill(plan, &buffer[..needed])?;
    }
    Ok(())
}
