use super::{
    LogLimits, LogRange, LogWindow,
    append::append_lines,
    archive::compress,
    batch_reader::{LogSnapshot, prepare, read_data, read_plans, read_windows},
    plan::WindowRequest,
    reader::read_window_counted,
    types::{LogFiles, Progress},
    work::ReadWork,
};
use rbit_core::{
    RepoId,
    actions::{LogState, TaskId},
};
use std::{
    fs::{File, OpenOptions},
    io::Write,
};

fn fixture(lines: &[String]) -> (tempfile::TempDir, LogFiles, LogSnapshot) {
    let directory = tempfile::tempdir().unwrap();
    let files = LogFiles::new(directory.path(), RepoId(1), TaskId(2));
    let progress = append_lines(&files, Progress { lines: 0, bytes: 0, truncated: false }, lines, u64::MAX, true).unwrap();
    let snapshot = LogSnapshot { lines: progress.lines, bytes: progress.bytes, state: LogState::Live, live: true, truncated: false };
    (directory, files, snapshot)
}

fn request(start: u64, end: Option<u64>, range: LogRange, lines: usize, bytes: usize) -> WindowRequest {
    WindowRequest { start, end, range, limits: LogLimits { lines, bytes } }
}

fn baseline(files: &LogFiles, snapshot: LogSnapshot, requests: &[WindowRequest]) -> (Vec<LogWindow>, ReadWork) {
    let mut work = ReadWork::default();
    let windows = requests
        .iter()
        .map(|request| {
            let mut window = LogWindow {
                first_line: request.start,
                next_line: request.start,
                live: snapshot.live,
                truncated: snapshot.truncated,
                ..LogWindow::default()
            };
            read_window_counted(files, snapshot.state == LogState::Final, *request, &mut window, &mut work).unwrap();
            window
        })
        .collect();
    (windows, work)
}

#[test]
fn batched_windows_equal_legacy_for_ranges_budgets_utf8_overlap_and_original_order() {
    let lines = ["", "\u{e9}\u{1f642} <untrusted>", "\u{1b}[31mred\u{1b}[0m", "##[error]a & b", "tail", "long"].map(str::to_owned);
    let (_directory, files, mut snapshot) = fixture(&lines);
    let mut requests = Vec::new();
    for (start, end) in [(0, None), (1, Some(4)), (2, Some(2)), (99, None), (0, Some(0))] {
        for range in [LogRange::Tail, LogRange::From(0), LogRange::From(2), LogRange::From(99)] {
            for bytes in [0, 1, 7, 1024] {
                for lines in [1, 2, 100] {
                    requests.push(request(start, end, range, lines, bytes));
                }
            }
        }
    }
    requests.reverse();
    requests.extend([request(0, None, LogRange::Tail, 100, 1024); 3]);
    for state in [LogState::Live, LogState::Final] {
        if state == LogState::Final {
            compress(&files).unwrap();
            snapshot.state = state;
            snapshot.live = false;
        }
        let (expected, _) = baseline(&files, snapshot, &requests);
        let mut work = ReadWork::default();
        assert_eq!(read_windows(&files, snapshot, &requests, &mut work).unwrap(), expected);
        assert_eq!((work.index_opens, work.data_opens), (1, 1));
        assert_eq!(work.decoders, u64::from(state == LogState::Final));
    }
}

#[test]
fn one_pass_inflation_and_handle_counts_replace_repeated_prefix_reads_at_512_steps() {
    let lines: Vec<_> = (0..2048).map(|index| format!("line {index}: {}", "x".repeat(80))).collect();
    let (_directory, files, mut snapshot) = fixture(&lines);
    compress(&files).unwrap();
    snapshot.state = LogState::Final;
    snapshot.live = false;
    let requests: Vec<_> = (0..512).map(|step| request(step * 4, Some(step * 4 + 4), LogRange::Tail, 1000, 256 * 1024)).collect();
    let (expected, legacy) = baseline(&files, snapshot, &requests);
    let mut batched = ReadWork::default();
    assert_eq!(read_windows(&files, snapshot, &requests, &mut batched).unwrap(), expected);
    assert_eq!((legacy.index_opens, legacy.data_opens, legacy.decoders), (512, 512, 512));
    assert_eq!((batched.index_opens, batched.data_opens, batched.decoders), (1, 1, 1));
    assert_eq!(batched.inflated_bytes, snapshot.bytes);
    assert!(legacy.inflated_bytes > 200 * batched.inflated_bytes);
}

