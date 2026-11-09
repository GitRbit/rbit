//! Writing, recovering, finalising and removing one task's log files through the
//! [`TaskLogFiles`] port.
//!
//! * Lines and their index entries are written, then both files are synced; the caller
//!   records the progress only after that, so a crash leaves the files ahead of the
//!   record, never behind it.
//! * Before a process first appends to a task, both files are cut back to the recorded
//!   length (a crash between the writes and the record).
//! * Finalising gzips the plain log into a partial file, syncs it, renames it into place
//!   and only then removes the plain log; the index stays.

use std::io::{self, Read, SeekFrom, Write};

use crate::downloads::ZipWriter;

use super::{
    LogProgress,
    files::{LogFile, LogPart, TaskLogFiles, optional},
    layout,
};

/// gzip level of finalised logs.
pub const GZIP_LEVEL: u32 = 6;

/// Appends `lines` after `progress`, within `max_bytes`; with `recover`, first cuts the
/// files back to what was recorded. Returns the new progress, with both files synced.
pub fn append_lines<F: TaskLogFiles>(
    files: &F,
    mut progress: LogProgress,
    lines: &[String],
    max_bytes: u64,
    recover: bool,
) -> io::Result<LogProgress> {
    files.prepare_directory()?;
    if recover {
        // Cut with writable handles: an append-only handle may not change the length.
        let log = files.open_writable(LogPart::Log)?;
        let length = log.length()?;
        if length > progress.bytes {
            log.truncate_to(progress.bytes)?;
        }
        progress.bytes = progress.bytes.min(length);
        let mut index = files.open_writable(LogPart::Index)?;
        let entries = index.length()? / 8;
        let mut keep = 0;
        let mut entry = [0u8; 8];
        while keep < entries {
            index.read_exact(&mut entry)?;
            if u64::from_le_bytes(entry) > progress.bytes {
                break;
            }
            keep += 1;
        }
        index.truncate_to(keep * 8)?;
    }
    let mut log = files.open_append(LogPart::Log)?;
    let mut index = files.open_append(LogPart::Index)?;
    let laid = layout(progress, lines, max_bytes);
    log.write_all(&laid.text)?;
    index.write_all(&laid.ends)?;
    log.sync_data()?;
    index.sync_data()?;
    Ok(laid.progress)
}

/// Gzips the plain log into place and removes it; the index stays. Nothing when there is
/// no plain log.
pub fn compress<F: TaskLogFiles>(files: &F) -> io::Result<()> {
    let Some(mut source) = optional(files.open(LogPart::Log))? else { return Ok(()) };
    let mut encoder = flate2::write::GzEncoder::new(files.create(LogPart::GzipPartial)?, flate2::Compression::new(GZIP_LEVEL));
    io::copy(&mut source, &mut encoder)?;
    encoder.finish()?.sync_all()?;
    files.rename(LogPart::GzipPartial, LogPart::Gzip)?;
    drop(source);
    files.remove(LogPart::Log)
}

/// Removes every file of the log (retention, the instance log budget, deleted runs);
/// missing ones are fine.
pub fn remove<F: TaskLogFiles>(files: &F) -> io::Result<()> {
    for part in [LogPart::Log, LogPart::Index, LogPart::Gzip] {
        optional(files.remove(part))?;
    }
    Ok(())
}

/// Adds `log` (a raw log's file: gzip or not) to `zip` as `name`, decompressed and read
/// once more to count its size first, so it never needs to be held in memory. A live log
/// is taken as far as it is written now.
///
/// # Errors
///
/// What reading the log or writing the zip fails with.
pub fn add_raw_log<W: Write, F: LogFile>(zip: &mut ZipWriter<W>, name: &str, mut log: F, gzip: bool) -> io::Result<()> {
    if gzip {
        let size = io::copy(&mut flate2::read::GzDecoder::new(&mut log), &mut io::sink())?;
        log.seek(SeekFrom::Start(0))?;
        zip.add(name, size, &mut flate2::read::GzDecoder::new(log).take(size))
    } else {
        let size = log.length()?;
        zip.add(name, size, &mut log.take(size))
    }
}

/// A job name as a file name in the run log archive: no path separators or control
/// characters, at most 100 characters.
pub fn log_file_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .take(100)
        .collect();
    let safe = safe.trim().trim_matches('.').to_owned();
    if safe.is_empty() { "job".to_owned() } else { safe }
}
