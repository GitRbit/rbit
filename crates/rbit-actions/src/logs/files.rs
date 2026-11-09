//! The file-system port for one task's log: which files exist and how each is opened.
//! The composition maps the parts to paths under its data directory; the rules here decide
//! the order of writes, syncs, renames and removals.

use std::{
    fs::File,
    io::{self, Read, Seek, Write},
};

/// The files of one task's log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogPart {
    /// The lines while the task runs.
    Log,
    /// One little-endian `u64` per stored line: the byte offset where it ends.
    Index,
    /// The lines once finalised.
    Gzip,
    /// The gzip file while it is being written.
    GzipPartial,
}

/// An open log file.
pub trait LogFile: Read + Write + Seek {
    fn length(&self) -> io::Result<u64>;
    /// Cuts the file to `length` bytes (the handle must be writable).
    fn truncate_to(&self, length: u64) -> io::Result<()>;
    fn sync_data(&self) -> io::Result<()>;
    fn sync_all(&self) -> io::Result<()>;
}

impl LogFile for File {
    fn length(&self) -> io::Result<u64> {
        Ok(self.metadata()?.len())
    }

    fn truncate_to(&self, length: u64) -> io::Result<()> {
        self.set_len(length)
    }

    fn sync_data(&self) -> io::Result<()> {
        File::sync_data(self)
    }

    fn sync_all(&self) -> io::Result<()> {
        File::sync_all(self)
    }
}

/// How the composition opens, creates, renames and removes one task's log files. A
/// missing file is reported as [`io::ErrorKind::NotFound`].
pub trait TaskLogFiles {
    type File: LogFile;

    /// Creates the directory the files live in.
    fn prepare_directory(&self) -> io::Result<()>;
    /// Opens `part` for reading.
    fn open(&self, part: LogPart) -> io::Result<Self::File>;
    /// Opens `part` for reading and writing, creating it, without truncating it.
    fn open_writable(&self, part: LogPart) -> io::Result<Self::File>;
    /// Opens `part` for appending, creating it.
    fn open_append(&self, part: LogPart) -> io::Result<Self::File>;
    /// Creates `part` empty.
    fn create(&self, part: LogPart) -> io::Result<Self::File>;
    fn rename(&self, from: LogPart, to: LogPart) -> io::Result<()>;
    fn remove(&self, part: LogPart) -> io::Result<()>;
}

/// `result`, with a missing file as `None`.
pub(crate) fn optional<T>(result: io::Result<T>) -> io::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
