//! One index snapshot and one data descriptor for all selected task windows, read by
//! `rbit_actions::logs`; this maps the stored log state onto its representation.
use super::{LogWindow, plan::WindowRequest, types::LogFiles, work::ReadWork};
use rbit_actions::logs::{Representation, Snapshot};
use rbit_core::actions::LogState;
use std::io;

#[derive(Debug, Clone, Copy)]
pub(super) struct LogSnapshot {
    pub lines: u64,
    pub bytes: u64,
    pub state: LogState,
    pub live: bool,
    pub truncated: bool,
}

/// Which file holds a log in `state`.
pub(super) fn representation(state: LogState) -> Representation {
    match state {
        LogState::Live => Representation::Plain,
        LogState::Final => Representation::Gzip,
        LogState::Expired => Representation::Expired,
    }
}

impl LogSnapshot {
    fn rules(self) -> Snapshot {
        Snapshot {
            lines: self.lines,
            bytes: self.bytes,
            representation: representation(self.state),
            live: self.live,
            truncated: self.truncated,
        }
    }
}

pub(super) fn read_windows(
    files: &LogFiles,
    snapshot: LogSnapshot,
    requests: &[WindowRequest],
    work: &mut ReadWork,
) -> io::Result<Vec<LogWindow>> {
    rbit_actions::logs::read_windows(files, snapshot.rules(), requests, work)
}

#[cfg(test)]
pub(super) fn prepare(
    files: &LogFiles,
    snapshot: LogSnapshot,
    requests: &[WindowRequest],
    work: &mut ReadWork,
) -> io::Result<Vec<super::plan::WindowPlan>> {
    rbit_actions::logs::prepare(files, snapshot.rules(), requests, work)
}

#[cfg(test)]
pub(super) fn read_plans(
    files: &LogFiles,
    gzip: bool,
    plans: Vec<super::plan::WindowPlan>,
    work: &mut ReadWork,
) -> io::Result<Vec<LogWindow>> {
    rbit_actions::logs::read_plans(files, gzip, plans, work)
}

#[cfg(test)]
pub(super) fn read_data(
    file: std::fs::File,
    gzip: bool,
    plans: Vec<super::plan::WindowPlan>,
    work: &mut ReadWork,
) -> io::Result<Vec<LogWindow>> {
    rbit_actions::logs::read_data(file, gzip, plans, work)
}
