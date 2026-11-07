//! One index snapshot and one data descriptor for all selected task windows.
use super::{
    LogWindow,
    plan::{WindowPlan, WindowRequest, durable_extent, invalid, plan},
    types::LogFiles,
    work::{Inflated, ReadWork},
};
use rbit_core::actions::LogState;
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
};

#[derive(Debug, Clone, Copy)]
pub(super) struct LogSnapshot {
    pub lines: u64,
    pub bytes: u64,
    pub state: LogState,
    pub live: bool,
    pub truncated: bool,
}

impl LogSnapshot {
    fn empty(self, start: u64) -> LogWindow {
        LogWindow {
            first_line: start,
            next_line: start,
            live: self.live,
            truncated: self.truncated,
            expired: self.state == LogState::Expired,
            ..LogWindow::default()
        }
    }
}

pub(super) fn read_windows(
    files: &LogFiles,
    snapshot: LogSnapshot,
    requests: &[WindowRequest],
    work: &mut ReadWork,
) -> io::Result<Vec<LogWindow>> {
    let plans = prepare(files, snapshot, requests, work)?;
    read_plans(files, snapshot.state == LogState::Final, plans, work)
}

pub(super) fn prepare(
    files: &LogFiles,
    snapshot: LogSnapshot,
    requests: &[WindowRequest],
    work: &mut ReadWork,
) -> io::Result<Vec<WindowPlan>> {
    let empty =
        || requests.iter().map(|request| WindowPlan { begin: 0, ends: Vec::new(), window: snapshot.empty(request.start) }).collect();
    if snapshot.state == LogState::Expired || requests.is_empty() {
        return Ok(empty());
    }
    work.index_opens += 1;
    let mut index = match File::open(&files.index) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(empty()),
        Err(error) => return Err(error),
    };
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

pub(super) fn read_plans(files: &LogFiles, gzip: bool, plans: Vec<WindowPlan>, work: &mut ReadWork) -> io::Result<Vec<LogWindow>> {
    if plans.iter().all(|plan| plan.ends.is_empty()) {
        return Ok(plans.into_iter().map(|plan| plan.window).collect());
    }
    work.data_opens += 1;
    let file = File::open(if gzip { &files.gzip } else { &files.log })?;
    read_data(file, gzip, plans, work)
}

pub(super) fn read_data(mut file: File, gzip: bool, mut plans: Vec<WindowPlan>, work: &mut ReadWork) -> io::Result<Vec<LogWindow>> {
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
