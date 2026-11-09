//! Single-window reads through `rbit_actions::logs::read_window`; no authorization or database access.
use super::{
    LogLimits, LogRange, LogWindow,
    plan::WindowRequest,
    types::LogFiles,
    work::{ReadWork, record},
};

/// Fills `window` with lines `start..end` of the log, chosen by `range` and the limits.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_window(
    files: &LogFiles,
    gzip: bool,
    start: u64,
    end: Option<u64>,
    range: LogRange,
    max_lines: usize,
    max_bytes: usize,
    window: &mut LogWindow,
) -> std::io::Result<()> {
    let request = WindowRequest { start, end, range, limits: LogLimits { lines: max_lines, bytes: max_bytes } };
    let mut work = ReadWork::default();
    let result = read_window_counted(files, gzip, request, window, &mut work);
    record(&work);
    result
}

pub(super) fn read_window_counted(
    files: &LogFiles,
    gzip: bool,
    request: WindowRequest,
    window: &mut LogWindow,
    work: &mut ReadWork,
) -> std::io::Result<()> {
    rbit_actions::logs::read_window(files, gzip, request, window, work)
}
