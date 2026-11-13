//! Test reports and failure fingerprints (agent plan steps 3.4 and 3.5).
//!
//! * A job hands the server a `JUnit` report by uploading an artifact named
//!   [`REPORT_ARTIFACT`] holding one or more `.xml` files (nextest, pytest, `JUnit` and most
//!   other runners write this format). The server reads at most [`MAX_REPORT_BYTES`] of XML,
//!   counts the tests and keeps the first [`MAX_FAILED_TESTS`] failures, each with its id,
//!   where it failed when the report or its output says so, and a message of at most
//!   [`MAX_TEST_MESSAGE`] bytes. A report is untrusted input: it is parsed by a small,
//!   bounded scanner that never resolves entities beyond the five XML ones and numbers.
//! * A failure's fingerprint hashes its job, what failed (a test id, or the step) and its
//!   message with numbers, hashes and directories taken out, so the same failure on another
//!   commit or another day has the same fingerprint ([`fingerprint`]).

mod junit;

use std::{fmt::Write as _, io::Read};

use sha2::{Digest as _, Sha256};

pub use junit::parse_junit;

/// The artifact name that carries a job's test report.
pub const REPORT_ARTIFACT: &str = "rbit-test-report";

/// Whether an artifact named `name` carries its job's test report: [`REPORT_ARTIFACT`], or
/// that name and a suffix (`rbit-test-report-linux`), since artifact names are unique in a
/// run and every leg of a test matrix has a report.
#[must_use]
pub fn is_report_artifact(name: &str) -> bool {
    name.strip_prefix(REPORT_ARTIFACT).is_some_and(|rest| rest.is_empty() || (rest.len() > 1 && rest.starts_with('-')))
}
/// XML read from one report, over all its files.
pub const MAX_REPORT_BYTES: u64 = 16 * 1024 * 1024;
/// Failed tests kept per job attempt.
pub const MAX_FAILED_TESTS: usize = 50;
/// Bytes kept of a failed test's message.
pub const MAX_TEST_MESSAGE: usize = 1024;

/// A failed test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedTest {
    /// `<suite or class> <name>`, as the report names it.
    pub id: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub message: String,
}

/// Passed test ids kept per job attempt: enough to tell that a failed test passed in
/// another attempt (it is flaky), bounded so a huge suite costs a bounded amount.
pub const MAX_PASSED_TESTS: usize = 2000;

/// What a report says: how many tests ran and failed, the first failures, and the ids of
/// the first tests that passed (skipped ones are neither).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestReport {
    pub total: u64,
    pub failed: u64,
    pub items: Vec<FailedTest>,
    pub passed: Vec<String>,
}

/// Why a report was not read.
#[derive(Debug)]
pub enum ReportError {
    TooLarge,
    Empty,
    Io(std::io::Error),
}

impl std::fmt::Display for ReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => write!(f, "the test report is larger than {MAX_REPORT_BYTES} bytes of XML"),
            Self::Empty => f.write_str("the test report artifact holds no .xml file"),
            Self::Io(error) => write!(f, "cannot read the test report artifact: {error}"),
        }
    }
}

impl std::error::Error for ReportError {}

impl From<std::io::Error> for ReportError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Most bytes a report archive may unpack to, whatever its files are.
pub const MAX_UNPACKED_BYTES: u64 = 4 * MAX_REPORT_BYTES;
/// Most entries a report archive may have.
pub const MAX_REPORT_ENTRIES: usize = 10_000;

/// A reader that fails once more than `left` bytes came out of it.
struct Bounded<R> {
    inner: R,
    left: u64,
}

impl<R: Read> Read for Bounded<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.left = self
            .left
            .checked_sub(read as u64)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "the report archive unpacks to too much"))?;
        Ok(read)
    }
}

/// The report in artifact archive `archive` (a tar.gz of `.xml` files).
///
/// # Errors
///
/// [`ReportError`] when the archive cannot be read, has no XML file, or more XML than
/// [`MAX_REPORT_BYTES`].
pub fn read_report(archive: impl Read) -> Result<TestReport, ReportError> {
    // Everything the archive unpacks to counts, the files skipped too: a small gzip stream
    // can stand for gigabytes of zeros.
    let unpacked = Bounded { inner: flate2::read::GzDecoder::new(archive), left: MAX_UNPACKED_BYTES };
    let mut tar = tar::Archive::new(unpacked);
    let (mut report, mut budget, mut files) = (TestReport::default(), MAX_REPORT_BYTES, 0);
    for (index, entry) in tar.entries()?.enumerate() {
        if index == MAX_REPORT_ENTRIES {
            return Err(ReportError::TooLarge);
        }
        let entry = entry?;
        let file = matches!(entry.header().entry_type(), tar::EntryType::Regular | tar::EntryType::Continuous | tar::EntryType::GNUSparse);
        let is_xml = file && entry.path()?.extension().is_some_and(|extension| extension == "xml");
        if !is_xml {
            continue;
        }
        let mut xml = Vec::new();
        entry.take(budget + 1).read_to_end(&mut xml)?;
        budget = budget.checked_sub(xml.len() as u64).ok_or(ReportError::TooLarge)?;
        parse_junit(&String::from_utf8_lossy(&xml), &mut report);
        files += 1;
    }
    if files == 0 {
        return Err(ReportError::Empty);
    }
    Ok(report)
}

