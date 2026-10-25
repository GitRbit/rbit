//! A task's log on its way to the server: numbered lines, a disk spool, and batched
//! appends by line offset (§12.5.4 "Reconnect" and §12.6 `tasks/{id}/logs`).
//!
//! Every line is written to `<task dir>/log.spool` as it is produced and numbered from 0;
//! the server stores lines from its own count, so resending a range it already has is
//! harmless. [`TaskLog::flush`] sends what the server has not acknowledged, in batches,
//! and follows a `409 {next_line}` back to where the server's log really ends.
//!
//! Acknowledged lines stay in the spool until room is needed, so a resync can resend
//! them. The spool holds at most [`MAX_SPOOL_BYTES`]: when the server is unreachable for
//! long enough to fill it, further lines are counted and dropped, and one line saying how
//! many were lost is written as soon as there is room again. Lines are numbered only when
//! they are kept, so offsets never have holes.

use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{self, BufWriter, Read as _, Seek as _, SeekFrom, Write as _},
    path::{Path, PathBuf},
    sync::Mutex,
};

use rbit_workflow::protocol::{MAX_LOG_LINE_BYTES, TaskId};

use crate::client::{CallError, Client};

/// The most a task's spool holds.
pub const MAX_SPOOL_BYTES: u64 = 64 * 1024 * 1024;
/// The largest log batch sent at once (the endpoint accepts 8 MiB).
pub const MAX_BATCH_BYTES: usize = 1024 * 1024;
/// The most batches one [`TaskLog::flush`] sends, so a heartbeat is never starved.
const MAX_BATCHES_PER_FLUSH: usize = 16;

/// The log of one task.
#[derive(Debug)]
pub struct TaskLog {
    state: Mutex<Spool>,
}

#[derive(Debug)]
struct Spool {
    path: PathBuf,
    /// `None` after a write failed or the spool was compacted, until the next write.
    writer: Option<BufWriter<File>>,
    cap: u64,
    /// Lines kept so far: the next line's number.
    total: u64,
    /// Lines the server has acknowledged.
    acked: u64,
    /// The number of the spool's first line.
    first: u64,
    /// Where each spooled line starts, from line `first` on.
    offsets: VecDeque<u64>,
    /// Bytes in the spool.
    bytes: u64,
    /// Lines dropped since the spool was last full.
    dropped: u64,
    /// The server refused this log for good; nothing more is sent.
    refused: bool,
}

impl TaskLog {
    /// A new, empty log spooled to `path`.
    pub fn create(path: &Path) -> io::Result<Self> {
        Self::with_cap(path, MAX_SPOOL_BYTES)
    }

    pub fn with_cap(path: &Path, cap: u64) -> io::Result<Self> {
        let file = File::options().create(true).truncate(true).read(true).write(true).open(path)?;
        Ok(Self {
            state: Mutex::new(Spool {
                path: path.to_path_buf(),
                writer: Some(BufWriter::new(file)),
                cap,
                total: 0,
                acked: 0,
                first: 0,
                offsets: VecDeque::new(),
                bytes: 0,
                dropped: 0,
                refused: false,
            }),
        })
    }

    /// Lines kept so far, which is also the next line's number.
    pub fn len(&self) -> u64 {
        self.lock().total
    }

    /// Lines the server has acknowledged.
    #[cfg(test)]
    pub fn acked(&self) -> u64 {
        self.lock().acked
    }

    /// Whether the server has every line.
    pub fn is_flushed(&self) -> bool {
        let spool = self.lock();
        spool.refused || spool.acked == spool.total
    }

