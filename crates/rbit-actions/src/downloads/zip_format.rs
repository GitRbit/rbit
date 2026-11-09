//! The zip format of downloads: entries with data descriptors, ZIP64 where needed, and
//! the central directory.

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
pub(super) struct Written {
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
pub(super) struct Counter<W> {
    pub(super) inner: W,
    pub(super) count: u64,
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

// Actions: run logs.
/// A zip written to `out` one file at a time, as the files become available, so only one
/// is read at once.
pub struct ZipWriter<W: Write> {
    out: Counter<W>,
    written: Vec<Written>,
    dos: (u16, u16),
}

impl<W: Write> std::fmt::Debug for ZipWriter<W> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ZipWriter").field("files", &self.written.len()).field("bytes", &self.out.count).finish_non_exhaustive()
    }
}

impl<W: Write> ZipWriter<W> {
    /// A zip whose files all changed at `mtime` (Unix seconds).
    pub fn new(out: W, mtime: u64) -> Self {
        Self { out: Counter { inner: out, count: 0 }, written: Vec::new(), dos: dos_time(mtime) }
    }

    /// Adds file `name`: exactly `size` bytes of `data`.
    ///
    /// # Errors
    ///
    /// What reading `data` or writing fails with, and `data` ending before `size` bytes.
    pub fn add(&mut self, name: &str, size: u64, data: &mut impl Read) -> io::Result<()> {
        let entry = write_entry(&mut self.out, name.as_bytes().to_vec(), size, 0o644, self.dos, data)?;
        self.written.push(entry);
        Ok(())
    }

    /// Writes the central directory and returns `out`.
    ///
    /// # Errors
    ///
    /// What writing fails with.
    pub fn finish(mut self) -> io::Result<W> {
        write_directory(&mut self.out, &self.written)?;
        self.out.flush()?;
        Ok(self.out.inner)
    }
}

pub(super) fn write_entry<W: Write>(
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

pub(super) fn write_directory<W: Write>(out: &mut Counter<W>, entries: &[Written]) -> io::Result<()> {
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
pub(super) fn dos_time(unix: u64) -> (u16, u16) {
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