/// `message` without what changes between two runs of the same failure: digits, hex ids of
/// seven or more characters and the directories of paths.
#[must_use]
pub fn normalize(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    for word in message.split_whitespace().take(64) {
        let word = word.rsplit(['/', '\\']).next().unwrap_or(word);
        let core = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        let is_hex = core.len() >= 7 && core.chars().all(|c| c.is_ascii_hexdigit()) && core.chars().any(|c| c.is_ascii_digit());
        if !out.is_empty() {
            out.push(' ');
        }
        if is_hex {
            out.push_str(&word.replacen(core, "#", 1));
            continue;
        }
        let mut digits = false;
        for c in word.chars() {
            if c.is_ascii_digit() {
                if !digits {
                    out.push('0');
                }
                digits = true;
            } else {
                out.push(c);
                digits = false;
            }
        }
    }
    out
}

/// The fingerprint of a failure of job `job` at `what` (a test id or a step) saying
/// `message`: 16 hex digits.
#[must_use]
pub fn fingerprint(job: &str, what: &str, message: &str) -> String {
    let mut hash = Sha256::new();
    for part in [normalize(job).as_str(), what, normalize(message).as_str()] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    hash.finalize().iter().take(8).fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_artifacts_may_carry_a_suffix() {
        assert!(is_report_artifact("rbit-test-report"));
        assert!(is_report_artifact("rbit-test-report-ubuntu-3.12"));
        assert!(!is_report_artifact("rbit-test-report-"));
        assert!(!is_report_artifact("rbit-test-reports"));
        assert!(!is_report_artifact("my-rbit-test-report"));
    }

    #[test]
    fn the_same_failure_on_another_day_has_the_same_fingerprint() {
        let one = "assertion failed at /home/ci/work/1/crates/a/src/lib.rs:41 after 0.25s (4c1e9a0b)";
        let two = "assertion failed at /var/build/77/crates/a/src/lib.rs:43 after 1.5s (9f00d1e2)";
        assert_eq!(normalize(one), "assertion failed at lib.rs:0 after 0.0s (#)");
        assert_eq!(fingerprint("Tests (linux, 1/2)", "a::b", one), fingerprint("Tests (linux, 2/2)", "a::b", two));
        assert_ne!(fingerprint("Tests", "a::b", one), fingerprint("Tests", "a::c", one));
        assert_eq!(fingerprint("Tests", "a::b", one).len(), 16);
    }

    #[test]
    fn a_report_is_read_from_the_artifact_archive() {
        let xml = r#"<testsuites><testsuite name="s"><testcase name="ok" classname="c"/><testcase name="bad" classname="c"><failure message="boom"/></testcase></testsuite></testsuites>"#;
        let mut archive = Vec::new();
        {
            let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(&mut archive, flate2::Compression::fast()));
            let mut header = tar::Header::new_gnu();
            header.set_size(xml.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, "target/nextest/junit.xml", xml.as_bytes()).unwrap();
            let mut header = tar::Header::new_gnu();
            header.set_size(2);
            header.set_cksum();
            tar.append_data(&mut header, "notes.txt", &b"hi"[..]).unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        let report = read_report(archive.as_slice()).unwrap();
        assert_eq!((report.total, report.failed), (2, 1));
        assert_eq!(report.items[0].id, "c bad");
        assert!(matches!(read_report(&b"not gzip"[..]), Err(ReportError::Io(_))));
    }

    #[test]
    fn a_report_archive_that_unpacks_to_too_much_is_refused() {
        let mut archive = Vec::new();
        {
            let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(&mut archive, flate2::Compression::fast()));
            let mut header = tar::Header::new_gnu();
            header.set_size(MAX_UNPACKED_BYTES + 1);
            header.set_mode(0o644);
            header.set_cksum();
            let zeros = std::io::Read::take(std::io::repeat(0), MAX_UNPACKED_BYTES + 1);
            tar.append_data(&mut header, "notes.bin", zeros).unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        assert!(archive.len() < 1 << 20, "a small archive: {} bytes", archive.len());
        assert!(read_report(archive.as_slice()).is_err(), "skipped files count too");
    }
}