    /// Appends `text` as one or more lines: it is split at newlines and at
    /// [`MAX_LOG_LINE_BYTES`]. Returns how many lines were kept.
    pub fn push(&self, text: &str) -> u64 {
        let mut spool = self.lock();
        let mut kept = 0;
        for line in text.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            for chunk in chunks(line, MAX_LOG_LINE_BYTES) {
                if spool.dropped > 0 {
                    let notice = format!(
                        "[rbit-runner] {} log line(s) were dropped while the server was unreachable and the log spool was full",
                        spool.dropped
                    );
                    if spool.append(&notice) {
                        spool.dropped = 0;
                    }
                }
                if spool.dropped == 0 && spool.append(chunk) {
                    kept += 1;
                } else {
                    spool.dropped += 1;
                }
            }
        }
        kept
    }

    /// Sends unacknowledged lines, in batches, until the server has them all or a call
    /// fails. A `409` moves the next batch to the server's `next_line`.
    pub async fn flush(&self, client: &Client, task: TaskId) -> Result<(), CallError> {
        for _ in 0..MAX_BATCHES_PER_FLUSH {
            let batch = match self.next_batch() {
                Ok(Some(batch)) => batch,
                Ok(None) => return Ok(()),
                Err(error) => {
                    tracing::error!(task = %task, %error, "cannot read the log spool; the rest of this task's log is lost");
                    self.lock().refused = true;
                    return Ok(());
                }
            };
            match client.task_logs(task, batch.from, batch.body).await {
                Ok(answer) => self.acknowledge(task, answer.next_line),
                Err(error) => match error.next_line() {
                    Some(next_line) => self.acknowledge(task, next_line),
                    None => return Err(error),
                },
            }
        }
        Ok(())
    }

    /// Stops sending this log, after the server refused it for good.
    pub fn give_up(&self) {
        self.lock().refused = true;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Spool> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Up to [`MAX_BATCH_BYTES`] of whole lines from the first unacknowledged one.
    fn next_batch(&self) -> io::Result<Option<Batch>> {
        let mut spool = self.lock();
        if spool.refused || spool.acked >= spool.total {
            return Ok(None);
        }
        spool.flush_writer()?;
        let index = usize::try_from(spool.acked - spool.first).unwrap_or(usize::MAX);
        let start = spool.offsets[index];
        // The batch ends before the first line that starts beyond the batch size, but it
        // always holds at least one line.
        let limit = MAX_BATCH_BYTES as u64;
        let end = match spool.offsets.iter().skip(index + 1).position(|&offset| offset - start > limit) {
            Some(lines) => spool.offsets[index + lines.max(1)],
            None => spool.bytes,
        };
        let mut file = File::open(&spool.path)?;
        file.seek(SeekFrom::Start(start))?;
        let mut body = Vec::with_capacity(usize::try_from(end - start).unwrap_or(0));
        file.take(end - start).read_to_end(&mut body)?;
        Ok(Some(Batch { from: spool.acked, body }))
    }

    fn acknowledge(&self, task: TaskId, next_line: u64) {
        let mut spool = self.lock();
        if next_line > spool.total {
            tracing::warn!(task = %task, next_line, sent = spool.total, "the server reports more log lines than were sent");
            spool.acked = spool.total;
        } else if next_line < spool.first {
            tracing::error!(task = %task, next_line, "the server lost log lines that are no longer spooled; the rest of this task's log is lost");
            spool.refused = true;
        } else {
            spool.acked = next_line;
        }
    }
}

impl Spool {
    /// Writes one line, making room by dropping acknowledged lines when the spool is
    /// full. `false` when there is no room.
    fn append(&mut self, line: &str) -> bool {
        let size = line.len() as u64 + 1;
        if self.bytes + size > self.cap && self.acked > self.first && self.compact().is_err() {
            return false;
        }
        if self.bytes + size > self.cap {
            return false;
        }
        let written = match self.writer() {
            Ok(writer) => writer.write_all(line.as_bytes()).and_then(|()| writer.write_all(b"\n")),
            Err(error) => Err(error),
        };
        if written.is_err() {
            // A partial write would shift every later offset: start over from a clean file.
            self.writer = None;
            return false;
        }
        self.offsets.push_back(self.bytes);
        self.bytes += size;
        self.total += 1;
        true
    }

    /// The open writer, reopened at the spool's end after a failure or a compaction.
    fn writer(&mut self) -> io::Result<&mut BufWriter<File>> {
        let writer = if let Some(writer) = self.writer.take() {
            writer
        } else {
            let file = File::options().write(true).open(&self.path)?;
            file.set_len(self.bytes)?;
            let mut writer = BufWriter::new(file);
            writer.seek(SeekFrom::Start(self.bytes))?;
            writer
        };
        Ok(self.writer.insert(writer))
    }

    fn flush_writer(&mut self) -> io::Result<()> {
        match &mut self.writer {
            Some(writer) => writer.flush(),
            None => Ok(()),
        }
    }

