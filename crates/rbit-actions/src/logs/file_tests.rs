use std::{fs, fs::File, io, path::PathBuf};

use super::*;

/// One task's log files in a temporary directory.
struct Dir(tempfile::TempDir);

impl Dir {
    fn path(&self, part: LogPart) -> PathBuf {
        self.0.path().join("logs").join(match part {
            LogPart::Log => "1.log",
            LogPart::Index => "1.idx",
            LogPart::Gzip => "1.log.gz",
            LogPart::GzipPartial => "1.log.gz.partial",
        })
    }
}

impl TaskLogFiles for Dir {
    type File = File;
    fn prepare_directory(&self) -> io::Result<()> {
        fs::create_dir_all(self.0.path().join("logs"))
    }
    fn open(&self, part: LogPart) -> io::Result<File> {
        File::open(self.path(part))
    }
    fn open_writable(&self, part: LogPart) -> io::Result<File> {
        fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(self.path(part))
    }
    fn open_append(&self, part: LogPart) -> io::Result<File> {
        fs::OpenOptions::new().create(true).append(true).open(self.path(part))
    }
    fn create(&self, part: LogPart) -> io::Result<File> {
        File::create(self.path(part))
    }
    fn rename(&self, from: LogPart, to: LogPart) -> io::Result<()> {
        fs::rename(self.path(from), self.path(to))
    }
    fn remove(&self, part: LogPart) -> io::Result<()> {
        fs::remove_file(self.path(part))
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn tail(dir: &Dir, gzip: bool) -> Vec<String> {
    let request = WindowRequest { start: 0, end: None, range: LogRange::Tail, limits: LogLimits { lines: 100, bytes: 1 << 20 } };
    let mut window = LogWindow::default();
    read_window(dir, gzip, request, &mut window, &mut ReadWork::default()).unwrap();
    window.lines
}

#[test]
fn recovery_cuts_files_ahead_of_the_record_and_finalising_keeps_the_index() {
    let dir = Dir(tempfile::tempdir().unwrap());
    let start = LogProgress { lines: 0, bytes: 0, truncated: false };
    let recorded = append_lines(&dir, start, &strings(&["kept"]), 1024, true).unwrap();
    append_lines(&dir, recorded, &strings(&["lost"]), 1024, false).unwrap();
    let resumed = append_lines(&dir, recorded, &strings(&["again"]), 1024, true).unwrap();
    assert_eq!(resumed.lines, 2);
    assert_eq!(fs::read_to_string(dir.path(LogPart::Log)).unwrap(), "kept\nagain\n");
    assert_eq!(fs::metadata(dir.path(LogPart::Index)).unwrap().len(), 16);
    assert_eq!(tail(&dir, false), strings(&["kept", "again"]));

    compress(&dir).unwrap();
    assert!(!dir.path(LogPart::Log).exists() && !dir.path(LogPart::GzipPartial).exists() && dir.path(LogPart::Index).exists());
    assert_eq!(tail(&dir, true), strings(&["kept", "again"]), "a finalised log reads the same");
    compress(&dir).unwrap();
    remove(&dir).unwrap();
    remove(&dir).unwrap();
    assert!(!dir.path(LogPart::Gzip).exists() && !dir.path(LogPart::Index).exists());
}

#[test]
fn page_reads_honour_the_committed_snapshot_and_lifecycle() {
    let dir = Dir(tempfile::tempdir().unwrap());
    let start = LogProgress { lines: 0, bytes: 0, truncated: false };
    let committed = append_lines(&dir, start, &strings(&["one", "two"]), 1024, true).unwrap();
    append_lines(&dir, committed, &strings(&["uncommitted"]), 1024, false).unwrap();
    let snapshot =
        Snapshot { lines: committed.lines, bytes: committed.bytes, representation: Representation::Plain, live: true, truncated: false };
    let request = WindowRequest { start: 0, end: None, range: LogRange::Tail, limits: LogLimits { lines: 100, bytes: 1024 } };
    let windows = read_windows(&dir, snapshot, &[request], &mut ReadWork::default()).unwrap();
    assert_eq!(windows[0].lines, strings(&["one", "two"]));
    let expired = Snapshot { representation: Representation::Expired, ..snapshot };
    let mut work = ReadWork::default();
    assert!(read_windows(&dir, expired, &[request], &mut work).unwrap()[0].expired);
    assert_eq!(work, ReadWork::default(), "an expired log is not opened");
    assert!(retryable(Representation::Plain, Representation::Gzip) && !retryable(Representation::Gzip, Representation::Plain));
    assert_eq!(log_file_name("a/b"), "a_b");
}
