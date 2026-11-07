use super::*;
use super::{
    append::{append_lines, clean_line},
    archive::{compress, log_file_name},
    reader::read_window,
    types::{LogFiles, Progress},
};
use rbit_core::{RepoId, actions::TaskId};
use rbit_workflow::protocol::MAX_LOG_LINE_BYTES;

// Actions: run logs.
#[test]
fn job_names_become_safe_file_names() {
    assert_eq!(log_file_name("build (ubuntu-latest, 1.80)"), "build (ubuntu-latest, 1.80)");
    assert_eq!(log_file_name("a/b\\c:d"), "a_b_c_d");
    assert_eq!(log_file_name(".."), "job");
}

#[test]
fn secrets_and_their_lines_are_masked() {
    let masker = Masker::new(["hunter2-secret", "line-one\nline-two\nab", "abc"]);
    assert_eq!(masker.mask("password=hunter2-secret!"), "password=***!");
    assert_eq!(masker.mask("got line-two and ab"), "got *** and ab", "short lines stay");
    assert_eq!(masker.mask("abc is too short to mask"), "abc is too short to mask");
    assert_eq!(masker.mask("line-one\nline-two\nab"), "***", "the whole value at once");
    assert_eq!(masker.mask("line-one\nline-two"), "***\n***");
    assert_eq!(Masker::none().mask("hunter2-secret"), "hunter2-secret");
    assert_eq!(Masker::new(["x"]).mask("x"), "x");
}

fn log_files() -> (tempfile::TempDir, LogFiles) {
    let dir = tempfile::tempdir().unwrap();
    let files = LogFiles::new(dir.path(), RepoId(1), TaskId(2));
    (dir, files)
}

fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn window(files: &LogFiles, gzip: bool, start: u64, end: Option<u64>, range: LogRange, max_lines: usize, max_bytes: usize) -> LogWindow {
    let mut window = LogWindow::default();
    read_window(files, gzip, start, end, range, max_lines, max_bytes, &mut window).unwrap();
    window
}

#[test]
fn lines_are_appended_indexed_and_read_by_range() {
    let (_dir, files) = log_files();
    let start = Progress { lines: 0, bytes: 0, truncated: false };
    let progress = append_lines(&files, start, &lines(&["one", "two", "three"]), 1024, true).unwrap();
    assert_eq!(progress, Progress { lines: 3, bytes: 14, truncated: false });
    let progress = append_lines(&files, progress, &lines(&["four"]), 1024, false).unwrap();
    assert_eq!(std::fs::read_to_string(&files.log).unwrap(), "one\ntwo\nthree\nfour\n");

    let all = window(&files, false, 0, None, LogRange::From(0), 100, 1 << 20);
    assert_eq!((all.lines.clone(), all.first_line, all.next_line), (lines(&["one", "two", "three", "four"]), 0, 4));
    let middle = window(&files, false, 1, Some(3), LogRange::From(0), 100, 1 << 20);
    assert_eq!((middle.lines.clone(), middle.first_line, middle.more_before), (lines(&["two", "three"]), 1, false));
    let tail = window(&files, false, 0, None, LogRange::Tail, 2, 1 << 20);
    assert_eq!((tail.lines.clone(), tail.first_line, tail.more_before), (lines(&["three", "four"]), 2, true));
    let budget = window(&files, false, 0, None, LogRange::Tail, 100, 6);
    assert_eq!(budget.lines, lines(&["four"]), "the tail keeps within the byte budget");
    let from = window(&files, false, 0, None, LogRange::From(1), 100, 5);
    assert_eq!((from.lines.clone(), from.next_line), (lines(&["two"]), 2));
    let one = window(&files, false, 0, None, LogRange::From(2), 100, 1);
    assert_eq!(one.lines, lines(&["three"]), "at least one line");
    assert_eq!(window(&files, false, 0, None, LogRange::From(9), 100, 100).lines, Vec::<String>::new());

    compress(&files).unwrap();
    assert!(!files.log.exists() && files.gzip.exists() && files.index.exists());
    let finished = window(&files, true, 0, None, LogRange::From(1), 2, 1 << 20);
    assert_eq!(finished.lines, lines(&["two", "three"]), "a finished log reads the same");
    assert_eq!(progress.lines, 4);
}

#[test]
fn logs_stop_at_their_budget_and_recover_from_a_crash() {
    let (_dir, files) = log_files();
    let start = Progress { lines: 0, bytes: 0, truncated: false };
    let progress = append_lines(&files, start, &lines(&["12345", "67890", "abcde"]), 13, true).unwrap();
    assert_eq!((progress.lines, progress.truncated), (3, true));
    let text = std::fs::read_to_string(&files.log).unwrap();
    assert!(text.starts_with("12345\n67890\nrbit: the log was cut here"), "{text}");
    let progress = append_lines(&files, progress, &lines(&["more"]), 13, false).unwrap();
    assert_eq!(progress.lines, 4, "dropped lines still count");
    assert_eq!(window(&files, false, 0, None, LogRange::From(0), 100, 1 << 20).lines, lines(&["12345", "67890"]));

    // The files got ahead of the database: the next process cuts them back first.
    let (_dir, files) = log_files();
    let recorded = append_lines(&files, start, &lines(&["kept"]), 1024, true).unwrap();
    append_lines(&files, recorded, &lines(&["lost"]), 1024, false).unwrap();
    let resumed = append_lines(&files, recorded, &lines(&["again"]), 1024, true).unwrap();
    assert_eq!(resumed.lines, 2);
    assert_eq!(std::fs::read_to_string(&files.log).unwrap(), "kept\nagain\n");
    assert_eq!(window(&files, false, 0, None, LogRange::From(0), 100, 1 << 20).lines, lines(&["kept", "again"]));
}

#[test]
fn lines_are_masked_trimmed_and_capped() {
    let masker = Masker::new(["s3cr3t-value"]);
    assert_eq!(clean_line(&masker, "key=s3cr3t-value\r"), "key=***");
    let long = clean_line(&masker, &"é".repeat(MAX_LOG_LINE_BYTES));
    assert!(long.len() <= MAX_LOG_LINE_BYTES && long.ends_with("[line truncated]"));
}

#[test]
fn overlapping_secrets_mask_the_longest() {
    let masker = Masker::new(["token", "token-extended"]);
    assert_eq!(masker.mask("a token-extended b token"), "a *** b ***");
}