    /// Rewrites the spool without its acknowledged lines. The writer is closed first,
    /// because Windows does not replace a file that is open.
    fn compact(&mut self) -> io::Result<()> {
        self.flush_writer()?;
        self.writer = None;
        let index = usize::try_from(self.acked - self.first).unwrap_or(usize::MAX);
        let start = self.offsets.get(index).copied().unwrap_or(self.bytes);
        let mut tail = Vec::new();
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(start))?;
        file.take(self.bytes - start).read_to_end(&mut tail)?;
        let temp = self.path.with_extension("compact");
        fs::write(&temp, &tail)?;
        fs::rename(&temp, &self.path)?;
        self.offsets.drain(..index);
        for offset in &mut self.offsets {
            *offset -= start;
        }
        self.bytes -= start;
        self.first = self.acked;
        Ok(())
    }
}

/// One append: whole lines from line `from`.
#[derive(Debug)]
struct Batch {
    from: u64,
    body: Vec<u8>,
}

/// `line` in pieces of at most `max` bytes, split at character boundaries; an empty line
/// is one empty piece.
fn chunks(line: &str, max: usize) -> Vec<&str> {
    if line.len() <= max {
        return vec![line];
    }
    let mut pieces = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        let mut end = rest.len().min(max);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (piece, tail) = rest.split_at(end);
        pieces.push(piece);
        rest = tail;
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeServer;

    #[test]
    fn lines_are_split_at_newlines_and_the_size_cap() {
        let dir = tempfile::tempdir().unwrap();
        let log = TaskLog::create(&dir.path().join("log.spool")).unwrap();
        assert_eq!(log.push("one\r\ntwo\n"), 3, "the trailing newline ends an empty line");
        assert_eq!(log.push(&"é".repeat(MAX_LOG_LINE_BYTES)), 2);
        assert_eq!(log.len(), 5);
        assert_eq!(chunks("", 4), [""]);
        assert_eq!(chunks("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(chunks("aé", 2), ["a", "é"]);
    }

    #[tokio::test]
    async fn flushes_resend_harmlessly_and_follow_a_gap() {
        let server = FakeServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let log = TaskLog::create(&dir.path().join("log.spool")).unwrap();
        let task = TaskId(1);
        for line in ["a", "b", "c", "d", "e"] {
            log.push(line);
        }
        log.flush(&server.client(), task).await.unwrap();
        assert!(log.is_flushed());
        assert_eq!(server.log(task), ["a", "b", "c", "d", "e"]);

        // The server lost two lines it had acknowledged: the next append is a gap, and the
        // runner resends from where the server's log ends.
        server.truncate_log(task, 3);
        log.push("f");
        log.flush(&server.client(), task).await.unwrap();
        assert_eq!(server.log(task), ["a", "b", "c", "d", "e", "f"]);
        assert!(server.log_calls(task).iter().any(|&(from, status)| (from, status) == (5, 409)));
        assert_eq!(log.acked(), 6);
    }

    #[tokio::test]
    async fn the_spool_keeps_lines_while_the_server_is_away() {
        let server = FakeServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let log = TaskLog::create(&dir.path().join("log.spool")).unwrap();
        let task = TaskId(2);
        server.fail_next("logs", 503, 3);
        let lines: Vec<String> = (0..3000).map(|i| format!("line {i} {}", "x".repeat(i % 2000))).collect();
        for (i, line) in lines.iter().enumerate() {
            log.push(line);
            if i % 1000 == 999 {
                assert!(log.flush(&server.client(), task).await.is_err(), "the server is away");
            }
        }
        while !log.is_flushed() {
            log.flush(&server.client(), task).await.unwrap();
        }
        assert_eq!(server.log(task), lines);
        let calls = server.log_calls(task);
        assert!(calls.iter().filter(|(_, status)| *status == 200).count() > 1, "{calls:?} should be batched");
    }

    #[tokio::test]
    async fn a_full_spool_drops_lines_and_says_so() {
        let server = FakeServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let log = TaskLog::with_cap(&dir.path().join("log.spool"), 160).unwrap();
        let task = TaskId(3);
        for i in 0..10 {
            log.push(&format!("line {i:02} ..........."));
        }
        // 8 lines of 20 bytes fill the spool; two are dropped.
        assert_eq!(log.len(), 8);
        server.fail_next("logs", 503, 1);
        assert!(log.flush(&server.client(), task).await.is_err());
        log.flush(&server.client(), task).await.unwrap();
        // Acknowledged lines make room: the notice comes first, then the new line.
        log.push("after");
        log.flush(&server.client(), task).await.unwrap();
        let received = server.log(task);
        assert_eq!(received.len(), 10);
        assert!(received[8].contains("2 log line(s) were dropped"), "{received:?}");
        assert_eq!(received[9], "after");
    }
}
