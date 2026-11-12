//! Artifact downloads: the stored archive rewritten as it is sent.
//!
//! Artifacts are stored as the tar.gz the runner uploads, whose files sit under a `w/`
//! directory (the workspace's area in the runner's archive format, shared with its cache).
//! A download never shows that directory: [`to_zip`] writes the files as a zip, the format
//! GitHub serves, and [`to_tar_gz`] as a tar.gz with the files at its top. Both work as the
//! stored archive is read, so a download needs no temporary file and starts at once.
//!
//! * Only regular files are kept. Directories, links and other special entries are left
//!   out, as `actions/upload-artifact` leaves them out; a name outside `w/`, absolute or
//!   leaving the archive (`..`) is skipped.
//! * Sizes and CRCs follow each entry's data in a data descriptor (general purpose bit 3),
//!   because the compressed size is only known once it is written.
//! * ZIP64 records are written where a size, an offset or the entry count needs them, so
//!   archives past 4 GiB or 65,535 files open in every reader.
//! * Names are UTF-8 (bit 11). Permissions keep the read, write and execute bits only:
//!   set-user-ID, set-group-ID and sticky bits never travel from a job to whoever unpacks
//!   the zip.

use std::io::{self, Read, Write};

mod zip_format;

pub use self::zip_format::ZipWriter;
use self::zip_format::{Counter, dos_time, write_directory, write_entry};

/// The stored archive's directory that holds an artifact's files.
const STORED_ROOT: &str = rbit_workflow::protocol::ARCHIVE_WORKSPACE_DIR;

/// A regular file of a stored artifact, as the downloads write it.
struct File<'a, R: Read> {
    /// Relative to the artifact's top, `/`-separated.
    name: Vec<u8>,
    size: u64,
    /// Read, write and execute bits only.
    mode: u32,
    mtime: u64,
    data: tar::Entry<'a, R>,
}

/// Calls `each` with every regular file of the stored tar.gz `source`, in order.
fn stored_files<R: Read>(source: R, mut each: impl FnMut(File<'_, flate2::read::GzDecoder<R>>) -> io::Result<()>) -> io::Result<()> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(source));
    for entry in archive.entries()? {
        let entry = entry?;
        let header = entry.header();
        // A file with holes (a freshly linked binary often has some) is a GNU sparse entry
        // when an older runner packed it; reading it yields the whole file.
        if !matches!(header.entry_type(), tar::EntryType::Regular | tar::EntryType::Continuous | tar::EntryType::GNUSparse) {
            continue;
        }
        let Some(name) = entry_name(&entry.path_bytes()) else { continue };
        // The file's size: for a sparse entry, larger than what the archive stores.
        let size = entry.size();
        let mode = match header.mode()? & 0o777 {
            0 => 0o644,
            mode => mode,
        };
        let mtime = header.mtime().unwrap_or(0);
        each(File { name, size, mode, mtime, data: entry })?;
    }
    // Reading on to the end checks the gzip trailer (its CRC and length), which the tar
    // reader, stopping at its end-of-archive blocks, never reaches.
    io::copy(&mut archive.into_inner(), &mut io::sink())?;
    Ok(())
}

// Agent loop: promoting an artifact.
/// Writes the files of the stored tar.gz `source` into directory `dir` (flattened to their
/// file names), at most `max_files`; each file's name and path, in order.
///
/// # Errors
///
/// What reading `source` or writing a file fails with, and [`io::ErrorKind::InvalidData`]
/// for more than `max_files` files, or for two files with one name (`linux/app` and
/// `macos/app`): one would overwrite the other, and what ships would not be what was built.
pub fn extract_files<R: Read>(source: R, dir: &std::path::Path, max_files: usize) -> io::Result<Vec<(String, std::path::PathBuf)>> {
    let mut written = Vec::new();
    stored_files(source, |mut file| {
        if written.len() == max_files {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("the artifact has more than {max_files} files")));
        }
        let name = String::from_utf8_lossy(&file.name).rsplit('/').next().unwrap_or_default().to_owned();
        if name.is_empty() || name.starts_with('.') {
            return Ok(());
        }
        if written.iter().any(|(taken, _)| *taken == name) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("the artifact has two files named `{name}`")));
        }
        let path = dir.join(&name);
        let mut out = std::fs::File::create(&path)?;
        io::copy(&mut file.data, &mut out)?;
        written.push((name, path));
        Ok(())
    })?;
    Ok(written)
}

/// Reads the stored tar.gz `source` and writes its files as a zip to `out`.
///
/// # Errors
///
/// What reading `source` (a corrupt archive included) or writing `out` fails with.
pub fn to_zip<R: Read, W: Write>(source: R, out: W) -> io::Result<W> {
    let mut out = Counter { inner: out, count: 0 };
    let mut written = Vec::new();
    stored_files(source, |mut file| {
        let dos = dos_time(file.mtime);
        written.push(write_entry(&mut out, file.name, file.size, file.mode, dos, &mut file.data)?);
        Ok(())
    })?;
    write_directory(&mut out, &written)?;
    out.flush()?;
    Ok(out.inner)
}

/// Reads the stored tar.gz `source` and writes its files as a tar.gz to `out`, at its top.
///
/// # Errors
///
/// What reading `source` (a corrupt archive included) or writing `out` fails with.
pub fn to_tar_gz<R: Read, W: Write>(source: R, out: W) -> io::Result<W> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(out, flate2::Compression::fast()));
    stored_files(source, |file| {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(file.size);
        header.set_mode(file.mode);
        header.set_mtime(file.mtime);
        let name = String::from_utf8_lossy(&file.name).into_owned();
        builder.append_data(&mut header, name, file.data)
    })?;
    let mut out = builder.into_inner()?.finish()?;
    out.flush()?;
    Ok(out)
}

/// The name of a stored entry relative to the artifact's top: `/`-separated, without `.`
/// parts, or `None` when it is outside [`STORED_ROOT`], absolute, leaves the archive or
/// names the root itself.
fn entry_name(path: &[u8]) -> Option<Vec<u8>> {
    let text = String::from_utf8_lossy(path).replace('\\', "/");
    if text.starts_with('/') {
        return None;
    }
    let mut parts = Vec::new();
    for part in text.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            part if part.contains(':') && parts.is_empty() => return None,
            part => parts.push(part),
        }
    }
    if parts.first() != Some(&STORED_ROOT) || parts.len() < 2 {
        return None;
    }
    Some(parts[1..].join("/").into_bytes())
}

#[cfg(test)]
mod tests;