#[test]
fn complete_durable_extent_excludes_midappend_and_partial_index_entries() {
    let (_directory, files, snapshot) = fixture(&["kept".into()]);
    append_lines(
        &files,
        Progress { lines: snapshot.lines, bytes: snapshot.bytes, truncated: false },
        &["not committed".into()],
        1024,
        false,
    )
    .unwrap();
    OpenOptions::new().append(true).open(&files.index).unwrap().write_all(&[0xFF, 0xFF, 0xFF]).unwrap();
    let requests = [request(0, None, LogRange::Tail, 1000, 1024)];
    let mut work = ReadWork::default();
    let windows = read_windows(&files, snapshot, &requests, &mut work).unwrap();
    assert_eq!(windows[0].lines, ["kept"]);
    assert_eq!(windows[0].next_line, 1);
    let ahead_lines = LogSnapshot { lines: 2, ..snapshot };
    assert_eq!(read_windows(&files, ahead_lines, &requests, &mut work).unwrap(), windows);
}

#[test]
fn planned_extent_stays_fixed_across_append_and_opened_handles_survive_retention() {
    let (_directory, files, snapshot) = fixture(&["before".into()]);
    let requests = [request(0, None, LogRange::Tail, 1000, 1024)];
    let mut work = ReadWork::default();
    let plans = prepare(&files, snapshot, &requests, &mut work).unwrap();
    append_lines(&files, Progress { lines: snapshot.lines, bytes: snapshot.bytes, truncated: false }, &["after".into()], 1024, false)
        .unwrap();
    let file = File::open(&files.log).unwrap();
    std::fs::remove_file(&files.log).unwrap();
    std::fs::remove_file(&files.index).unwrap();
    let windows = read_data(file, false, plans, &mut work).unwrap();
    assert_eq!(windows[0].lines, ["before"]);
    assert_eq!(windows[0].next_line, 1);
}

#[test]
fn representation_transition_is_retryable_but_not_misreported_as_expiry() {
    let (_directory, files, snapshot) = fixture(&["retained".into()]);
    let requests = [request(0, None, LogRange::Tail, 1000, 1024)];
    let mut work = ReadWork::default();
    let plans = prepare(&files, snapshot, &requests, &mut work).unwrap();
    compress(&files).unwrap();
    assert_eq!(read_plans(&files, false, plans, &mut work).unwrap_err().kind(), std::io::ErrorKind::NotFound);
    let final_snapshot = LogSnapshot { state: LogState::Final, live: false, ..snapshot };
    let windows = read_windows(&files, final_snapshot, &requests, &mut work).unwrap();
    assert_eq!(windows[0].lines, ["retained"]);
    assert!(!windows[0].expired);
}

#[test]
fn expiry_and_empty_requests_do_no_file_io_and_truncation_keeps_indexed_cursor() {
    let (_directory, files, snapshot) = fixture(&["one".into()]);
    let requests = [request(0, None, LogRange::Tail, 1000, 1024)];
    let mut work = ReadWork::default();
    let expired = LogSnapshot { state: LogState::Expired, live: false, ..snapshot };
    assert!(read_windows(&files, expired, &requests, &mut work).unwrap()[0].expired);
    assert!(read_windows(&files, snapshot, &[], &mut work).unwrap().is_empty());
    assert_eq!(work, ReadWork::default());
    let progress = append_lines(
        &files,
        Progress { lines: snapshot.lines, bytes: snapshot.bytes, truncated: false },
        &["dropped".into()],
        snapshot.bytes,
        false,
    )
    .unwrap();
    let truncated = LogSnapshot { lines: progress.lines, bytes: progress.bytes, truncated: true, ..snapshot };
    let windows = read_windows(&files, truncated, &requests, &mut work).unwrap();
    assert!(windows[0].truncated && !windows[0].expired);
    assert_eq!(windows[0].next_line, 1);
    assert_eq!(windows[0].lines, ["one"]);
}

#[test]
fn oversized_single_line_keeps_legacy_overshoot_and_corrupt_index_fails_bounded() {
    let line = "\u{e9}".repeat(rbit_workflow::protocol::MAX_LOG_LINE_BYTES / 2);
    let (_directory, files, snapshot) = fixture(std::slice::from_ref(&line));
    let requests = [request(0, None, LogRange::Tail, 1000, 1)];
    let mut work = ReadWork::default();
    assert_eq!(read_windows(&files, snapshot, &requests, &mut work).unwrap()[0].lines, [line]);
    std::fs::write(&files.index, [9u64.to_le_bytes(), 8u64.to_le_bytes()].concat()).unwrap();
    let corrupt = LogSnapshot { lines: 2, ..snapshot };
    assert_eq!(read_windows(&files, corrupt, &requests, &mut work).unwrap_err().kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn only_forward_persisted_lifecycle_changes_permit_the_single_retry() {
    use super::batch::retryable;
    assert!(retryable(LogState::Live, LogState::Final));
    assert!(retryable(LogState::Live, LogState::Expired));
    assert!(retryable(LogState::Final, LogState::Expired));
    for state in [LogState::Live, LogState::Final, LogState::Expired] {
        assert!(!retryable(state, state));
    }
    assert!(!retryable(LogState::Final, LogState::Live));
    assert!(!retryable(LogState::Expired, LogState::Live));
    assert!(!retryable(LogState::Expired, LogState::Final));
}
