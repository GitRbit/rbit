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

/// Above this uncompressed size an entry is written with ZIP64 sizes. Deflate can grow
/// incompressible data by a little, so the margin keeps the compressed size in range too.
const ZIP64_ENTRY: u64 = 0xFFFF_0000;
const MAX_32: u64 = 0xFFFF_FFFF;
const MAX_16: u64 = 0xFFFF;
/// "Made by" Unix (3), specification 4.5.
const VERSION_MADE_BY: u16 = (3 << 8) | 0x2d;
const VERSION_DEFLATE: u16 = 20;
const VERSION_ZIP64: u16 = 45;
const FLAG_DESCRIPTOR: u16 = 1 << 3;
const FLAG_UTF8: u16 = 1 << 11;
const METHOD_DEFLATE: u16 = 8;
/// A regular file (`S_IFREG`) in the high half of the external attributes.
const S_IFREG: u32 = 0o100_000;

/// What the central directory needs of a written entry.
struct Written {
    name: Vec<u8>,
    crc: u32,
    compressed: u64,
    size: u64,
    offset: u64,
    dos: (u16, u16),
    mode: u32,
    zip64: bool,
}

/// Counts what is written through it: the offsets the zip records.
struct Counter<W> {
    inner: W,
    count: u64,
}

impl<W: Write> Write for Counter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.count += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// The stored archive's directory that holds an artifact's files.
const STORED_ROOT: &str = "w";

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
        if !matches!(header.entry_type(), tar::EntryType::Regular | tar::EntryType::Continuous) {
            continue;
        }
        let Some(name) = entry_name(&entry.path_bytes()) else { continue };
        let size = header.size()?;
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

// Actions: run logs.
/// Writes `files` (name, contents), changed at `mtime` (Unix seconds), as a zip to `out`.
///
/// # Errors
///
/// What writing `out` fails with.
pub fn files_to_zip<W: Write>(files: &[(String, Vec<u8>)], mtime: u64, out: W) -> io::Result<W> {
    let mut out = Counter { inner: out, count: 0 };
    let dos = dos_time(mtime);
    let mut written = Vec::with_capacity(files.len());
    for (name, data) in files {
        written.push(write_entry(&mut out, name.as_bytes().to_vec(), data.len() as u64, 0o644, dos, &mut data.as_slice())?);
    }
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

fn write_entry<W: Write>(
    out: &mut Counter<W>,
    name: Vec<u8>,
    size: u64,
    mode: u32,
    dos: (u16, u16),
    data: &mut impl Read,
) -> io::Result<Written> {
    let offset = out.count;
    let zip64 = size >= ZIP64_ENTRY;
    let mut header = Vec::with_capacity(30 + name.len() + 20);
    header.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
    header.extend_from_slice(&(if zip64 { VERSION_ZIP64 } else { VERSION_DEFLATE }).to_le_bytes());
    header.extend_from_slice(&(FLAG_DESCRIPTOR | FLAG_UTF8).to_le_bytes());
    header.extend_from_slice(&METHOD_DEFLATE.to_le_bytes());
    header.extend_from_slice(&dos.0.to_le_bytes());
    header.extend_from_slice(&dos.1.to_le_bytes());
    header.extend_from_slice(&0_u32.to_le_bytes());
    let unknown: u32 = if zip64 { u32::MAX } else { 0 };
    header.extend_from_slice(&unknown.to_le_bytes());
    header.extend_from_slice(&unknown.to_le_bytes());
    header.extend_from_slice(&u16::try_from(name.len()).map_err(|_| invalid("a file name is too long"))?.to_le_bytes());
    header.extend_from_slice(&(if zip64 { 20_u16 } else { 0 }).to_le_bytes());
    header.extend_from_slice(&name);
    if zip64 {
        header.extend_from_slice(&1_u16.to_le_bytes());
        header.extend_from_slice(&16_u16.to_le_bytes());
        header.extend_from_slice(&[0; 16]);
    }
    out.write_all(&header)?;

    let start = out.count;
    let mut crc = flate2::Crc::new();
    let mut encoder = flate2::write::DeflateEncoder::new(&mut *out, flate2::Compression::default());
    let mut buffer = vec![0; 64 * 1024];
    let mut read = 0_u64;
    loop {
        let count = data.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        crc.update(&buffer[..count]);
        encoder.write_all(&buffer[..count])?;
        read += count as u64;
    }
    encoder.finish()?;
    if read != size {
        return Err(invalid("an entry is shorter than its header says"));
    }
    let compressed = out.count - start;

    let mut descriptor = Vec::with_capacity(24);
    descriptor.extend_from_slice(&0x0807_4b50_u32.to_le_bytes());
    descriptor.extend_from_slice(&crc.sum().to_le_bytes());
    if zip64 {
        descriptor.extend_from_slice(&compressed.to_le_bytes());
        descriptor.extend_from_slice(&size.to_le_bytes());
    } else {
        let narrow = |value: u64| u32::try_from(value).map_err(|_| invalid("an entry outgrew its 32-bit sizes"));
        descriptor.extend_from_slice(&narrow(compressed)?.to_le_bytes());
        descriptor.extend_from_slice(&narrow(size)?.to_le_bytes());
    }
    out.write_all(&descriptor)?;
    Ok(Written { name, crc: crc.sum(), compressed, size, offset, dos, mode, zip64 })
}

fn write_directory<W: Write>(out: &mut Counter<W>, entries: &[Written]) -> io::Result<()> {
    let start = out.count;
    for entry in entries {
        // ZIP64 extra fields hold, in this order, only the values that do not fit.
        let mut extra = Vec::new();
        let wide_size = entry.zip64 || entry.size >= MAX_32;
        let wide_compressed = entry.zip64 || entry.compressed >= MAX_32;
        let wide_offset = entry.offset >= MAX_32;
        for (wide, value) in [(wide_size, entry.size), (wide_compressed, entry.compressed), (wide_offset, entry.offset)] {
            if wide {
                extra.extend_from_slice(&value.to_le_bytes());
            }
        }
        // A value that does not fit is in the ZIP64 field; its 32-bit field says so with all ones.
        let fit = |wide: bool, value: u64| if wide { u32::MAX } else { u32::try_from(value).unwrap_or(u32::MAX) };
        let zip64 = !extra.is_empty();
        let mut record = Vec::with_capacity(46 + entry.name.len() + 4 + extra.len());
        record.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        record.extend_from_slice(&VERSION_MADE_BY.to_le_bytes());
        record.extend_from_slice(&(if zip64 { VERSION_ZIP64 } else { VERSION_DEFLATE }).to_le_bytes());
        record.extend_from_slice(&(FLAG_DESCRIPTOR | FLAG_UTF8).to_le_bytes());
        record.extend_from_slice(&METHOD_DEFLATE.to_le_bytes());
        record.extend_from_slice(&entry.dos.0.to_le_bytes());
        record.extend_from_slice(&entry.dos.1.to_le_bytes());
        record.extend_from_slice(&entry.crc.to_le_bytes());
        record.extend_from_slice(&fit(wide_compressed, entry.compressed).to_le_bytes());
        record.extend_from_slice(&fit(wide_size, entry.size).to_le_bytes());
        record.extend_from_slice(&short(entry.name.len()).to_le_bytes());
        record.extend_from_slice(&(if zip64 { short(extra.len() + 4) } else { 0 }).to_le_bytes());
        record.extend_from_slice(&0_u16.to_le_bytes()); // comment
        record.extend_from_slice(&0_u16.to_le_bytes()); // disk
        record.extend_from_slice(&0_u16.to_le_bytes()); // internal attributes
        record.extend_from_slice(&((S_IFREG | entry.mode) << 16).to_le_bytes());
        record.extend_from_slice(&fit(wide_offset, entry.offset).to_le_bytes());
        record.extend_from_slice(&entry.name);
        if zip64 {
            record.extend_from_slice(&1_u16.to_le_bytes());
            record.extend_from_slice(&short(extra.len()).to_le_bytes());
            record.extend_from_slice(&extra);
        }
        out.write_all(&record)?;
    }
    let size = out.count - start;
    let count = entries.len() as u64;
    if count >= MAX_16 || size >= MAX_32 || start >= MAX_32 {
        let record = out.count;
        let mut end64 = Vec::with_capacity(56 + 20);
        end64.extend_from_slice(&0x0606_4b50_u32.to_le_bytes());
        end64.extend_from_slice(&44_u64.to_le_bytes());
        end64.extend_from_slice(&VERSION_MADE_BY.to_le_bytes());
        end64.extend_from_slice(&VERSION_ZIP64.to_le_bytes());
        end64.extend_from_slice(&0_u32.to_le_bytes());
        end64.extend_from_slice(&0_u32.to_le_bytes());
        end64.extend_from_slice(&count.to_le_bytes());
        end64.extend_from_slice(&count.to_le_bytes());
        end64.extend_from_slice(&size.to_le_bytes());
        end64.extend_from_slice(&start.to_le_bytes());
        end64.extend_from_slice(&0x0706_4b50_u32.to_le_bytes());
        end64.extend_from_slice(&0_u32.to_le_bytes());
        end64.extend_from_slice(&record.to_le_bytes());
        end64.extend_from_slice(&1_u32.to_le_bytes());
        out.write_all(&end64)?;
    }
    let mut end = Vec::with_capacity(22);
    end.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    end.extend_from_slice(&0_u16.to_le_bytes());
    end.extend_from_slice(&0_u16.to_le_bytes());
    let entries16 = u16::try_from(count.min(MAX_16)).unwrap_or(u16::MAX);
    end.extend_from_slice(&entries16.to_le_bytes());
    end.extend_from_slice(&entries16.to_le_bytes());
    end.extend_from_slice(&u32::try_from(size.min(MAX_32)).unwrap_or(u32::MAX).to_le_bytes());
    end.extend_from_slice(&u32::try_from(start.min(MAX_32)).unwrap_or(u32::MAX).to_le_bytes());
    end.extend_from_slice(&0_u16.to_le_bytes());
    out.write_all(&end)
}

/// A length the zip records in 16 bits; names were checked to fit when their entry was
/// written, and the extra fields are a few bytes.
fn short(length: usize) -> u16 {
    u16::try_from(length).unwrap_or(u16::MAX)
}

/// MS-DOS time and date of a Unix time, in UTC; times before 1980 become its first second
/// and years past 2107 its last year.
fn dos_time(unix: u64) -> (u16, u16) {
    const EARLIEST: u64 = 315_532_800; // 1980-01-01T00:00:00Z
    let unix = unix.max(EARLIEST);
    let (days, seconds) = (unix / 86_400, unix % 86_400);
    // Howard Hinnant's civil_from_days, for days after 1970 (so every value is positive).
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    let field = |value: u64| u16::try_from(value).unwrap_or(0);
    let time = field(seconds / 3600) << 11 | field(seconds % 3600 / 60) << 5 | field(seconds % 60 / 2);
    let date = field((year - 1980).min(127)) << 9 | field(month) << 5 | field(day);
    (time, date)
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn tar_gz(files: &[(&str, &[u8], u32, tar::EntryType)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        for (name, data, mode, kind) in files {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(*kind);
            header.set_size(data.len() as u64);
            header.set_mode(*mode);
            header.set_mtime(1_700_000_000);
            if *kind == tar::EntryType::Symlink {
                header.set_link_name("target").unwrap();
            }
            // `append_data` would refuse the names the converter must skip itself.
            let bytes = name.as_bytes();
            header.as_old_mut().name[..bytes.len()].copy_from_slice(bytes);
            header.set_cksum();
            builder.append(&header, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn convert(tar: &[u8]) -> zip::ZipArchive<Cursor<Vec<u8>>> {
        let bytes = to_zip(Cursor::new(tar), Vec::new()).unwrap();
        zip::ZipArchive::new(Cursor::new(bytes)).unwrap()
    }

    #[test]
    fn files_keep_their_paths_contents_and_safe_modes() {
        use tar::EntryType::{Directory, Regular, Symlink};
        let tar = tar_gz(&[
            ("./w/a.txt", b"ay", 0o644, Regular),
            ("w/nested/", b"", 0o755, Directory),
            ("w/nested/deep/run.sh", b"#!/bin/sh\necho hi\n", 0o4755, Regular),
            ("w/link", b"", 0o777, Symlink),
            ("w/../escape", b"no", 0o644, Regular),
            ("/w/abs", b"no", 0o644, Regular),
            ("h/home", b"no", 0o644, Regular),
            ("w", b"no", 0o644, Regular),
            ("w/empty", b"", 0, Regular),
        ]);
        let mut zip = convert(&tar);
        let names: Vec<String> = zip.file_names().map(|name| name.unwrap().into_owned()).collect();
        assert_eq!(names, ["a.txt", "nested/deep/run.sh", "empty"]);
        let mut text = String::new();
        zip.by_name("nested/deep/run.sh").unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "#!/bin/sh\necho hi\n");
        assert_eq!(zip.by_name("nested/deep/run.sh").unwrap().unix_mode(), Some(0o100_755), "set-user-ID is dropped");
        assert_eq!(zip.by_name("empty").unwrap().unix_mode(), Some(0o100_644));
        assert_eq!(zip.by_name("a.txt").unwrap().size(), 2);
    }

    #[test]
    fn many_entries_use_zip64_end_records() {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        for index in 0..70_000 {
            let mut header = tar::Header::new_gnu();
            header.set_size(1);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, format!("w/f/{index}"), &b"x"[..]).unwrap();
        }
        let tar = builder.into_inner().unwrap().finish().unwrap();
        let mut zip = convert(&tar);
        assert_eq!(zip.len(), 70_000);
        let mut text = String::new();
        zip.by_name("f/69999").unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "x");
    }

    #[test]
    fn an_empty_or_broken_archive() {
        assert_eq!(convert(&tar_gz(&[])).len(), 0);
        let mut broken = tar_gz(&[("w/a", b"0123456789", 0o644, tar::EntryType::Regular)]);
        broken.truncate(broken.len() - 8);
        assert!(to_zip(Cursor::new(&broken), Vec::new()).is_err());
        assert!(to_tar_gz(Cursor::new(&broken), Vec::new()).is_err());
    }

    #[test]
    fn tar_gz_downloads_have_the_files_at_their_top() {
        use tar::EntryType::{Regular, Symlink};
        let stored = tar_gz(&[("w/dist/a.zip", b"zip", 0o6755, Regular), ("w/link", b"", 0o777, Symlink), ("h/x", b"no", 0o644, Regular)]);
        let bytes = to_tar_gz(Cursor::new(stored), Vec::new()).unwrap();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(bytes)));
        let entries: Vec<_> = archive
            .entries()
            .unwrap()
            .map(|entry| {
                let mut entry = entry.unwrap();
                let mut text = String::new();
                entry.read_to_string(&mut text).unwrap();
                (entry.path().unwrap().display().to_string(), entry.header().mode().unwrap(), text)
            })
            .collect();
        assert_eq!(entries, [("dist/a.zip".to_owned(), 0o755, "zip".to_owned())]);
    }

    // Actions: run logs.
    #[test]
    fn files_become_a_zip() {
        let files = vec![("0_build.txt".to_owned(), b"compiling\n".to_vec()), ("1_test.txt".to_owned(), Vec::new())];
        let bytes = files_to_zip(&files, 1_700_000_000, Vec::new()).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), 2);
        let mut text = String::new();
        archive.by_name("0_build.txt").unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "compiling\n");
    }

    #[test]
    fn dos_times() {
        assert_eq!(dos_time(0), (0, 1 << 5 | 1), "before 1980 is 1980-01-01");
        // 2023-11-14T22:13:20Z
        assert_eq!(dos_time(1_700_000_000), ((22 << 11) + (13 << 5) + 10, (43 << 9) + (11 << 5) + 14));
    }
}
